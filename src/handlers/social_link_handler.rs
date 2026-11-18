use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    handlers::admin_handler::AdminApiError, models::social_link::SocialLink, state::AppState,
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

fn validate_social_link(
    platform: &str,
    url: &str,
    display_order: i32,
) -> Result<(), (StatusCode, Json<AdminApiError>)> {
    if platform.is_empty() || platform.chars().count() > 100 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Platform is required and must not exceed 100 characters.",
        ));
    }

    if url.is_empty() || url.chars().count() > 500 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Social URL is required and must not exceed 500 characters.",
        ));
    }

    let valid_url = url.split_once("://").is_some_and(|(scheme, remainder)| {
        matches!(scheme, "https" | "http")
            && !remainder.is_empty()
            && !remainder.starts_with('/')
            && !remainder.starts_with('.')
            && !remainder.chars().any(char::is_whitespace)
            && !remainder.chars().any(char::is_control)
    });

    if !valid_url {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Social URL must be a valid HTTP or HTTPS address.",
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

#[derive(Debug, Deserialize)]
pub struct CreateSocialLinkRequest {
    pub platform: String,
    pub url: String,
    #[serde(default)]
    pub display_order: i32,
    #[serde(default = "default_visible")]
    pub is_visible: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSocialLinkRequest {
    pub platform: String,
    pub url: String,
    pub display_order: i32,
    pub is_visible: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSocialVisibilityRequest {
    pub is_visible: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSocialOrderRequest {
    pub display_order: i32,
}

fn default_visible() -> bool {
    true
}

// ADMIN: CREATE SOCIAL LINK
pub async fn create_social_link(
    State(state): State<AppState>,
    Json(payload): Json<CreateSocialLinkRequest>,
) -> Result<(StatusCode, Json<SocialLink>), (StatusCode, Json<AdminApiError>)> {
    let platform = payload.platform.trim().to_string();
    let url = payload.url.trim().to_string();

    validate_social_link(&platform, &url, payload.display_order)?;

    let link = sqlx::query_as::<_, SocialLink>(
        r#"
        INSERT INTO social_links (
            platform,
            url,
            display_order,
            is_visible
        )
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(&platform)
    .bind(&url)
    .bind(payload.display_order)
    .bind(payload.is_visible)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Social link creation error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to create social link.",
        )
    })?;

    Ok((StatusCode::CREATED, Json(link)))
}

// ADMIN: LIST ALL SOCIAL LINKS
pub async fn list_social_links(
    State(state): State<AppState>,
) -> Result<Json<Vec<SocialLink>>, (StatusCode, Json<AdminApiError>)> {
    let links = sqlx::query_as::<_, SocialLink>(
        r#"
        SELECT *
        FROM social_links
        ORDER BY display_order ASC, created_at ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Social link listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve social links.",
        )
    })?;

    Ok(Json(links))
}

// ADMIN: GET ONE SOCIAL LINK
pub async fn get_social_link(
    State(state): State<AppState>,
    Path(link_id): Path<Uuid>,
) -> Result<Json<SocialLink>, (StatusCode, Json<AdminApiError>)> {
    let link = sqlx::query_as::<_, SocialLink>("SELECT * FROM social_links WHERE id = $1")
        .bind(link_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Social link retrieval error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to retrieve social link.",
            )
        })?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Social link not found."))?;

    Ok(Json(link))
}

// ADMIN: UPDATE SOCIAL LINK
pub async fn update_social_link(
    State(state): State<AppState>,
    Path(link_id): Path<Uuid>,
    Json(payload): Json<UpdateSocialLinkRequest>,
) -> Result<Json<SocialLink>, (StatusCode, Json<AdminApiError>)> {
    let platform = payload.platform.trim().to_string();
    let url = payload.url.trim().to_string();

    validate_social_link(&platform, &url, payload.display_order)?;

    let link = sqlx::query_as::<_, SocialLink>(
        r#"
        UPDATE social_links
        SET
            platform = $2,
            url = $3,
            display_order = $4,
            is_visible = $5,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(link_id)
    .bind(&platform)
    .bind(&url)
    .bind(payload.display_order)
    .bind(payload.is_visible)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Social link update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update social link.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Social link not found."))?;

    Ok(Json(link))
}

// ADMIN: UPDATE VISIBILITY
pub async fn update_social_visibility(
    State(state): State<AppState>,
    Path(link_id): Path<Uuid>,
    Json(payload): Json<UpdateSocialVisibilityRequest>,
) -> Result<Json<SocialLink>, (StatusCode, Json<AdminApiError>)> {
    let link = sqlx::query_as::<_, SocialLink>(
        r#"
        UPDATE social_links
        SET
            is_visible = $2,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(link_id)
    .bind(payload.is_visible)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Social visibility update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update social link visibility.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Social link not found."))?;

    Ok(Json(link))
}

// ADMIN: UPDATE DISPLAY ORDER
pub async fn update_social_order(
    State(state): State<AppState>,
    Path(link_id): Path<Uuid>,
    Json(payload): Json<UpdateSocialOrderRequest>,
) -> Result<Json<SocialLink>, (StatusCode, Json<AdminApiError>)> {
    if !(0..=1_000_000).contains(&payload.display_order) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Display order must be between 0 and 1000000.",
        ));
    }

    let link = sqlx::query_as::<_, SocialLink>(
        r#"
        UPDATE social_links
        SET
            display_order = $2,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(link_id)
    .bind(payload.display_order)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Social order update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update social link order.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Social link not found."))?;

    Ok(Json(link))
}

// ADMIN: DELETE SOCIAL LINK
pub async fn delete_social_link(
    State(state): State<AppState>,
    Path(link_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<AdminApiError>)> {
    let result = sqlx::query("DELETE FROM social_links WHERE id = $1")
        .bind(link_id)
        .execute(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Social link deletion error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to delete social link.",
            )
        })?;

    if result.rows_affected() == 0 {
        return Err(api_error(StatusCode::NOT_FOUND, "Social link not found."));
    }

    Ok(StatusCode::NO_CONTENT)
}

// PUBLIC: LIST VISIBLE SOCIAL LINKS
pub async fn list_public_social_links(
    State(state): State<AppState>,
) -> Result<Json<Vec<SocialLink>>, (StatusCode, Json<AdminApiError>)> {
    let links = sqlx::query_as::<_, SocialLink>(
        r#"
        SELECT *
        FROM social_links
        WHERE is_visible = TRUE
        ORDER BY display_order ASC, created_at ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public social link listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve social links.",
        )
    })?;

    Ok(Json(links))
}
