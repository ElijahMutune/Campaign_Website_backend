use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ContactMessage {
    pub id: Uuid,
    pub full_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub subject: Option<String>,
    pub message: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub admin_notes: Option<String>,
    pub updated_at: DateTime<Utc>,
}
