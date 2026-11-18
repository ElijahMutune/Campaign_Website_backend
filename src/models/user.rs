use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct User {
    pub id: Uuid,
    pub full_name: String,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub role: String,
    pub is_active: bool,
    pub must_change_password: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Safe user response. Never expose password_hash.
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub full_name: String,
    pub username: String,
    pub email: Option<String>,
    pub role: String,
    pub is_active: bool,
    pub must_change_password: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            full_name: user.full_name,
            username: user.username,
            email: user.email,
            role: user.role,
            is_active: user.is_active,
            must_change_password: user.must_change_password,
            created_at: user.created_at,
            updated_at: user.updated_at,
        }
    }
}

// Information available to authenticated handlers.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct AuthenticatedUser {
    pub id: Uuid,
    pub full_name: String,
    pub username: String,
    pub email: Option<String>,
    pub role: String,
    pub must_change_password: bool,
}
