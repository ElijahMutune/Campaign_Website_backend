use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::{handlers::admin_handler::AdminApiError, models::headline::Headline, state::AppState};

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

fn validate_link(value: Option<String>) -> Result<Option<String>, String> {
    let value = clean_optional(value);

    if let Some(ref url) = value {
        let valid = url.len() <= 500
            && (url.starts_with("https://")
                || url.starts_with("http://")
                || (url.starts_with('/') && !url.starts_with("//")));

        if !valid {
            return Err("Link must be an HTTP, HTTPS or local relative URL.".to_string());
        }
    }

    Ok(value)
}

fn validate_schedule(
    starts_at: &Option<DateTime<Utc>>,
    ends_at: &Option<DateTime<Utc>>,
) -> Result<(), String> {
    if let (Some(start), Some(end)) = (starts_at, ends_at) {
        if start >= end {
            return Err("Headline end time must be later than its start time.".to_string());
        }
    }

    Ok(())
}

async fn validate_media(
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
        eprintln!("Headline media validation error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to validate selected image.",
        )
    })?;

    if !exists {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Selected image does not exist in the active media library.",
        ));
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct CreateHeadlineRequest {
    pub title: String,
    pub description: Option<String>,
    pub media_id: Option<Uuid>,
    pub link_url: Option<String>,
    pub display_order: Option<i32>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateHeadlineRequest {
    pub title: String,
    pub description: Option<String>,
    pub media_id: Option<Uuid>,
    pub link_url: Option<String>,
    pub display_order: i32,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateHeadlineStatusRequest {
    pub is_active: bool,
}

// CREATE HEADLINE
pub async fn create_headline(
    State(state): State<AppState>,
    Json(payload): Json<CreateHeadlineRequest>,
) -> Result<(StatusCode, Json<Headline>), (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();
    let description = clean_optional(payload.description);
    let link_url = validate_link(payload.link_url)
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, &error))?;

    let display_order = payload.display_order.unwrap_or(0);

    if title.is_empty() || title.chars().count() > 255 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Title is required and must not exceed 255 characters.",
        ));
    }

    if description
        .as_ref()
        .is_some_and(|v| v.chars().count() > 20000)
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Description is too long.",
        ));
    }

    if !(0..=1_000_000).contains(&display_order) {
        return Err(api_error(StatusCode::BAD_REQUEST, "Invalid display order."));
    }

    validate_schedule(&payload.starts_at, &payload.ends_at)
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, &error))?;

    validate_media(&state, payload.media_id).await?;

    // New headlines remain unpublished until explicitly activated.
    let headline = sqlx::query_as::<_, Headline>(
        r#"
        INSERT INTO headlines (
            title,
            description,
            media_id,
            link_url,
            is_active,
            display_order,
            starts_at,
            ends_at
        )
        VALUES ($1, $2, $3, $4, FALSE, $5, $6, $7)
        RETURNING
            id,
            title,
            description,
            media_id,
            link_url,
            is_active,
            display_order,
            starts_at,
            ends_at,
            created_at,
            updated_at
        "#,
    )
    .bind(&title)
    .bind(&description)
    .bind(payload.media_id)
    .bind(&link_url)
    .bind(display_order)
    .bind(&payload.starts_at)
    .bind(&payload.ends_at)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Headline creation error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to create headline.",
        )
    })?;

    Ok((StatusCode::CREATED, Json(headline)))
}

// LIST ALL HEADLINES FOR ADMIN
pub async fn list_headlines(
    State(state): State<AppState>,
) -> Result<Json<Vec<Headline>>, (StatusCode, Json<AdminApiError>)> {
    let headlines = sqlx::query_as::<_, Headline>(
        r#"
        SELECT
            id,
            title,
            description,
            media_id,
            link_url,
            is_active,
            display_order,
            starts_at,
            ends_at,
            created_at,
            updated_at
        FROM headlines
        ORDER BY display_order ASC, created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Headline listing error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve headlines.",
        )
    })?;

    Ok(Json(headlines))
}

