use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};

use serde::{Deserialize, Serialize};

use crate::{handlers::admin_handler::AdminApiError, models::media::MediaRecord, state::AppState};

#[derive(Debug, Deserialize)]
pub struct MediaListQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub search: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MediaListResponse {
    pub success: bool,
    pub page: u32,
    pub per_page: u32,
    pub total_items: i64,
    pub total_pages: i64,
    pub items: Vec<MediaRecord>,
}

fn api_error(status: StatusCode, message: &str) -> (StatusCode, Json<AdminApiError>) {
    (
        status,
        Json(AdminApiError {
            success: false,
            message: message.to_string(),
        }),
    )
}

// List uploaded media with pagination and search.
pub async fn list_media(
    State(state): State<AppState>,
    Query(params): Query<MediaListQuery>,
) -> Result<Json<MediaListResponse>, (StatusCode, Json<AdminApiError>)> {
    let page = params.page.unwrap_or(1);
    let per_page = params.per_page.unwrap_or(20);

    if page == 0 || per_page == 0 || per_page > 100 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Page must be at least 1 and per_page must be between 1 and 100.",
        ));
    }

    let search = params
        .search
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if search
        .as_ref()
        .is_some_and(|value| value.chars().count() > 100)
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Search text must not exceed 100 characters.",
        ));
    }

    // Count matching media records.
    let total_items: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM media
        WHERE deleted_at IS NULL
        AND (
            $1::TEXT IS NULL
            OR original_filename ILIKE '%' || $1 || '%'
            OR COALESCE(alt_text, '') ILIKE '%' || $1 || '%'
            OR COALESCE(caption, '') ILIKE '%' || $1 || '%'
        )
        "#,
    )
    .bind(search.as_deref())
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Media count error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve media records.",
        )
    })?;

    let offset = (i64::from(page) - 1) * i64::from(per_page);

    // Retrieve media records for the requested page.
    let items = sqlx::query_as::<_, MediaRecord>(
        r#"
        SELECT
            id,
            original_filename,
            mime_type,
            file_size,
            media_type,
            alt_text,
            caption,
            uploaded_by,
            created_at
        FROM media
        WHERE deleted_at IS NULL
        AND (
            $1::TEXT IS NULL
            OR original_filename ILIKE '%' || $1 || '%'
            OR COALESCE(alt_text, '') ILIKE '%' || $1 || '%'
            OR COALESCE(caption, '') ILIKE '%' || $1 || '%'
        )
        ORDER BY created_at DESC, id DESC
        LIMIT $2
        OFFSET $3
        "#,
    )
    .bind(search.as_deref())
    .bind(i64::from(per_page))
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Media listing error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve media records.",
        )
    })?;

    let total_pages = if total_items == 0 {
        0
    } else {
        1 + (total_items - 1) / i64::from(per_page)
    };

    Ok(Json(MediaListResponse {
        success: true,
        page,
        per_page,
        total_items,
        total_pages,
        items,
    }))
}
