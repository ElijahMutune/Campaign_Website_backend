use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    handlers::admin_handler::AdminApiError, models::poster_category::PosterCategory,
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

fn validate_name(name: &str) -> Result<(), (StatusCode, Json<AdminApiError>)> {
    if name.is_empty() || name.chars().count() > 150 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Category name is required and must not exceed 150 characters.",
        ));
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct CreatePosterCategoryRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePosterCategoryRequest {
    pub name: String,
    pub description: Option<String>,
}

// ADMIN: CREATE CATEGORY
pub async fn create_poster_category(
    State(state): State<AppState>,
    Json(payload): Json<CreatePosterCategoryRequest>,
) -> Result<(StatusCode, Json<PosterCategory>), (StatusCode, Json<AdminApiError>)> {
    let name = payload.name.trim().to_string();
    let description = clean_optional(payload.description);

    validate_name(&name)?;

    let category = sqlx::query_as::<_, PosterCategory>(
        r#"
        INSERT INTO poster_categories (
            name,
            description
        )
        VALUES ($1, $2)
        RETURNING id, name, description, created_at
        "#,
    )
    .bind(&name)
    .bind(&description)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        if error.to_string().contains("poster_categories_name_key") {
            api_error(
                StatusCode::CONFLICT,
                "A poster category with this name already exists.",
            )
        } else {
            eprintln!("Poster category creation error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to create poster category.",
            )
        }
    })?;

    Ok((StatusCode::CREATED, Json(category)))
}

// ADMIN: LIST CATEGORIES
pub async fn list_poster_categories(
    State(state): State<AppState>,
) -> Result<Json<Vec<PosterCategory>>, (StatusCode, Json<AdminApiError>)> {
    let categories = sqlx::query_as::<_, PosterCategory>(
        r#"
        SELECT id, name, description, created_at
        FROM poster_categories
        ORDER BY name ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Poster category listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve poster categories.",
        )
    })?;

    Ok(Json(categories))
}

// ADMIN: GET ONE CATEGORY
pub async fn get_poster_category(
    State(state): State<AppState>,
    Path(category_id): Path<Uuid>,
) -> Result<Json<PosterCategory>, (StatusCode, Json<AdminApiError>)> {
    let category = sqlx::query_as::<_, PosterCategory>(
        r#"
        SELECT id, name, description, created_at
        FROM poster_categories
        WHERE id = $1
        "#,
    )
    .bind(category_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Poster category retrieval error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve poster category.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Poster category not found."))?;

    Ok(Json(category))
}

// ADMIN: UPDATE CATEGORY
pub async fn update_poster_category(
    State(state): State<AppState>,
    Path(category_id): Path<Uuid>,
    Json(payload): Json<UpdatePosterCategoryRequest>,
) -> Result<Json<PosterCategory>, (StatusCode, Json<AdminApiError>)> {
    let name = payload.name.trim().to_string();
    let description = clean_optional(payload.description);

    validate_name(&name)?;

    let category = sqlx::query_as::<_, PosterCategory>(
        r#"
        UPDATE poster_categories
        SET
            name = $2,
            description = $3
        WHERE id = $1
        RETURNING id, name, description, created_at
        "#,
    )
    .bind(category_id)
    .bind(&name)
    .bind(&description)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        if error.to_string().contains("poster_categories_name_key") {
            api_error(
                StatusCode::CONFLICT,
                "Another category already uses this name.",
            )
        } else {
            eprintln!("Poster category update error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to update poster category.",
            )
        }
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Poster category not found."))?;

    Ok(Json(category))
}

// ADMIN: DELETE CATEGORY
// Existing posters are retained with category_id set to NULL.
pub async fn delete_poster_category(
    State(state): State<AppState>,
    Path(category_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<AdminApiError>)> {
    let result = sqlx::query("DELETE FROM poster_categories WHERE id = $1")
        .bind(category_id)
        .execute(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Poster category deletion error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to delete poster category.",
            )
        })?;

    if result.rows_affected() == 0 {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Poster category not found.",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
