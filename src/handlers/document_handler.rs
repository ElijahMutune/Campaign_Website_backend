use axum::{
    Json,
    body::Body,
    extract::{Extension, Multipart, Path, State},
    http::{StatusCode, header},
    response::Response,
};
use serde::Deserialize;
use std::{env, path::PathBuf};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::{
    handlers::admin_handler::AdminApiError,
    models::{document::CampaignDocument, media::MediaRecord, user::AuthenticatedUser},
    state::AppState,
};

const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;

fn api_error(status: StatusCode, message: &str) -> (StatusCode, Json<AdminApiError>) {
    (
        status,
        Json(AdminApiError {
            success: false,
            message: message.to_string(),
        }),
    )
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn clean_original_filename(filename: &str) -> String {
    let basename = filename
        .rsplit(|character| character == '/' || character == '\\')
        .next()
        .unwrap_or("document.pdf");

    let cleaned: String = basename
        .chars()
        .filter(|character| !character.is_control())
        .take(250)
        .collect();

    if cleaned.is_empty() {
        "document.pdf".to_string()
    } else {
        cleaned
    }
}

fn validate_document_fields(
    title: &str,
    category: &Option<String>,
    display_order: i32,
) -> Result<(), (StatusCode, Json<AdminApiError>)> {
    if title.is_empty() || title.chars().count() > 255 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Document title is required and must not exceed 255 characters.",
        ));
    }

    if category
        .as_ref()
        .is_some_and(|value| value.chars().count() > 150)
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Document category must not exceed 150 characters.",
        ));
    }

    if !(0..=1_000_000).contains(&display_order) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Display order must be between 0 and 1000000.",
        ));
    }

    Ok(())
}

