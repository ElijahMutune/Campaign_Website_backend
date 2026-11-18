use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    handlers::admin_handler::AdminApiError, models::navigation_item::NavigationItem,
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

fn validate_fields(
    title: &str,
    url: &str,
    display_order: i32,
) -> Result<(), (StatusCode, Json<AdminApiError>)> {
    if title.is_empty() || title.chars().count() > 150 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Navigation title is required and must not exceed 150 characters.",
        ));
    }

    if url.is_empty() || url.chars().count() > 500 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Navigation URL is required and must not exceed 500 characters.",
        ));
    }

    let safe_url = url.starts_with("https://")
        || url.starts_with("http://")
        || (url.starts_with('/') && !url.starts_with("//"))
        || (url.starts_with('#') && url.len() > 1);

    if !safe_url || url.chars().any(char::is_control) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "URL must be HTTP(S), a local path beginning with one slash, or a valid page anchor.",
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

async fn validate_parent(
    state: &AppState,
    parent_id: Option<Uuid>,
    current_id: Option<Uuid>,
) -> Result<(), (StatusCode, Json<AdminApiError>)> {
    let Some(parent_id) = parent_id else {
        return Ok(());
    };

    if current_id == Some(parent_id) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "A navigation item cannot be its own parent.",
        ));
    }

    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM navigation_items WHERE id = $1)")
            .bind(parent_id)
            .fetch_one(&state.db)
            .await
            .map_err(|error| {
                eprintln!("Navigation parent validation error: {error}");
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Unable to validate navigation parent.",
                )
            })?;

    if !exists {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Selected parent navigation item does not exist.",
        ));
    }

    if let Some(current_id) = current_id {
        let would_create_cycle: bool = sqlx::query_scalar(
            r#"
            WITH RECURSIVE descendants(id) AS (
                SELECT id
                FROM navigation_items
                WHERE parent_id = $1

                UNION

                SELECT item.id
                FROM navigation_items item
                INNER JOIN descendants child
                    ON item.parent_id = child.id
            )
            SELECT EXISTS (
                SELECT 1
                FROM descendants
                WHERE id = $2
            )
            "#,
        )
        .bind(current_id)
        .bind(parent_id)
        .fetch_one(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Navigation hierarchy validation error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to validate navigation hierarchy.",
            )
        })?;

        if would_create_cycle {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "This parent assignment would create a navigation cycle.",
            ));
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct CreateNavigationRequest {
    pub title: String,
    pub url: String,
    pub parent_id: Option<Uuid>,
    #[serde(default)]
    pub display_order: i32,
}

#[derive(Debug, Deserialize)]
pub struct UpdateNavigationRequest {
    pub title: String,
    pub url: String,
    pub parent_id: Option<Uuid>,
    pub display_order: i32,
    pub is_visible: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateNavigationVisibilityRequest {
    pub is_visible: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateNavigationOrderRequest {
    pub display_order: i32,
}

// ADMIN: CREATE NAVIGATION ITEM
pub async fn create_navigation_item(
    State(state): State<AppState>,
    Json(payload): Json<CreateNavigationRequest>,
) -> Result<(StatusCode, Json<NavigationItem>), (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();
    let url = payload.url.trim().to_string();

    validate_fields(&title, &url, payload.display_order)?;
    validate_parent(&state, payload.parent_id, None).await?;

    let item = sqlx::query_as::<_, NavigationItem>(
        r#"
        INSERT INTO navigation_items (
            title,
            url,
            parent_id,
            display_order,
            is_visible
        )
        VALUES ($1, $2, $3, $4, TRUE)
        RETURNING *
        "#,
    )
    .bind(&title)
    .bind(&url)
    .bind(payload.parent_id)
    .bind(payload.display_order)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Navigation creation error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to create navigation item.",
        )
    })?;

    Ok((StatusCode::CREATED, Json(item)))
}

