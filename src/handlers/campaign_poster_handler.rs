use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::Response,
};
use serde::Deserialize;
use sqlx::FromRow;
use std::{env, path::PathBuf};
use uuid::Uuid;

use crate::{
    handlers::admin_handler::AdminApiError, models::campaign_poster::CampaignPoster,
    state::AppState,
};

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

fn validate_fields(
    title: &str,
    display_order: i32,
) -> Result<(), (StatusCode, Json<AdminApiError>)> {
    if title.is_empty() || title.chars().count() > 255 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Poster title is required and must not exceed 255 characters.",
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

async fn validate_category(
    state: &AppState,
    category_id: Option<Uuid>,
) -> Result<(), (StatusCode, Json<AdminApiError>)> {
    let Some(category_id) = category_id else {
        return Ok(());
    };

    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM poster_categories WHERE id = $1)")
            .bind(category_id)
            .fetch_one(&state.db)
            .await
            .map_err(|error| {
                eprintln!("Poster category validation error: {error}");
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Unable to validate poster category.",
                )
            })?;

    if !exists {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Selected poster category does not exist.",
        ));
    }

    Ok(())
}

async fn validate_poster_image(
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
              AND media_type = 'image'
              AND deleted_at IS NULL
        )
        "#,
    )
    .bind(media_id)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Poster media validation error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to validate selected poster image.",
        )
    })?;

    if !exists {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Selected file must be an active image in the media library.",
        ));
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct CreatePosterRequest {
    pub title: String,
    pub description: Option<String>,
    pub category_id: Option<Uuid>,
    pub preview_media_id: Option<Uuid>,
    pub download_media_id: Option<Uuid>,
    #[serde(default)]
    pub display_order: i32,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePosterRequest {
    pub title: String,
    pub description: Option<String>,
    pub category_id: Option<Uuid>,
    pub preview_media_id: Option<Uuid>,
    pub download_media_id: Option<Uuid>,
    pub display_order: i32,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePosterPublicationRequest {
    pub is_published: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePosterOrderRequest {
    pub display_order: i32,
}

// ADMIN: CREATE POSTER
pub async fn create_poster(
    State(state): State<AppState>,
    Json(payload): Json<CreatePosterRequest>,
) -> Result<(StatusCode, Json<CampaignPoster>), (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();
    let description = clean_optional(payload.description);

    validate_fields(&title, payload.display_order)?;

    validate_category(&state, payload.category_id).await?;
    validate_poster_image(&state, payload.preview_media_id).await?;
    validate_poster_image(&state, payload.download_media_id).await?;

    let poster = sqlx::query_as::<_, CampaignPoster>(
        r#"
        INSERT INTO campaign_posters (
            category_id,
            title,
            description,
            preview_media_id,
            download_media_id,
            is_published,
            download_count,
            published_at,
            display_order
        )
        VALUES ($1, $2, $3, $4, $5, FALSE, 0, NULL, $6)
        RETURNING *
        "#,
    )
    .bind(payload.category_id)
    .bind(&title)
    .bind(&description)
    .bind(payload.preview_media_id)
    .bind(payload.download_media_id)
    .bind(payload.display_order)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Poster creation error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to create campaign poster.",
        )
    })?;

    Ok((StatusCode::CREATED, Json(poster)))
}

// ADMIN: LIST ALL POSTERS
pub async fn list_posters(
    State(state): State<AppState>,
) -> Result<Json<Vec<CampaignPoster>>, (StatusCode, Json<AdminApiError>)> {
    let posters = sqlx::query_as::<_, CampaignPoster>(
        r#"
        SELECT *
        FROM campaign_posters
        ORDER BY display_order ASC, created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Poster listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve campaign posters.",
        )
    })?;

    Ok(Json(posters))
}

