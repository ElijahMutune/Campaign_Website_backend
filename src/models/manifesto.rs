use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ManifestoItem {
    pub id: Uuid,
    pub title: String,
    pub slug: String,
    pub description: String,
    pub media_id: Option<Uuid>,
    pub display_order: i32,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
