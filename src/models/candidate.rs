use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow, Serialize)]
pub struct CandidateProfile {
    pub id: Uuid,
    pub full_name: String,
    pub position: Option<String>,
    pub constituency: Option<String>,
    pub slogan: Option<String>,
    pub biography: Option<String>,
    pub profile_media_id: Option<Uuid>,
    pub is_published: bool,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
