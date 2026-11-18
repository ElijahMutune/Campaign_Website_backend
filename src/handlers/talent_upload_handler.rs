use axum::{
    Json,
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode},
};
use image::{ImageFormat, ImageReader};
use serde_json::json;
use std::{env, io::Cursor, path::PathBuf};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::{handlers::admin_handler::AdminApiError, state::AppState};

const MAX_ATTACHMENT_BYTES: usize = 20 * 1024 * 1024;
const MAX_ATTACHMENTS: i64 = 5;
const MAX_IMAGE_PIXELS: u64 = 40_000_000;
const MAX_IMAGE_DIMENSION: u32 = 12_000;

fn api_error(status: StatusCode, message: &str) -> (StatusCode, Json<AdminApiError>) {
    (
        status,
        Json(AdminApiError {
            success: false,
            message: message.to_string(),
        }),
    )
}

fn clean_filename(filename: &str) -> String {
    let basename = filename
        .rsplit(|character| character == '/' || character == '\\')
        .next()
        .unwrap_or("attachment");

    let cleaned: String = basename
        .chars()
        .filter(|character| !character.is_control())
        .take(250)
        .collect();

    if cleaned.is_empty() {
        "attachment".to_string()
    } else {
        cleaned
    }
}

// Validate actual file content, not only the supplied MIME type.
fn validate_attachment(
    bytes: &[u8],
    supplied_mime: &str,
) -> Result<(&'static str, &'static str, &'static str), String> {
    match supplied_mime {
        "image/jpeg" | "image/png" | "image/webp" => {
            let reader = ImageReader::new(Cursor::new(bytes))
                .with_guessed_format()
                .map_err(|_| "Unable to identify image format".to_string())?;

            let format = reader
                .format()
                .ok_or_else(|| "Unknown image format".to_string())?;

            let (mime, extension) = match format {
                ImageFormat::Jpeg => ("image/jpeg", "jpg"),
                ImageFormat::Png => ("image/png", "png"),
                ImageFormat::WebP => ("image/webp", "webp"),
                _ => return Err("Unsupported image format".to_string()),
            };

            if mime != supplied_mime {
                return Err("Image content does not match its MIME type".to_string());
            }

            let (width, height) = reader
                .into_dimensions()
                .map_err(|_| "Invalid image file".to_string())?;

            if width == 0
                || height == 0
                || width > MAX_IMAGE_DIMENSION
                || height > MAX_IMAGE_DIMENSION
                || (width as u64 * height as u64) > MAX_IMAGE_PIXELS
            {
                return Err("Image dimensions exceed permitted limits".to_string());
            }

            image::load_from_memory_with_format(bytes, format)
                .map_err(|_| "Image is corrupt or invalid".to_string())?;

            Ok((mime, "image", extension))
        }

        "video/mp4" => {
            if bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
                return Err("Invalid MP4 video file".to_string());
            }

            Ok(("video/mp4", "video", "mp4"))
        }

        "video/webm" => {
            if !bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
                return Err("Invalid WebM video file".to_string());
            }

            Ok(("video/webm", "video", "webm"))
        }

        _ => Err("Only JPEG, PNG, WebP, MP4 and WebM files are accepted".to_string()),
    }
}

