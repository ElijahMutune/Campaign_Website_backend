use axum::{
    extract::{Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};

use uuid::Uuid;

use crate::{
    models::user::AuthenticatedUser, services::auth_service::verify_token, state::AppState,
};

// Extract the bearer token.
fn extract_token(request: &Request) -> Result<String, StatusCode> {
    let header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let (scheme, token) = header.split_once(' ').ok_or(StatusCode::UNAUTHORIZED)?;

    if !scheme.eq_ignore_ascii_case("Bearer") || token.trim().is_empty() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(token.trim().to_string())
}

// Validate JWT and retrieve the active user from PostgreSQL.
async fn load_authenticated_user(
    token: String,
    state: &AppState,
) -> Result<AuthenticatedUser, StatusCode> {
    let claims = verify_token(&token, &state.jwt_secret).map_err(|_| StatusCode::UNAUTHORIZED)?;

    let user_id = Uuid::parse_str(&claims.sub).map_err(|_| StatusCode::UNAUTHORIZED)?;

    let user = sqlx::query_as::<_, AuthenticatedUser>(
        r#"
        SELECT
            id,
            full_name,
            username,
            email,
            role,
            must_change_password
        FROM users
        WHERE id = $1
        AND is_active = TRUE
        "#,
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Authentication database error: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::UNAUTHORIZED)?;

    if user.username != claims.username || user.role != claims.role {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(user)
}

// Any authenticated user.
// This intentionally allows access to /me and /change-password.
pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = extract_token(&request)?;

    let user = load_authenticated_user(token, &state).await?;

    request.extensions_mut().insert(user);

    Ok(next.run(request).await)
}

// Authenticated users who have completed their password change.
pub async fn require_password_changed(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = extract_token(&request)?;

    let user = load_authenticated_user(token, &state).await?;

    if user.must_change_password {
        return Err(StatusCode::FORBIDDEN);
    }

    request.extensions_mut().insert(user);

    Ok(next.run(request).await)
}

// Administrators only, with mandatory password change enforced.
pub async fn require_admin(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = extract_token(&request)?;

    let user = load_authenticated_user(token, &state).await?;

    if user.must_change_password {
        return Err(StatusCode::FORBIDDEN);
    }

    if user.role != "admin" && user.role != "mp" {
        return Err(StatusCode::FORBIDDEN);
    }

    request.extensions_mut().insert(user);

    Ok(next.run(request).await)
}
