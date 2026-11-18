use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{handlers::admin_handler::AdminApiError, models::news::NewsArticle, state::AppState};

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
        eprintln!("News media validation error: {error}");
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
pub struct CreateNewsRequest {
    pub title: String,
    pub slug: Option<String>,
    pub content: String,
    pub featured_media_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateNewsRequest {
    pub title: String,
    pub slug: Option<String>,
    pub content: String,
    pub featured_media_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateNewsStatusRequest {
    pub status: String,
}

// CREATE ARTICLE
pub async fn create_news(
    State(state): State<AppState>,
    Json(payload): Json<CreateNewsRequest>,
) -> Result<(StatusCode, Json<NewsArticle>), (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();
    let content = payload.content.trim().to_string();

    let slug = clean_optional(payload.slug)
        .map(|value| make_slug(&value))
        .unwrap_or_else(|| make_slug(&title));

    if title.is_empty() || title.chars().count() > 255 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Title is required and must not exceed 255 characters.",
        ));
    }

    if content.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Article content is required.",
        ));
    }

    if slug.is_empty() || slug.chars().count() > 255 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "A valid article slug is required.",
        ));
    }

    validate_media(&state, payload.featured_media_id).await?;

    let article = sqlx::query_as::<_, NewsArticle>(
        r#"
        INSERT INTO news (
            title,
            slug,
            content,
            featured_media_id,
            status,
            published_at
        )
        VALUES ($1, $2, $3, $4, 'draft', NULL)
        RETURNING
            id,
            title,
            slug,
            content,
            featured_media_id,
            status,
            published_at,
            created_at,
            updated_at
        "#,
    )
    .bind(&title)
    .bind(&slug)
    .bind(&content)
    .bind(payload.featured_media_id)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        if error.to_string().contains("news_slug_key") {
            api_error(
                StatusCode::CONFLICT,
                "Another article already uses this slug.",
            )
        } else {
            eprintln!("News creation error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to create news article.",
            )
        }
    })?;

    Ok((StatusCode::CREATED, Json(article)))
}

// ADMIN: LIST ALL ARTICLES
pub async fn list_news(
    State(state): State<AppState>,
) -> Result<Json<Vec<NewsArticle>>, (StatusCode, Json<AdminApiError>)> {
    let articles = sqlx::query_as::<_, NewsArticle>(
        r#"
        SELECT
            id,
            title,
            slug,
            content,
            featured_media_id,
            status,
            published_at,
            created_at,
            updated_at
        FROM news
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("News listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve news articles.",
        )
    })?;

    Ok(Json(articles))
}

// ADMIN: GET ONE ARTICLE
pub async fn get_news(
    State(state): State<AppState>,
    Path(article_id): Path<Uuid>,
) -> Result<Json<NewsArticle>, (StatusCode, Json<AdminApiError>)> {
    let article = sqlx::query_as::<_, NewsArticle>(
        r#"
        SELECT
            id,
            title,
            slug,
            content,
            featured_media_id,
            status,
            published_at,
            created_at,
            updated_at
        FROM news
        WHERE id = $1
        "#,
    )
    .bind(article_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("News retrieval error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve article.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "News article not found."))?;

    Ok(Json(article))
}

// UPDATE ARTICLE
// Editing does not reset published_at.
pub async fn update_news(
    State(state): State<AppState>,
    Path(article_id): Path<Uuid>,
    Json(payload): Json<UpdateNewsRequest>,
) -> Result<Json<NewsArticle>, (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();
    let content = payload.content.trim().to_string();

    let slug = clean_optional(payload.slug)
        .map(|value| make_slug(&value))
        .unwrap_or_else(|| make_slug(&title));

    if title.is_empty() || title.chars().count() > 255 {
        return Err(api_error(StatusCode::BAD_REQUEST, "Invalid article title."));
    }

    if content.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Article content is required.",
        ));
    }

    if slug.is_empty() || slug.chars().count() > 255 {
        return Err(api_error(StatusCode::BAD_REQUEST, "Invalid article slug."));
    }

    validate_media(&state, payload.featured_media_id).await?;

    let article = sqlx::query_as::<_, NewsArticle>(
        r#"
        UPDATE news
        SET
            title = $2,
            slug = $3,
            content = $4,
            featured_media_id = $5,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id,
            title,
            slug,
            content,
            featured_media_id,
            status,
            published_at,
            created_at,
            updated_at
        "#,
    )
    .bind(article_id)
    .bind(&title)
    .bind(&slug)
    .bind(&content)
    .bind(payload.featured_media_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        if error.to_string().contains("news_slug_key") {
            api_error(
                StatusCode::CONFLICT,
                "Another article already uses this slug.",
            )
        } else {
            eprintln!("News update error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to update article.",
            )
        }
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "News article not found."))?;

    Ok(Json(article))
}

// PUBLISH OR RETURN TO DRAFT
pub async fn update_news_status(
    State(state): State<AppState>,
    Path(article_id): Path<Uuid>,
    Json(payload): Json<UpdateNewsStatusRequest>,
) -> Result<Json<NewsArticle>, (StatusCode, Json<AdminApiError>)> {
    let status = payload.status.trim().to_lowercase();

    if status != "draft" && status != "published" {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Status must be either 'draft' or 'published'.",
        ));
    }

    let article = sqlx::query_as::<_, NewsArticle>(
        r#"
        UPDATE news
        SET
            status = $2,
            published_at = CASE
                WHEN $2 = 'published'
                    THEN COALESCE(published_at, NOW())
                ELSE NULL
            END,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id,
            title,
            slug,
            content,
            featured_media_id,
            status,
            published_at,
            created_at,
            updated_at
        "#,
    )
    .bind(article_id)
    .bind(&status)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("News status update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update article status.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "News article not found."))?;

    Ok(Json(article))
}

// DELETE ARTICLE
// The associated media file is not deleted.
pub async fn delete_news(
    State(state): State<AppState>,
    Path(article_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<AdminApiError>)> {
    let result = sqlx::query("DELETE FROM news WHERE id = $1")
        .bind(article_id)
        .execute(&state.db)
        .await
        .map_err(|error| {
            eprintln!("News deletion error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to delete article.",
            )
        })?;

    if result.rows_affected() == 0 {
        return Err(api_error(StatusCode::NOT_FOUND, "News article not found."));
    }

    Ok(StatusCode::NO_CONTENT)
}

// PUBLIC: LIST PUBLISHED ARTICLES
pub async fn list_public_news(
    State(state): State<AppState>,
) -> Result<Json<Vec<NewsArticle>>, (StatusCode, Json<AdminApiError>)> {
    let articles = sqlx::query_as::<_, NewsArticle>(
        r#"
        SELECT
            id,
            title,
            slug,
            content,
            featured_media_id,
            status,
            published_at,
            created_at,
            updated_at
        FROM news
        WHERE status = 'published'
          AND published_at IS NOT NULL
        ORDER BY published_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public news listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve news articles.",
        )
    })?;

    Ok(Json(articles))
}

// PUBLIC: GET ONE PUBLISHED ARTICLE BY SLUG
pub async fn get_public_news_by_slug(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<NewsArticle>, (StatusCode, Json<AdminApiError>)> {
    let article = sqlx::query_as::<_, NewsArticle>(
        r#"
        SELECT
            id,
            title,
            slug,
            content,
            featured_media_id,
            status,
            published_at,
            created_at,
            updated_at
        FROM news
        WHERE slug = $1
          AND status = 'published'
          AND published_at IS NOT NULL
        "#,
    )
    .bind(slug)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public news retrieval error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve news article.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Published article not found."))?;

    Ok(Json(article))
}
