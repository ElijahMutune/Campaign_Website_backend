use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct TalentApplication {
    pub id: Uuid,
    pub full_name: String,
    pub age: Option<i32>,
    pub phone: String,
    pub email: Option<String>,
    pub location: Option<String>,
    pub category_id: Option<Uuid>,
    pub talent_description: String,
    pub achievements: Option<String>,
    pub support_needed: Option<String>,
    pub status: String,
    pub admin_notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_public: bool,
    pub public_display_name: Option<String>,
    pub public_summary: Option<String>,
    pub public_media_id: Option<Uuid>,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct TalentApplicationAttachment {
    pub application_id: Uuid,
    pub media_id: Uuid,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct PublicTalentShowcase {
    pub id: Uuid,
    pub public_display_name: String,
    pub public_summary: String,
    pub public_media_id: Option<Uuid>,
    pub published_at: DateTime<Utc>,
}