// PUBLIC: UPLOAD PRIVATE TALENT APPLICATION ATTACHMENT
//
// The upload token acts as a secret credential.
// It is required together with the application reference.
pub async fn upload_talent_attachment(
    Path(application_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<AdminApiError>)> {
    let token_value = headers
        .get("x-talent-upload-token")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| api_error(StatusCode::UNAUTHORIZED, "Talent upload token is required."))?;

    let upload_token = Uuid::parse_str(token_value)
        .map_err(|_| api_error(StatusCode::UNAUTHORIZED, "Invalid talent upload token."))?;

    // Read the single uploaded file.
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut original_filename = String::from("attachment");
    let mut supplied_mime = String::new();

    while let Some(field) = multipart.next_field().await.map_err(|error| {
        eprintln!("Talent multipart error: {error}");
        api_error(
            StatusCode::BAD_REQUEST,
            "Invalid attachment upload request.",
        )
    })? {
        if field.name() != Some("file") || file_bytes.is_some() {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "Exactly one file field named 'file' is required.",
            ));
        }

        original_filename = field.file_name().unwrap_or("attachment").to_string();

        supplied_mime = field.content_type().unwrap_or("").to_string();

        let bytes = field.bytes().await.map_err(|error| {
            eprintln!("Talent attachment reading error: {error}");
            api_error(StatusCode::BAD_REQUEST, "Unable to read attachment.")
        })?;

        if bytes.is_empty() {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "Uploaded attachment is empty.",
            ));
        }

        if bytes.len() > MAX_ATTACHMENT_BYTES {
            return Err(api_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Each attachment must not exceed 20 MB.",
            ));
        }

        file_bytes = Some(bytes.to_vec());
    }

    let bytes = file_bytes
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "An attachment file is required."))?;

    let (mime_type, media_type, extension) = validate_attachment(&bytes, &supplied_mime)
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, &error))?;

    // Lock the application row and verify the secret token.
    // Only active application statuses can receive attachments.
    let mut tx = state.db.begin().await.map_err(|error| {
        eprintln!("Talent upload transaction error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to prepare attachment upload.",
        )
    })?;

    let application_status: Option<String> = sqlx::query_scalar(
        r#"
        SELECT status
        FROM talent_applications
        WHERE id = $1
          AND attachment_upload_token = $2
        FOR UPDATE
        "#,
    )
    .bind(application_id)
    .bind(upload_token)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| {
        eprintln!("Talent upload authorization error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to verify application upload permission.",
        )
    })?;

    let application_status = application_status.ok_or_else(|| {
        api_error(
            StatusCode::NOT_FOUND,
            "Application reference or upload token is invalid.",
        )
    })?;

    if !matches!(
        application_status.as_str(),
        "new" | "under_review" | "contacted"
    ) {
        return Err(api_error(
            StatusCode::CONFLICT,
            "This application is no longer accepting attachments.",
        ));
    }

    // Enforce five attachments per application.
    let attachment_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM talent_application_media
        WHERE application_id = $1
        "#,
    )
    .bind(application_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| {
        eprintln!("Talent attachment count error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to check attachment limit.",
        )
    })?;

    if attachment_count >= MAX_ATTACHMENTS {
        return Err(api_error(
            StatusCode::CONFLICT,
            "This application has reached its limit of five attachments.",
        ));
    }

    let storage_dir = env::var("MEDIA_STORAGE_DIR").unwrap_or_else(|_| "uploads/media".to_string());

    let storage_path = PathBuf::from(storage_dir);

    tokio::fs::create_dir_all(&storage_path)
        .await
        .map_err(|error| {
            eprintln!("Talent storage directory error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to prepare attachment storage.",
            )
        })?;

    let stored_filename = format!("{}.{}", Uuid::new_v4(), extension);
    let full_path = storage_path.join(&stored_filename);
    let file_path = full_path.to_string_lossy().to_string();

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&full_path)
        .await
        .map_err(|error| {
            eprintln!("Talent attachment storage error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to create unique attachment file.",
            )
        })?;

    if let Err(error) = file.write_all(&bytes).await {
        eprintln!("Talent attachment write error: {error}");
        drop(file);
        let _ = tokio::fs::remove_file(&full_path).await;

        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to save attachment.",
        ));
    }

    drop(file);

    let original_filename = clean_filename(&original_filename);
    let file_size = bytes.len() as i64;

    // Save the media record inside the same database transaction.
    let media_result = sqlx::query_scalar::<_, Uuid>(
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
        VALUES ($1, $2, $3, $4, $5, $6, NULL)
        RETURNING id
        "#,
    )
    .bind(&original_filename)
    .bind(&stored_filename)
    .bind(&file_path)
    .bind(mime_type)
    .bind(file_size)
    .bind(media_type)
    .fetch_one(&mut *tx)
    .await;

    let media_id = match media_result {
        Ok(id) => id,
        Err(error) => {
            eprintln!("Talent media registration error: {error}");
            let _ = tokio::fs::remove_file(&full_path).await;

            return Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to register attachment.",
            ));
        }
    };

    let association_result = sqlx::query(
        r#"
        INSERT INTO talent_application_media (
            application_id,
            media_id
        )
        VALUES ($1, $2)
        "#,
    )
    .bind(application_id)
    .bind(media_id)
    .execute(&mut *tx)
    .await;

    if let Err(error) = association_result {
        eprintln!("Talent attachment association error: {error}");
        let _ = tokio::fs::remove_file(&full_path).await;

        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to attach file to the application.",
        ));
    }

    if let Err(error) = tx.commit().await {
        eprintln!("Talent attachment commit error: {error}");

        // Keep the file if commit outcome is uncertain.
        // Removing it could leave a committed media record without its file.
        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to complete attachment registration.",
        ));
    }

    // Do not return storage paths or the upload token.
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "success": true,
            "message": "Private attachment uploaded successfully.",
            "application_reference": application_id,
            "media_id": media_id,
            "media_type": media_type,
            "file_size": file_size
        })),
    ))
}
