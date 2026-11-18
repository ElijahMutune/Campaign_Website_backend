use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    handlers::admin_handler::AdminApiError,
    models::{media::MediaRecord, media_album::MediaAlbum},
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

// Album listing response with the number of images.
#[derive(Debug, FromRow, Serialize)]
pub struct MediaAlbumSummary {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub is_published: bool,
    pub display_order: i32,
    pub media_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

use sqlx::FromRow;

#[derive(Debug, Serialize)]
pub struct MediaAlbumDetail {
    pub album: MediaAlbum,
    pub media: Vec<MediaRecord>,
}

// Create album request.
#[derive(Debug, Deserialize)]
pub struct CreateAlbumRequest {
    pub title: String,
    pub description: Option<String>,
    pub display_order: Option<i32>,
}

// Update album request.
#[derive(Debug, Deserialize)]
pub struct UpdateAlbumRequest {
    pub title: String,
    pub description: Option<String>,
    pub is_published: bool,
    pub display_order: i32,
}

// Validate album information.
fn validate_album(
    title: &str,
    description: &Option<String>,
    display_order: i32,
) -> Result<(), String> {
    if title.trim().is_empty() || title.chars().count() > 255 {
        return Err("Album title is required and must not exceed 255 characters.".to_string());
    }

    if description
        .as_ref()
        .is_some_and(|value| value.chars().count() > 10000)
    {
        return Err("Album description is too long.".to_string());
    }

    if !(0..=1_000_000).contains(&display_order) {
        return Err("Invalid album display order.".to_string());
    }

    Ok(())
}

// CREATE ALBUM
pub async fn create_album(
    State(state): State<AppState>,
    Json(payload): Json<CreateAlbumRequest>,
) -> Result<(StatusCode, Json<MediaAlbum>), (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();

    let description = payload
        .description
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let display_order = payload.display_order.unwrap_or(0);

    validate_album(&title, &description, display_order)
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, &error))?;

    // New albums are private drafts by default.
    let album = sqlx::query_as::<_, MediaAlbum>(
        r#"
        INSERT INTO media_albums (
            title,
            description,
            is_published,
            display_order
        )
        VALUES ($1, $2, FALSE, $3)
        RETURNING
            id,
            title,
            description,
            is_published,
            display_order,
            created_at,
            updated_at
        "#,
    )
    .bind(&title)
    .bind(&description)
    .bind(display_order)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Album creation error: {error}");

        api_error(StatusCode::INTERNAL_SERVER_ERROR, "Unable to create album.")
    })?;

    Ok((StatusCode::CREATED, Json(album)))
}

// LIST ALBUMS
pub async fn list_albums(
    State(state): State<AppState>,
) -> Result<Json<Vec<MediaAlbumSummary>>, (StatusCode, Json<AdminApiError>)> {
    let albums = sqlx::query_as::<_, MediaAlbumSummary>(
        r#"
        SELECT
            a.id,
            a.title,
            a.description,
            a.is_published,
            a.display_order,
            COUNT(m.id)::BIGINT AS media_count,
            a.created_at,
            a.updated_at
        FROM media_albums a
        LEFT JOIN album_media am
            ON am.album_id = a.id
        LEFT JOIN media m
            ON m.id = am.media_id
            AND m.deleted_at IS NULL
        GROUP BY a.id
        ORDER BY a.display_order ASC, a.created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Album listing error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve albums.",
        )
    })?;

    Ok(Json(albums))
}

// VIEW ALBUM AND ITS IMAGES
pub async fn get_album(
    State(state): State<AppState>,
    Path(album_id): Path<Uuid>,
) -> Result<Json<MediaAlbumDetail>, (StatusCode, Json<AdminApiError>)> {
    let album = sqlx::query_as::<_, MediaAlbum>(
        r#"
        SELECT
            id,
            title,
            description,
            is_published,
            display_order,
            created_at,
            updated_at
        FROM media_albums
        WHERE id = $1
        "#,
    )
    .bind(album_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Album retrieval error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve album.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Album not found."))?;

    let media = sqlx::query_as::<_, MediaRecord>(
        r#"
        SELECT
            m.id,
            m.original_filename,
            m.mime_type,
            m.file_size,
            m.media_type,
            m.alt_text,
            m.caption,
            m.uploaded_by,
            m.created_at
        FROM media m
        INNER JOIN album_media am
            ON am.media_id = m.id
        WHERE am.album_id = $1
        AND m.deleted_at IS NULL
        ORDER BY am.display_order ASC, m.created_at DESC
        "#,
    )
    .bind(album_id)
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Album media retrieval error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve album images.",
        )
    })?;

    Ok(Json(MediaAlbumDetail { album, media }))
}

// UPDATE ALBUM
pub async fn update_album(
    State(state): State<AppState>,
    Path(album_id): Path<Uuid>,
    Json(payload): Json<UpdateAlbumRequest>,
) -> Result<Json<MediaAlbum>, (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();

    let description = payload
        .description
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    validate_album(&title, &description, payload.display_order)
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, &error))?;

    let album = sqlx::query_as::<_, MediaAlbum>(
        r#"
        UPDATE media_albums
        SET
            title = $2,
            description = $3,
            is_published = $4,
            display_order = $5,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id,
            title,
            description,
            is_published,
            display_order,
            created_at,
            updated_at
        "#,
    )
    .bind(album_id)
    .bind(&title)
    .bind(&description)
    .bind(payload.is_published)
    .bind(payload.display_order)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Album update error: {error}");

        api_error(StatusCode::INTERNAL_SERVER_ERROR, "Unable to update album.")
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Album not found."))?;

    Ok(Json(album))
}

// ADD EXISTING IMAGE TO ALBUM
pub async fn add_media_to_album(
    State(state): State<AppState>,
    Path((album_id, media_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<AdminApiError>)> {
    let album_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM media_albums WHERE id = $1)")
            .bind(album_id)
            .fetch_one(&state.db)
            .await
            .map_err(|error| {
                eprintln!("Album validation error: {error}");

                api_error(StatusCode::INTERNAL_SERVER_ERROR, "Unable to verify album.")
            })?;

    if !album_exists {
        return Err(api_error(StatusCode::NOT_FOUND, "Album not found."));
    }

    let image_exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
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
        eprintln!("Media validation error: {error}");

        api_error(StatusCode::INTERNAL_SERVER_ERROR, "Unable to verify image.")
    })?;

    if !image_exists {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Active image not found in media library.",
        ));
    }

    let result = sqlx::query(
        r#"
        INSERT INTO album_media (
            album_id,
            media_id,
            display_order
        )
        VALUES ($1, $2, 0)
        ON CONFLICT (album_id, media_id) DO NOTHING
        "#,
    )
    .bind(album_id)
    .bind(media_id)
    .execute(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Album media insertion error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to add image to album.",
        )
    })?;

    if result.rows_affected() == 0 {
        return Err(api_error(
            StatusCode::CONFLICT,
            "Image already belongs to this album.",
        ));
    }

    Ok(StatusCode::CREATED)
}

// REMOVE IMAGE FROM ALBUM.
// The original media file is preserved.
pub async fn remove_media_from_album(
    State(state): State<AppState>,
    Path((album_id, media_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<AdminApiError>)> {
    let result = sqlx::query(
        r#"
        DELETE FROM album_media
        WHERE album_id = $1
        AND media_id = $2
        "#,
    )
    .bind(album_id)
    .bind(media_id)
    .execute(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Album media removal error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to remove image from album.",
        )
    })?;

    if result.rows_affected() == 0 {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Image association was not found.",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
