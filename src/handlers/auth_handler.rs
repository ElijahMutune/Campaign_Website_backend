use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
};

use serde::{Deserialize, Serialize};

use crate::{
    models::user::{AuthenticatedUser, User, UserResponse},
    services::auth_service::{create_token, hash_password, verify_password},
    state::AppState,
};

const TOKEN_EXPIRATION: u64 = 900;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub success: bool,
    pub message: String,
    pub token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user: UserResponse,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct ChangePasswordResponse {
    pub success: bool,
    pub message: String,
    pub user: AuthenticatedUser,
}

#[derive(Debug, Serialize)]
pub(crate) struct ApiError {
    success: bool,
    message: String,
}

pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ApiError>)> {
    let username = payload.username.trim().to_string();

    if username.is_empty() || payload.password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                success: false,
                message: "Username and password are required".to_string(),
            }),
        ));
    }

    let user = sqlx::query_as::<_, User>(
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
        WHERE username = $1
        "#,
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Login database error: {error}");

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                success: false,
                message: "Unable to process login".to_string(),
            }),
        )
    })?;

    let user = match user {
        Some(user) => user,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ApiError {
                    success: false,
                    message: "Invalid username or password".to_string(),
                }),
            ));
        }
    };

    let password = payload.password;
    let password_hash = user.password_hash.clone();

    let verification =
        tokio::task::spawn_blocking(move || verify_password(&password, &password_hash))
            .await
            .map_err(|error| {
                eprintln!("Password verification task failed: {error}");

                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        success: false,
                        message: "Unable to process login".to_string(),
                    }),
                )
            })?;

    let password_valid = verification.map_err(|error| {
        eprintln!("Password hash verification error: {error}");

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                success: false,
                message: "Unable to process login".to_string(),
            }),
        )
    })?;

    if !password_valid || !user.is_active {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiError {
                success: false,
                message: "Invalid username or password".to_string(),
            }),
        ));
    }

    let token = create_token(
        user.id,
        &user.username,
        &user.role,
        &state.jwt_secret,
        TOKEN_EXPIRATION,
    )
    .map_err(|error| {
        eprintln!("JWT generation error: {error}");

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                success: false,
                message: "Unable to create authentication token".to_string(),
            }),
        )
    })?;

    let user_response = UserResponse::from(user);

    Ok(Json(LoginResponse {
        success: true,
        message: "Login successful".to_string(),
        token,
        token_type: "Bearer".to_string(),
        expires_in: TOKEN_EXPIRATION,
        user: user_response,
    }))
}

// Return the currently authenticated user.
pub async fn me(Extension(user): Extension<AuthenticatedUser>) -> Json<AuthenticatedUser> {
    Json(user)
}

// Change the authenticated user's password.
pub async fn change_password(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Json<ChangePasswordResponse>, (StatusCode, Json<ApiError>)> {
    if payload.current_password.is_empty()
        || !(12..=128).contains(&payload.new_password.chars().count())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                success: false,
                message: "The current password is required and the new password must contain 12 to 128 characters.".to_string(),
            }),
        ));
    }

    if payload.current_password == payload.new_password {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                success: false,
                message: "The new password must be different from the current password."
                    .to_string(),
            }),
        ));
    }

    let current_hash = sqlx::query_scalar::<_, String>(
        r#"
        SELECT password_hash
        FROM users
        WHERE id = $1
        AND is_active = TRUE
        "#,
    )
    .bind(user.id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Password lookup error: {error}");

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                success: false,
                message: "Unable to change password.".to_string(),
            }),
        )
    })?
    .ok_or((
        StatusCode::UNAUTHORIZED,
        Json(ApiError {
            success: false,
            message: "Authentication required.".to_string(),
        }),
    ))?;

    let current_password = payload.current_password;

    let password_valid =
        tokio::task::spawn_blocking(move || verify_password(&current_password, &current_hash))
            .await
            .map_err(|error| {
                eprintln!("Password verification task failed: {error}");

                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        success: false,
                        message: "Unable to change password.".to_string(),
                    }),
                )
            })?
            .map_err(|error| {
                eprintln!("Password verification error: {error}");

                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        success: false,
                        message: "Unable to change password.".to_string(),
                    }),
                )
            })?;

    if !password_valid {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiError {
                success: false,
                message: "Current password is incorrect.".to_string(),
            }),
        ));
    }

    let new_password = payload.new_password;

    let new_hash = tokio::task::spawn_blocking(move || hash_password(&new_password))
        .await
        .map_err(|error| {
            eprintln!("Password hashing task failed: {error}");

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    success: false,
                    message: "Unable to change password.".to_string(),
                }),
            )
        })?
        .map_err(|error| {
            eprintln!("Password hashing error: {error}");

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    success: false,
                    message: "Unable to change password.".to_string(),
                }),
            )
        })?;

    let updated_user = sqlx::query_as::<_, AuthenticatedUser>(
        r#"
        UPDATE users
        SET
            password_hash = $2,
            must_change_password = FALSE,
            updated_at = NOW()
        WHERE id = $1
        AND is_active = TRUE
        RETURNING
            id,
            full_name,
            username,
            email,
            role,
            must_change_password
        "#,
    )
    .bind(user.id)
    .bind(new_hash)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Password update error: {error}");

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                success: false,
                message: "Unable to save the new password.".to_string(),
            }),
        )
    })?
    .ok_or((
        StatusCode::UNAUTHORIZED,
        Json(ApiError {
            success: false,
            message: "Authentication required.".to_string(),
        }),
    ))?;

    Ok(Json(ChangePasswordResponse {
        success: true,
        message: "Password changed successfully.".to_string(),
        user: updated_user,
    }))
}