// ADMIN: GET ONE POSTER
pub async fn get_poster(
    State(state): State<AppState>,
    Path(poster_id): Path<Uuid>,
) -> Result<Json<CampaignPoster>, (StatusCode, Json<AdminApiError>)> {
    let poster =
        sqlx::query_as::<_, CampaignPoster>("SELECT * FROM campaign_posters WHERE id = $1")
            .bind(poster_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| {
                eprintln!("Poster retrieval error: {error}");
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Unable to retrieve campaign poster.",
                )
            })?
            .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Campaign poster not found."))?;

    Ok(Json(poster))
}

// ADMIN: UPDATE POSTER
pub async fn update_poster(
    State(state): State<AppState>,
    Path(poster_id): Path<Uuid>,
    Json(payload): Json<UpdatePosterRequest>,
) -> Result<Json<CampaignPoster>, (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();
    let description = clean_optional(payload.description);

    validate_fields(&title, payload.display_order)?;

    validate_category(&state, payload.category_id).await?;
    validate_poster_image(&state, payload.preview_media_id).await?;
    validate_poster_image(&state, payload.download_media_id).await?;

    let poster = sqlx::query_as::<_, CampaignPoster>(
        r#"
        UPDATE campaign_posters
        SET
            category_id = $2,
            title = $3,
            description = $4,
            preview_media_id = $5,
            download_media_id = $6,
            display_order = $7,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(poster_id)
    .bind(payload.category_id)
    .bind(&title)
    .bind(&description)
    .bind(payload.preview_media_id)
    .bind(payload.download_media_id)
    .bind(payload.display_order)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Poster update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update campaign poster.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Campaign poster not found."))?;

    Ok(Json(poster))
}

// ADMIN: PUBLISH OR UNPUBLISH POSTER
pub async fn update_poster_publication(
    State(state): State<AppState>,
    Path(poster_id): Path<Uuid>,
    Json(payload): Json<UpdatePosterPublicationRequest>,
) -> Result<Json<CampaignPoster>, (StatusCode, Json<AdminApiError>)> {
    let poster = sqlx::query_as::<_, CampaignPoster>(
        r#"
        UPDATE campaign_posters
        SET
            is_published = $2,
            published_at = CASE
                WHEN $2 = TRUE THEN COALESCE(published_at, NOW())
                ELSE NULL
            END,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(poster_id)
    .bind(payload.is_published)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Poster publication error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update poster publication.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Campaign poster not found."))?;

    Ok(Json(poster))
}

// ADMIN: UPDATE DISPLAY ORDER
pub async fn update_poster_order(
    State(state): State<AppState>,
    Path(poster_id): Path<Uuid>,
    Json(payload): Json<UpdatePosterOrderRequest>,
) -> Result<Json<CampaignPoster>, (StatusCode, Json<AdminApiError>)> {
    if !(0..=1_000_000).contains(&payload.display_order) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Display order must be between 0 and 1000000.",
        ));
    }

    let poster = sqlx::query_as::<_, CampaignPoster>(
        r#"
        UPDATE campaign_posters
        SET
            display_order = $2,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(poster_id)
    .bind(payload.display_order)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Poster order update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update poster display order.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Campaign poster not found."))?;

    Ok(Json(poster))
}

// ADMIN: DELETE POSTER
// Associated media files are retained in the media library.
pub async fn delete_poster(
    State(state): State<AppState>,
    Path(poster_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<AdminApiError>)> {
    let result = sqlx::query("DELETE FROM campaign_posters WHERE id = $1")
        .bind(poster_id)
        .execute(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Poster deletion error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to delete campaign poster.",
            )
        })?;

    if result.rows_affected() == 0 {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Campaign poster not found.",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

// PUBLIC: LIST PUBLISHED POSTERS
pub async fn list_public_posters(
    State(state): State<AppState>,
) -> Result<Json<Vec<CampaignPoster>>, (StatusCode, Json<AdminApiError>)> {
    let posters = sqlx::query_as::<_, CampaignPoster>(
        r#"
        SELECT *
        FROM campaign_posters
        WHERE is_published = TRUE
          AND published_at IS NOT NULL
        ORDER BY display_order ASC, created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public poster listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve published posters.",
        )
    })?;

    Ok(Json(posters))
}

