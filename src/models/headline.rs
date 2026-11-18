use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Headline {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub media_id: Option<Uuid>,
    pub link_url: Option<String>,
    pub is_active: bool,
    pub display_order: i32,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
