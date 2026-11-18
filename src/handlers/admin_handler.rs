use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    models::user::{AuthenticatedUser, User, UserResponse},
    services::auth_service::hash_password,
    state::AppState,
};

// Standard admin API error response.
#[derive(Debug, Serialize)]
pub(crate) struct AdminApiError {
    pub success: bool,
    pub message: String,
}

// Authenticated administrator information.
pub async fn admin_status(
    Extension(user): Extension<AuthenticatedUser>,
) -> Json<AuthenticatedUser> {
    Json(user)
}

// Request for creating an editor account.
#[derive(Debug, Deserialize)]
pub struct CreateEditorRequest {
    pub full_name: String,
    pub username: String,
    pub email: Option<String>,
    pub password: String,
}

// Create an editor account.
pub async fn create_editor(
    State(state): State<AppState>,
    Json(payload): Json<CreateEditorRequest>,
) -> Result<(StatusCode, Json<UserResponse>), (StatusCode, Json<AdminApiError>)> {
    let full_name = payload.full_name.trim().to_string();
    let username = payload.username.trim().to_ascii_lowercase();

    let email = payload
        .email
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if full_name.is_empty() || full_name.chars().count() > 150 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(AdminApiError {
                success: false,
                message: "Invalid full name.".to_string(),
            }),
        ));
    }

    let username_valid = (3..=50).contains(&username.chars().count())
        && username.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-' || b == b'.'
        });

    if !username_valid {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(AdminApiError {
                success: false,
                message: "Invalid username.".to_string(),
            }),
        ));
    }

    if email.as_ref().is_some_and(|value| value.len() > 255) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(AdminApiError {
                success: false,
                message: "Email address is too long.".to_string(),
            }),
        ));
    }

    if !(12..=128).contains(&payload.password.chars().count()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(AdminApiError {
                success: false,
                message: "Password must contain 12 to 128 characters.".to_string(),
            }),
        ));
    }

    // Hash the password outside the async runtime.
    let password = payload.password;

    let password_hash = tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|error| {
            eprintln!("Password hashing task failed: {error}");

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AdminApiError {
                    success: false,
                    message: "Unable to create editor account.".to_string(),
                }),
            )
        })?
        .map_err(|error| {
            eprintln!("Password hashing failed: {error}");

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AdminApiError {
                    success: false,
                    message: "Unable to create editor account.".to_string(),
                }),
            )
        })?;

    // The role is always editor.
    let result = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (
    full_name,
    username,
    email,
    password_hash,
    role,
    is_active,
    must_change_password
)
VALUES ($1, $2, $3, $4, 'editor', TRUE, FALSE)
        RETURNING
            id,
            full_name,
            username,
            email,
            password_hash,
            role,
            is_active,
             must_change_password,
            created_at,
            updated_at
        "#,
    )
    .bind(&full_name)
    .bind(&username)
    .bind(&email)
    .bind(&password_hash)
    .fetch_one(&state.db)
    .await;

    let user = match result {
        Ok(user) => user,

        Err(error) => {
            let duplicate = error
                .as_database_error()
                .is_some_and(|db| db.code().as_deref() == Some("23505"));

            if duplicate {
                return Err((
                    StatusCode::CONFLICT,
                    Json(AdminApiError {
                        success: false,
                        message: "Username or email already exists.".to_string(),
                    }),
                ));
            }

            eprintln!("Editor creation database error: {error}");

            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AdminApiError {
                    success: false,
                    message: "Unable to create editor account.".to_string(),
                }),
            ));
        }
    };

    Ok((StatusCode::CREATED, Json(UserResponse::from(user))))
}

// List all editor accounts.
pub async fn list_editors(
    State(state): State<AppState>,
) -> Result<Json<Vec<UserResponse>>, (StatusCode, Json<AdminApiError>)> {
    let users = sqlx::query_as::<_, User>(
        r#"
        SELECT
            id,
            full_name,
            username,
            email,
            password_hash,
            role,
            is_active,
             must_change_password,
            created_at,
            updated_at
        FROM users
        WHERE role = 'editor'
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Editor listing error: {error}");

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AdminApiError {
                success: false,
                message: "Unable to retrieve editor accounts.".to_string(),
            }),
        )
    })?;

    let response = users.into_iter().map(UserResponse::from).collect();

    Ok(Json(response))
}

// Request for activating or deactivating an editor.
#[derive(Debug, Deserialize)]
pub struct UpdateEditorStatusRequest {
    pub is_active: bool,
}

// Update editor account status.
pub async fn update_editor_status(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<UpdateEditorStatusRequest>,
) -> Result<Json<UserResponse>, (StatusCode, Json<AdminApiError>)> {
    let user = sqlx::query_as::<_, User>(
        r#"
        UPDATE users
        SET
            is_active = $2,
            updated_at = NOW()
        WHERE id = $1
        AND role = 'editor'
        RETURNING
            id,
            full_name,
            username,
            email,
            password_hash,
            role,
            is_active,
            must_change_password,
            created_at,
            updated_at
        "#,
    )
    .bind(user_id)
    .bind(payload.is_active)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Editor status update error: {error}");

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AdminApiError {
                success: false,
                message: "Unable to update editor account.".to_string(),
            }),
        )
    })?;

    let user = match user {
        Some(user) => user,

        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(AdminApiError {
                    success: false,
                    message: "Editor account not found.".to_string(),
                }),
            ));
        }
    };

    Ok(Json(UserResponse::from(user)))
}