// ADMIN: LIST ALL NAVIGATION ITEMS
pub async fn list_navigation_items(
    State(state): State<AppState>,
) -> Result<Json<Vec<NavigationItem>>, (StatusCode, Json<AdminApiError>)> {
    let items = sqlx::query_as::<_, NavigationItem>(
        r#"
        SELECT *
        FROM navigation_items
        ORDER BY
            parent_id NULLS FIRST,
            display_order ASC,
            title ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Navigation listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve navigation items.",
        )
    })?;

    Ok(Json(items))
}

// ADMIN: GET ONE NAVIGATION ITEM
pub async fn get_navigation_item(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> Result<Json<NavigationItem>, (StatusCode, Json<AdminApiError>)> {
    let item = sqlx::query_as::<_, NavigationItem>("SELECT * FROM navigation_items WHERE id = $1")
        .bind(item_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Navigation retrieval error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to retrieve navigation item.",
            )
        })?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Navigation item not found."))?;

    Ok(Json(item))
}

// ADMIN: UPDATE NAVIGATION ITEM
pub async fn update_navigation_item(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Json(payload): Json<UpdateNavigationRequest>,
) -> Result<Json<NavigationItem>, (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();
    let url = payload.url.trim().to_string();

    validate_fields(&title, &url, payload.display_order)?;
    validate_parent(&state, payload.parent_id, Some(item_id)).await?;

    let item = sqlx::query_as::<_, NavigationItem>(
        r#"
        UPDATE navigation_items
        SET
            title = $2,
            url = $3,
            parent_id = $4,
            display_order = $5,
            is_visible = $6,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(item_id)
    .bind(&title)
    .bind(&url)
    .bind(payload.parent_id)
    .bind(payload.display_order)
    .bind(payload.is_visible)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Navigation update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update navigation item.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Navigation item not found."))?;

    Ok(Json(item))
}

// ADMIN: CHANGE VISIBILITY
pub async fn update_navigation_visibility(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Json(payload): Json<UpdateNavigationVisibilityRequest>,
) -> Result<Json<NavigationItem>, (StatusCode, Json<AdminApiError>)> {
    let item = sqlx::query_as::<_, NavigationItem>(
        r#"
        UPDATE navigation_items
        SET
            is_visible = $2,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(item_id)
    .bind(payload.is_visible)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Navigation visibility update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update navigation visibility.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Navigation item not found."))?;

    Ok(Json(item))
}

// ADMIN: CHANGE DISPLAY ORDER
pub async fn update_navigation_order(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Json(payload): Json<UpdateNavigationOrderRequest>,
) -> Result<Json<NavigationItem>, (StatusCode, Json<AdminApiError>)> {
    if !(0..=1_000_000).contains(&payload.display_order) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Display order must be between 0 and 1000000.",
        ));
    }

    let item = sqlx::query_as::<_, NavigationItem>(
        r#"
        UPDATE navigation_items
        SET
            display_order = $2,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(item_id)
    .bind(payload.display_order)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Navigation order update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update navigation order.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Navigation item not found."))?;

    Ok(Json(item))
}

// ADMIN: DELETE NAVIGATION ITEM
// The database sets children parent_id to NULL.
pub async fn delete_navigation_item(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<AdminApiError>)> {
    let result = sqlx::query("DELETE FROM navigation_items WHERE id = $1")
        .bind(item_id)
        .execute(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Navigation deletion error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to delete navigation item.",
            )
        })?;

    if result.rows_affected() == 0 {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Navigation item not found.",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

// PUBLIC: LIST VISIBLE NAVIGATION ITEMS
// Results are flat; parent_id lets the frontend build the menu hierarchy.
pub async fn list_public_navigation(
    State(state): State<AppState>,
) -> Result<Json<Vec<NavigationItem>>, (StatusCode, Json<AdminApiError>)> {
    let items = sqlx::query_as::<_, NavigationItem>(
        r#"
        SELECT *
        FROM navigation_items
        WHERE is_visible = TRUE
        ORDER BY
            parent_id NULLS FIRST,
            display_order ASC,
            title ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public navigation listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve navigation.",
        )
    })?;

    Ok(Json(items))
}