async fn validate_document_media(
    state: &AppState,
    media_id: Option<Uuid>,
) -> Result<(), (StatusCode, Json<AdminApiError>)> {
    let Some(media_id) = media_id else {
        return Ok(());
    };

    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM media
            WHERE id = $1
              AND media_type = 'document'
              AND mime_type = 'application/pdf'
              AND deleted_at IS NULL
        )
        "#,
    )
    .bind(media_id)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Document media validation error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to validate selected document file.",
        )
    })?;

    if !exists {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Selected file must be an active PDF document in the media library.",
        ));
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct CreateDocumentRequest {
    pub title: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub media_id: Option<Uuid>,
    #[serde(default)]
    pub display_order: i32,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDocumentRequest {
    pub title: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub media_id: Option<Uuid>,
    pub display_order: i32,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDocumentPublicationRequest {
    pub is_published: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDocumentOrderRequest {
    pub display_order: i32,
}

// -----------------------------------------------------------------------------
// DOCUMENT FILE UPLOAD
// -----------------------------------------------------------------------------
//
// ADMIN: UPLOAD PDF TO THE MEDIA LIBRARY
// This endpoint creates a unique UUID filename and never overwrites a file.
// Only PDF is accepted in this first document-upload implementation.

pub async fn upload_document_media(
    Extension(user): Extension<AuthenticatedUser>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<MediaRecord>), (StatusCode, Json<AdminApiError>)> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut original_filename: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(|error| {
        eprintln!("Document multipart error: {error}");
        api_error(StatusCode::BAD_REQUEST, "Invalid document upload request.")
    })? {
        let field_name = field.name().unwrap_or("").to_string();

        if field_name != "file" {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "Only the file field is supported.",
            ));
        }

        if file_bytes.is_some() {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "Only one PDF document can be uploaded per request.",
            ));
        }

        let filename = field.file_name().unwrap_or("document.pdf").to_string();

        if !filename.to_lowercase().ends_with(".pdf") {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "Only PDF files are accepted.",
            ));
        }

        if field.content_type() != Some("application/pdf") {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "The uploaded file must have application/pdf content type.",
            ));
        }

        let bytes = field.bytes().await.map_err(|error| {
            eprintln!("Document reading error: {error}");
            api_error(StatusCode::BAD_REQUEST, "Unable to read uploaded document.")
        })?;

        if bytes.is_empty() {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "Uploaded document is empty.",
            ));
        }

        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(api_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "PDF document must not exceed 8 MB.",
            ));
        }

        // Check the PDF file signature and its end marker.
        if !bytes.starts_with(b"%PDF-")
            || !bytes
                .windows(5)
                .rev()
                .take(2048)
                .any(|window| window == b"%%EOF")
        {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "The uploaded file does not appear to be a valid PDF.",
            ));
        }

        file_bytes = Some(bytes.to_vec());
        original_filename = Some(filename);
    }

    let file_bytes =
        file_bytes.ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "A PDF file is required."))?;

    let original_filename =
        clean_original_filename(original_filename.as_deref().unwrap_or("document.pdf"));

    let storage_dir = env::var("MEDIA_STORAGE_DIR").unwrap_or_else(|_| "uploads/media".to_string());

    let storage_path = PathBuf::from(&storage_dir);

    tokio::fs::create_dir_all(&storage_path)
        .await
        .map_err(|error| {
            eprintln!("Document storage directory error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to prepare document storage.",
            )
        })?;

    let stored_filename = format!("{}.pdf", Uuid::new_v4());
    let full_path = storage_path.join(&stored_filename);
    let file_path = full_path.to_string_lossy().to_string();

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&full_path)
        .await
        .map_err(|error| {
            eprintln!("Document storage error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to create unique document file.",
            )
        })?;

    if let Err(error) = file.write_all(&file_bytes).await {
        eprintln!("Document writing error: {error}");
        drop(file);
        let _ = tokio::fs::remove_file(&full_path).await;

        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to save document.",
        ));
    }

    drop(file);

    let file_size = file_bytes.len() as i64;

    let result = sqlx::query_as::<_, MediaRecord>(
        r#"
        INSERT INTO media (
            original_filename,
            stored_filename,
            file_path,
            mime_type,
            file_size,
            media_type,
            uploaded_by
        )
        VALUES ($1, $2, $3, 'application/pdf', $4, 'document', $5)
        RETURNING
            id,
            original_filename,
            mime_type,
            file_size,
            media_type,
            alt_text,
            caption,
            uploaded_by,
            created_at
        "#,
    )
    .bind(&original_filename)
    .bind(&stored_filename)
    .bind(&file_path)
    .bind(file_size)
    .bind(user.id)
    .fetch_one(&state.db)
    .await;

    match result {
        Ok(record) => Ok((StatusCode::CREATED, Json(record))),
        Err(error) => {
            eprintln!("Document media database error: {error}");

            if let Err(cleanup_error) = tokio::fs::remove_file(&full_path).await {
                eprintln!("Document cleanup error: {cleanup_error}");
            }

            Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to register document in the media library.",
            ))
        }
    }
}

// -----------------------------------------------------------------------------
// ADMIN DOCUMENT MANAGEMENT
// -----------------------------------------------------------------------------

// ADMIN: CREATE DOCUMENT RECORD
// New records are always unpublished.
pub async fn create_document(
    State(state): State<AppState>,
    Json(payload): Json<CreateDocumentRequest>,
) -> Result<(StatusCode, Json<CampaignDocument>), (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();
    let description = clean_optional(payload.description);
    let category = clean_optional(payload.category);

    validate_document_fields(&title, &category, payload.display_order)?;
    validate_document_media(&state, payload.media_id).await?;

    let document = sqlx::query_as::<_, CampaignDocument>(
        r#"
        INSERT INTO documents (
            title,
            description,
            media_id,
            category,
            display_order,
            is_published,
            published_at,
            download_count
        )
        VALUES ($1, $2, $3, $4, $5, FALSE, NULL, 0)
        RETURNING *
        "#,
    )
    .bind(&title)
    .bind(&description)
    .bind(payload.media_id)
    .bind(&category)
    .bind(payload.display_order)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Document creation error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to create document record.",
        )
    })?;

    Ok((StatusCode::CREATED, Json(document)))
}

// ADMIN: LIST ALL DOCUMENTS
pub async fn list_documents(
    State(state): State<AppState>,
) -> Result<Json<Vec<CampaignDocument>>, (StatusCode, Json<AdminApiError>)> {
    let documents = sqlx::query_as::<_, CampaignDocument>(
        r#"
        SELECT *
        FROM documents
        ORDER BY display_order ASC, created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Document listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve documents.",
        )
    })?;

    Ok(Json(documents))
}

