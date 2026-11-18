use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    handlers::admin_handler::AdminApiError, models::manifesto::ManifestoItem, state::AppState,
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

fn make_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for ch in value.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }

    slug.trim_matches('-').to_string()
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
        eprintln!("Manifesto media validation error: {error}");

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
pub struct CreateManifestoRequest {
    pub title: String,
    pub slug: Option<String>,
    pub description: String,
    pub media_id: Option<Uuid>,
    pub display_order: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateManifestoRequest {
    pub title: String,
    pub slug: Option<String>,
    pub description: String,
    pub media_id: Option<Uuid>,
    pub display_order: i32,
}

#[derive(Debug, Deserialize)]
pub struct UpdateManifestoStatusRequest {
    pub status: String,
}

// CREATE
pub async fn create_manifesto_item(
    State(state): State<AppState>,
    Json(payload): Json<CreateManifestoRequest>,
) -> Result<(StatusCode, Json<ManifestoItem>), (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();
    let description = payload.description.trim().to_string();

    let requested_slug = clean_optional(payload.slug);
    let slug = requested_slug
        .map(|value| make_slug(&value))
        .unwrap_or_else(|| make_slug(&title));

    let display_order = payload.display_order.unwrap_or(0);

    if title.is_empty() || title.chars().count() > 255 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Title is required and must not exceed 255 characters.",
        ));
    }

    if slug.is_empty() || slug.chars().count() > 255 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "A valid slug is required.",
        ));
    }

    if description.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Description is required.",
        ));
    }

    if !(0..=1_000_000).contains(&display_order) {
        return Err(api_error(StatusCode::BAD_REQUEST, "Invalid display order."));
    }

    validate_media(&state, payload.media_id).await?;

    let item = sqlx::query_as::<_, ManifestoItem>(
        r#"
        INSERT INTO manifesto_items (
            title,
            slug,
            description,
            media_id,
            display_order,
            status
        )
        VALUES ($1, $2, $3, $4, $5, 'draft')
        RETURNING
            id,
            title,
            slug,
            description,
            media_id,
            display_order,
            status,
            created_at,
            updated_at
        "#,
    )
    .bind(&title)
    .bind(&slug)
    .bind(&description)
    .bind(payload.media_id)
    .bind(display_order)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        if error.to_string().contains("manifesto_items_slug_key") {
            api_error(
                StatusCode::CONFLICT,
                "Another manifesto item already uses this slug.",
            )
        } else {
            eprintln!("Manifesto creation error: {error}");

            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to create manifesto item.",
            )
        }
    })?;

    Ok((StatusCode::CREATED, Json(item)))
}

// LIST FOR ADMIN
pub async fn list_manifesto_items(
    State(state): State<AppState>,
) -> Result<Json<Vec<ManifestoItem>>, (StatusCode, Json<AdminApiError>)> {
    let items = sqlx::query_as::<_, ManifestoItem>(
        r#"
        SELECT
            id,
            title,
            slug,
            description,
            media_id,
            display_order,
            status,
            created_at,
            updated_at
        FROM manifesto_items
        ORDER BY display_order ASC, created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Manifesto listing error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve manifesto items.",
        )
    })?;

    Ok(Json(items))
}

// GET ONE
pub async fn get_manifesto_item(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> Result<Json<ManifestoItem>, (StatusCode, Json<AdminApiError>)> {
    let item = sqlx::query_as::<_, ManifestoItem>(
        r#"
        SELECT
            id,
            title,
            slug,
            description,
            media_id,
            display_order,
            status,
            created_at,
            updated_at
        FROM manifesto_items
        WHERE id = $1
        "#,
    )
    .bind(item_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Manifesto retrieval error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve manifesto item.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Manifesto item not found."))?;

    Ok(Json(item))
}

// UPDATE
pub async fn update_manifesto_item(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Json(payload): Json<UpdateManifestoRequest>,
) -> Result<Json<ManifestoItem>, (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();
    let description = payload.description.trim().to_string();

    let requested_slug = clean_optional(payload.slug);
    let slug = requested_slug
        .map(|value| make_slug(&value))
        .unwrap_or_else(|| make_slug(&title));

    if title.is_empty() || title.chars().count() > 255 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Invalid manifesto title.",
        ));
    }

    if slug.is_empty() || slug.chars().count() > 255 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Invalid manifesto slug.",
        ));
    }

    if description.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Description is required.",
        ));
    }

    if !(0..=1_000_000).contains(&payload.display_order) {
        return Err(api_error(StatusCode::BAD_REQUEST, "Invalid display order."));
    }

    validate_media(&state, payload.media_id).await?;

    let item = sqlx::query_as::<_, ManifestoItem>(
        r#"
        UPDATE manifesto_items
        SET
            title = $2,
            slug = $3,
            description = $4,
            media_id = $5,
            display_order = $6,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id,
            title,
            slug,
            description,
            media_id,
            display_order,
            status,
            created_at,
            updated_at
        "#,
    )
    .bind(item_id)
    .bind(&title)
    .bind(&slug)
    .bind(&description)
    .bind(payload.media_id)
    .bind(payload.display_order)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        if error.to_string().contains("manifesto_items_slug_key") {
            api_error(
                StatusCode::CONFLICT,
                "Another manifesto item already uses this slug.",
            )
        } else {
            eprintln!("Manifesto update error: {error}");

            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to update manifesto item.",
            )
        }
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Manifesto item not found."))?;

    Ok(Json(item))
}

// PUBLISH OR RETURN TO DRAFT
pub async fn update_manifesto_status(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Json(payload): Json<UpdateManifestoStatusRequest>,
) -> Result<Json<ManifestoItem>, (StatusCode, Json<AdminApiError>)> {
    let status = payload.status.trim().to_lowercase();

    if status != "draft" && status != "published" {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Status must be either 'draft' or 'published'.",
        ));
    }

    let item = sqlx::query_as::<_, ManifestoItem>(
        r#"
        UPDATE manifesto_items
        SET
            status = $2,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id,
            title,
            slug,
            description,
            media_id,
            display_order,
            status,
            created_at,
            updated_at
        "#,
    )
    .bind(item_id)
    .bind(&status)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Manifesto status update error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update manifesto status.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Manifesto item not found."))?;

    Ok(Json(item))
}

// DELETE
pub async fn delete_manifesto_item(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<AdminApiError>)> {
    let result = sqlx::query("DELETE FROM manifesto_items WHERE id = $1")
        .bind(item_id)
        .execute(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Manifesto deletion error: {error}");

            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to delete manifesto item.",
            )
        })?;

    if result.rows_affected() == 0 {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Manifesto item not found.",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

// PUBLIC LIST
pub async fn list_public_manifesto_items(
    State(state): State<AppState>,
) -> Result<Json<Vec<ManifestoItem>>, (StatusCode, Json<AdminApiError>)> {
    let items = sqlx::query_as::<_, ManifestoItem>(
        r#"
        SELECT
            id,
            title,
            slug,
            description,
            media_id,
            display_order,
            status,
            created_at,
            updated_at
        FROM manifesto_items
        WHERE status = 'published'
        ORDER BY display_order ASC, created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public manifesto listing error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve manifesto items.",
        )
    })?;

    Ok(Json(items))
}
