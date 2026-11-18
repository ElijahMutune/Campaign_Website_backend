use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CampaignDocument {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub media_id: Option<Uuid>,
    pub is_published: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub category: Option<String>,
    pub display_order: i32,
    pub published_at: Option<DateTime<Utc>>,
    pub download_count: i64,
}