// PUBLIC: GET ONE PUBLISHED POSTER
pub async fn get_public_poster(
    State(state): State<AppState>,
    Path(poster_id): Path<Uuid>,
) -> Result<Json<CampaignPoster>, (StatusCode, Json<AdminApiError>)> {
    let poster = sqlx::query_as::<_, CampaignPoster>(
        r#"
        SELECT *
        FROM campaign_posters
        WHERE id = $1
          AND is_published = TRUE
          AND published_at IS NOT NULL
        "#,
    )
    .bind(poster_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public poster retrieval error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve published poster.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Published poster not found."))?;

    Ok(Json(poster))
}

#[derive(Debug, FromRow)]
struct PosterStoredFile {
    media_id: Uuid,
    stored_filename: String,
    mime_type: String,
}

fn extension_for_mime(mime_type: &str) -> Option<&'static str> {
    match mime_type {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

// Both public preview and download use this function.
// The requested image must belong to an active, published poster.
async fn serve_poster_file(
    state: AppState,
    poster_id: Uuid,
    download: bool,
) -> Result<Response, StatusCode> {
    // The column name is selected only from these two fixed values.
    let media_column = if download {
        "download_media_id"
    } else {
        "preview_media_id"
    };

    let sql = format!(
        r#"
        SELECT
            m.id AS media_id,
            m.stored_filename,
            m.mime_type
        FROM campaign_posters p
        INNER JOIN media m ON m.id = p.{media_column}
        WHERE p.id = $1
          AND p.is_published = TRUE
          AND p.published_at IS NOT NULL
          AND m.media_type = 'image'
          AND m.deleted_at IS NULL
        "#
    );

    let media = sqlx::query_as::<_, PosterStoredFile>(&sql)
        .bind(poster_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Public poster file query error: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let expected_extension =
        extension_for_mime(&media.mime_type).ok_or(StatusCode::UNSUPPORTED_MEDIA_TYPE)?;

    // The stored filename must consist of a UUID and its expected extension.
    let (filename_id, extension) = media
        .stored_filename
        .rsplit_once('.')
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    Uuid::parse_str(filename_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if extension != expected_extension {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let storage_dir = env::var("MEDIA_STORAGE_DIR").unwrap_or_else(|_| "uploads/media".to_string());

    let file_path = PathBuf::from(storage_dir).join(&media.stored_filename);

    let bytes = tokio::fs::read(&file_path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            StatusCode::NOT_FOUND
        } else {
            eprintln!("Poster file reading error: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    let disposition = if download { "attachment" } else { "inline" };

    let filename = format!("campaign-poster-{}.{}", poster_id, expected_extension);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, media.mime_type.clone())
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CACHE_CONTROL, "public, no-store")
        .header(
            header::CONTENT_DISPOSITION,
            format!("{disposition}; filename=\"{filename}\""),
        )
        .body(Body::from(bytes))
        .map_err(|error| {
            eprintln!("Poster file response error: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Count only successfully read and prepared downloads.
    // The condition also prevents counting a poster unpublished during the request.
    if download {
        let result = sqlx::query(
            r#"
            UPDATE campaign_posters
            SET download_count = download_count + 1
            WHERE id = $1
              AND download_media_id = $2
              AND is_published = TRUE
              AND published_at IS NOT NULL
            "#,
        )
        .bind(poster_id)
        .bind(media.media_id)
        .execute(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Poster download count update error: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        if result.rows_affected() == 0 {
            return Err(StatusCode::NOT_FOUND);
        }
    }

    Ok(response)
}

// PUBLIC: VIEW POSTER PREVIEW
pub async fn serve_public_poster_preview(
    Path(poster_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    serve_poster_file(state, poster_id, false).await
}

// PUBLIC: DOWNLOAD POSTER
pub async fn download_public_poster(
    Path(poster_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    serve_poster_file(state, poster_id, true).await
}