// GET ONE HEADLINE
pub async fn get_headline(
    State(state): State<AppState>,
    Path(headline_id): Path<Uuid>,
) -> Result<Json<Headline>, (StatusCode, Json<AdminApiError>)> {
    let headline = sqlx::query_as::<_, Headline>(
        r#"
        SELECT
            id,
            title,
            description,
            media_id,
            link_url,
            is_active,
            display_order,
            starts_at,
            ends_at,
            created_at,
            updated_at
        FROM headlines
        WHERE id = $1
        "#,
    )
    .bind(headline_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Headline retrieval error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve headline.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Headline not found."))?;

    Ok(Json(headline))
}

// UPDATE HEADLINE CONTENT
pub async fn update_headline(
    State(state): State<AppState>,
    Path(headline_id): Path<Uuid>,
    Json(payload): Json<UpdateHeadlineRequest>,
) -> Result<Json<Headline>, (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();
    let description = clean_optional(payload.description);
    let link_url = validate_link(payload.link_url)
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, &error))?;

    if title.is_empty() || title.chars().count() > 255 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Invalid headline title.",
        ));
    }

    if description
        .as_ref()
        .is_some_and(|v| v.chars().count() > 20000)
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Description is too long.",
        ));
    }

    if !(0..=1_000_000).contains(&payload.display_order) {
        return Err(api_error(StatusCode::BAD_REQUEST, "Invalid display order."));
    }

    validate_schedule(&payload.starts_at, &payload.ends_at)
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, &error))?;

    validate_media(&state, payload.media_id).await?;

    let headline = sqlx::query_as::<_, Headline>(
        r#"
        UPDATE headlines
        SET
            title = $2,
            description = $3,
            media_id = $4,
            link_url = $5,
            display_order = $6,
            starts_at = $7,
            ends_at = $8,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id,
            title,
            description,
            media_id,
            link_url,
            is_active,
            display_order,
            starts_at,
            ends_at,
            created_at,
            updated_at
        "#,
    )
    .bind(headline_id)
    .bind(&title)
    .bind(&description)
    .bind(payload.media_id)
    .bind(&link_url)
    .bind(payload.display_order)
    .bind(&payload.starts_at)
    .bind(&payload.ends_at)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Headline update error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update headline.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Headline not found."))?;

    Ok(Json(headline))
}

// PUBLISH OR UNPUBLISH
pub async fn update_headline_status(
    State(state): State<AppState>,
    Path(headline_id): Path<Uuid>,
    Json(payload): Json<UpdateHeadlineStatusRequest>,
) -> Result<Json<Headline>, (StatusCode, Json<AdminApiError>)> {
    let headline = sqlx::query_as::<_, Headline>(
        r#"
        UPDATE headlines
        SET
            is_active = $2,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id,
            title,
            description,
            media_id,
            link_url,
            is_active,
            display_order,
            starts_at,
            ends_at,
            created_at,
            updated_at
        "#,
    )
    .bind(headline_id)
    .bind(payload.is_active)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Headline publication error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update headline status.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Headline not found."))?;

    Ok(Json(headline))
}

// DELETE HEADLINE RECORD ONLY.
// The associated image in the media library is preserved.
pub async fn delete_headline(
    State(state): State<AppState>,
    Path(headline_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<AdminApiError>)> {
    let result = sqlx::query("DELETE FROM headlines WHERE id = $1")
        .bind(headline_id)
        .execute(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Headline deletion error: {error}");

            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to delete headline.",
            )
        })?;

    if result.rows_affected() == 0 {
        return Err(api_error(StatusCode::NOT_FOUND, "Headline not found."));
    }

    Ok(StatusCode::NO_CONTENT)
}

// PUBLIC HEADLINES.
// Only active headlines within their scheduled dates are returned.
pub async fn list_public_headlines(
    State(state): State<AppState>,
) -> Result<Json<Vec<Headline>>, (StatusCode, Json<AdminApiError>)> {
    let headlines = sqlx::query_as::<_, Headline>(
        r#"
        SELECT
            h.id,
            h.title,
            h.description,
            h.media_id,
            h.link_url,
            h.is_active,
            h.display_order,
            h.starts_at,
            h.ends_at,
            h.created_at,
            h.updated_at
        FROM headlines h
        WHERE h.is_active = TRUE
        AND (h.starts_at IS NULL OR h.starts_at <= NOW())
        AND (h.ends_at IS NULL OR h.ends_at > NOW())
        AND (
            h.media_id IS NULL
            OR EXISTS (
                SELECT 1
                FROM media m
                WHERE m.id = h.media_id
                AND m.media_type = 'image'
                AND m.deleted_at IS NULL
            )
        )
        ORDER BY h.display_order ASC, h.created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public headline retrieval error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve public headlines.",
        )
    })?;

    Ok(Json(headlines))
}