// ADMIN: GET ONE DOCUMENT
pub async fn get_document(
    State(state): State<AppState>,
    Path(document_id): Path<Uuid>,
) -> Result<Json<CampaignDocument>, (StatusCode, Json<AdminApiError>)> {
    let document = sqlx::query_as::<_, CampaignDocument>("SELECT * FROM documents WHERE id = $1")
        .bind(document_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Document retrieval error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to retrieve document.",
            )
        })?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Document not found."))?;

    Ok(Json(document))
}

// ADMIN: UPDATE DOCUMENT
pub async fn update_document(
    State(state): State<AppState>,
    Path(document_id): Path<Uuid>,
    Json(payload): Json<UpdateDocumentRequest>,
) -> Result<Json<CampaignDocument>, (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();
    let description = clean_optional(payload.description);
    let category = clean_optional(payload.category);

    validate_document_fields(&title, &category, payload.display_order)?;
    validate_document_media(&state, payload.media_id).await?;

    let document = sqlx::query_as::<_, CampaignDocument>(
        r#"
        UPDATE documents
        SET
            title = $2,
            description = $3,
            category = $4,
            media_id = $5,
            display_order = $6,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(document_id)
    .bind(&title)
    .bind(&description)
    .bind(&category)
    .bind(payload.media_id)
    .bind(payload.display_order)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Document update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update document.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Document not found."))?;

    Ok(Json(document))
}

// ADMIN: PUBLISH OR UNPUBLISH DOCUMENT
pub async fn update_document_publication(
    State(state): State<AppState>,
    Path(document_id): Path<Uuid>,
    Json(payload): Json<UpdateDocumentPublicationRequest>,
) -> Result<Json<CampaignDocument>, (StatusCode, Json<AdminApiError>)> {
    if payload.is_published {
        let media_id: Option<Option<Uuid>> =
            sqlx::query_scalar("SELECT media_id FROM documents WHERE id = $1")
                .bind(document_id)
                .fetch_optional(&state.db)
                .await
                .map_err(|error| {
                    eprintln!("Document publication lookup error: {error}");
                    api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Unable to verify document file.",
                    )
                })?;

        match media_id {
            None => {
                return Err(api_error(StatusCode::NOT_FOUND, "Document not found."));
            }
            Some(None) => {
                return Err(api_error(
                    StatusCode::CONFLICT,
                    "A document file must be attached before publication.",
                ));
            }
            Some(Some(media_id)) => {
                validate_document_media(&state, Some(media_id)).await?;
            }
        }
    }

    let document = sqlx::query_as::<_, CampaignDocument>(
        r#"
        UPDATE documents
        SET
            is_published = $2,
            published_at = CASE
                WHEN $2 THEN COALESCE(published_at, NOW())
                ELSE NULL
            END,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(document_id)
    .bind(payload.is_published)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Document publication error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update document publication.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Document not found."))?;

    Ok(Json(document))
}

// ADMIN: CHANGE DOCUMENT DISPLAY ORDER
pub async fn update_document_order(
    State(state): State<AppState>,
    Path(document_id): Path<Uuid>,
    Json(payload): Json<UpdateDocumentOrderRequest>,
) -> Result<Json<CampaignDocument>, (StatusCode, Json<AdminApiError>)> {
    if !(0..=1_000_000).contains(&payload.display_order) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Display order must be between 0 and 1000000.",
        ));
    }

    let document = sqlx::query_as::<_, CampaignDocument>(
        r#"
        UPDATE documents
        SET
            display_order = $2,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(document_id)
    .bind(payload.display_order)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Document order update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update document order.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Document not found."))?;

    Ok(Json(document))
}

// ADMIN: DELETE DOCUMENT RECORD
// The original media file remains in the media library.
pub async fn delete_document(
    State(state): State<AppState>,
    Path(document_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<AdminApiError>)> {
    let result = sqlx::query("DELETE FROM documents WHERE id = $1")
        .bind(document_id)
        .execute(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Document deletion error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to delete document.",
            )
        })?;

    if result.rows_affected() == 0 {
        return Err(api_error(StatusCode::NOT_FOUND, "Document not found."));
    }

    Ok(StatusCode::NO_CONTENT)
}

// -----------------------------------------------------------------------------
// PUBLIC DOCUMENT ACCESS
// -----------------------------------------------------------------------------

// PUBLIC: LIST PUBLISHED DOCUMENTS
pub async fn list_public_documents(
    State(state): State<AppState>,
) -> Result<Json<Vec<CampaignDocument>>, (StatusCode, Json<AdminApiError>)> {
    let documents = sqlx::query_as::<_, CampaignDocument>(
        r#"
        SELECT d.*
        FROM documents d
        WHERE d.is_published = TRUE
          AND d.published_at IS NOT NULL
          AND EXISTS (
              SELECT 1
              FROM media m
              WHERE m.id = d.media_id
                AND m.media_type = 'document'
                AND m.mime_type = 'application/pdf'
                AND m.deleted_at IS NULL
          )
        ORDER BY d.display_order ASC, d.created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public document listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve published documents.",
        )
    })?;

    Ok(Json(documents))
}

// PUBLIC: GET ONE PUBLISHED DOCUMENT
pub async fn get_public_document(
    State(state): State<AppState>,
    Path(document_id): Path<Uuid>,
) -> Result<Json<CampaignDocument>, (StatusCode, Json<AdminApiError>)> {
    let document = sqlx::query_as::<_, CampaignDocument>(
        r#"
        SELECT d.*
        FROM documents d
        WHERE d.id = $1
          AND d.is_published = TRUE
          AND d.published_at IS NOT NULL
          AND EXISTS (
              SELECT 1
              FROM media m
              WHERE m.id = d.media_id
                AND m.media_type = 'document'
                AND m.mime_type = 'application/pdf'
                AND m.deleted_at IS NULL
          )
        "#,
    )
    .bind(document_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public document retrieval error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve published document.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Published document not found."))?;

    Ok(Json(document))
}

#[derive(sqlx::FromRow)]
struct StoredDocumentFile {
    media_id: Uuid,
    stored_filename: String,
    mime_type: String,
}

// PUBLIC: DOWNLOAD A PUBLISHED PDF
// The file must be attached to the requested published document.
pub async fn download_public_document(
    State(state): State<AppState>,
    Path(document_id): Path<Uuid>,
) -> Result<Response, StatusCode> {
    let media = sqlx::query_as::<_, StoredDocumentFile>(
        r#"
        SELECT
            m.id AS media_id,
            m.stored_filename,
            m.mime_type
        FROM documents d
        INNER JOIN media m ON m.id = d.media_id
        WHERE d.id = $1
          AND d.is_published = TRUE
          AND d.published_at IS NOT NULL
          AND m.media_type = 'document'
          AND m.mime_type = 'application/pdf'
          AND m.deleted_at IS NULL
        "#,
    )
    .bind(document_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public document file lookup error: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let (filename_id, extension) = media
        .stored_filename
        .rsplit_once('.')
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    Uuid::parse_str(filename_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if extension != "pdf" {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    let storage_dir = env::var("MEDIA_STORAGE_DIR").unwrap_or_else(|_| "uploads/media".to_string());

    let file_path = PathBuf::from(storage_dir).join(&media.stored_filename);

    let bytes = tokio::fs::read(&file_path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            StatusCode::NOT_FOUND
        } else {
            eprintln!("Public document file read error: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, media.mime_type.clone())
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CACHE_CONTROL, "public, no-store")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"document-{document_id}.pdf\""),
        )
        .body(Body::from(bytes))
        .map_err(|error| {
            eprintln!("Public document response error: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let result = sqlx::query(
        r#"
        UPDATE documents
        SET download_count = download_count + 1
        WHERE id = $1
          AND media_id = $2
          AND is_published = TRUE
          AND published_at IS NOT NULL
        "#,
    )
    .bind(document_id)
    .bind(media.media_id)
    .execute(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Document download count update error: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(response)
}
