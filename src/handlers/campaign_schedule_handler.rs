use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{NaiveDate, NaiveTime};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    handlers::admin_handler::AdminApiError, models::campaign_schedule::CampaignScheduleEntry,
    state::AppState,
};

fn api_error(status: StatusCode, message: &str) -> (StatusCode, Json<AdminApiError>) {
    (
        status,
        Json(AdminApiError {
            success: false,
            message: message.to_string(),
        }),
    )
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn validate_fields(
    title: &str,
    location: &Option<String>,
    display_order: i32,
) -> Result<(), (StatusCode, Json<AdminApiError>)> {
    if title.is_empty() || title.chars().count() > 255 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Title is required and must not exceed 255 characters.",
        ));
    }

    if location
        .as_ref()
        .is_some_and(|value| value.chars().count() > 255)
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Location must not exceed 255 characters.",
        ));
    }

    if !(0..=1_000_000).contains(&display_order) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Display order must be between 0 and 1000000.",
        ));
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct CreateScheduleRequest {
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub scheduled_date: NaiveDate,
    pub scheduled_time: Option<NaiveTime>,
    #[serde(default)]
    pub display_order: i32,
}

#[derive(Debug, Deserialize)]
pub struct UpdateScheduleRequest {
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub scheduled_date: NaiveDate,
    pub scheduled_time: Option<NaiveTime>,
    pub display_order: i32,
}

#[derive(Debug, Deserialize)]
pub struct UpdateScheduleStatusRequest {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSchedulePublicationRequest {
    pub is_published: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateScheduleOrderRequest {
    pub display_order: i32,
}

#[derive(Debug, Deserialize)]
pub struct PublicScheduleQuery {
    pub period: Option<String>,
}

// ADMIN: CREATE PROGRAMME ENTRY
pub async fn create_schedule_entry(
    State(state): State<AppState>,
    Json(payload): Json<CreateScheduleRequest>,
) -> Result<(StatusCode, Json<CampaignScheduleEntry>), (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();
    let description = clean_optional(payload.description);
    let location = clean_optional(payload.location);

    validate_fields(&title, &location, payload.display_order)?;

    let entry = sqlx::query_as::<_, CampaignScheduleEntry>(
        r#"
        INSERT INTO campaign_schedule (
            title,
            description,
            location,
            scheduled_date,
            scheduled_time,
            status,
            display_order,
            is_published,
            published_at
        )
        VALUES ($1, $2, $3, $4, $5, 'scheduled', $6, FALSE, NULL)
        RETURNING *
        "#,
    )
    .bind(&title)
    .bind(&description)
    .bind(&location)
    .bind(payload.scheduled_date)
    .bind(payload.scheduled_time)
    .bind(payload.display_order)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Campaign schedule creation error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to create programme entry.",
        )
    })?;

    Ok((StatusCode::CREATED, Json(entry)))
}

// ADMIN: LIST ALL PROGRAMME ENTRIES
pub async fn list_schedule_entries(
    State(state): State<AppState>,
) -> Result<Json<Vec<CampaignScheduleEntry>>, (StatusCode, Json<AdminApiError>)> {
    let entries = sqlx::query_as::<_, CampaignScheduleEntry>(
        r#"
        SELECT *
        FROM campaign_schedule
        ORDER BY
            scheduled_date ASC,
            display_order ASC,
            scheduled_time ASC NULLS LAST,
            created_at ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Campaign schedule listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve programme entries.",
        )
    })?;

    Ok(Json(entries))
}

// ADMIN: GET ONE PROGRAMME ENTRY
pub async fn get_schedule_entry(
    State(state): State<AppState>,
    Path(entry_id): Path<Uuid>,
) -> Result<Json<CampaignScheduleEntry>, (StatusCode, Json<AdminApiError>)> {
    let entry = sqlx::query_as::<_, CampaignScheduleEntry>(
        r#"
        SELECT *
        FROM campaign_schedule
        WHERE id = $1
        "#,
    )
    .bind(entry_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Campaign schedule retrieval error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve programme entry.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Programme entry not found."))?;

    Ok(Json(entry))
}

// ADMIN: UPDATE PROGRAMME ENTRY
pub async fn update_schedule_entry(
    State(state): State<AppState>,
    Path(entry_id): Path<Uuid>,
    Json(payload): Json<UpdateScheduleRequest>,
) -> Result<Json<CampaignScheduleEntry>, (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();
    let description = clean_optional(payload.description);
    let location = clean_optional(payload.location);

    validate_fields(&title, &location, payload.display_order)?;

    let entry = sqlx::query_as::<_, CampaignScheduleEntry>(
        r#"
        UPDATE campaign_schedule
        SET
            title = $2,
            description = $3,
            location = $4,
            scheduled_date = $5,
            scheduled_time = $6,
            display_order = $7,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(entry_id)
    .bind(&title)
    .bind(&description)
    .bind(&location)
    .bind(payload.scheduled_date)
    .bind(payload.scheduled_time)
    .bind(payload.display_order)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Campaign schedule update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update programme entry.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Programme entry not found."))?;

    Ok(Json(entry))
}

// ADMIN: CHANGE PROGRAMME STATUS
pub async fn update_schedule_status(
    State(state): State<AppState>,
    Path(entry_id): Path<Uuid>,
    Json(payload): Json<UpdateScheduleStatusRequest>,
) -> Result<Json<CampaignScheduleEntry>, (StatusCode, Json<AdminApiError>)> {
    let status = payload.status.trim().to_lowercase();

    if !matches!(status.as_str(), "scheduled" | "completed" | "cancelled") {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Status must be scheduled, completed, or cancelled.",
        ));
    }

    let entry = sqlx::query_as::<_, CampaignScheduleEntry>(
        r#"
        UPDATE campaign_schedule
        SET
            status = $2,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(entry_id)
    .bind(&status)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Campaign schedule status error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update programme status.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Programme entry not found."))?;

    Ok(Json(entry))
}

// ADMIN: CHANGE DISPLAY ORDER
pub async fn update_schedule_order(
    State(state): State<AppState>,
    Path(entry_id): Path<Uuid>,
    Json(payload): Json<UpdateScheduleOrderRequest>,
) -> Result<Json<CampaignScheduleEntry>, (StatusCode, Json<AdminApiError>)> {
    if !(0..=1_000_000).contains(&payload.display_order) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Display order must be between 0 and 1000000.",
        ));
    }

    let entry = sqlx::query_as::<_, CampaignScheduleEntry>(
        r#"
        UPDATE campaign_schedule
        SET
            display_order = $2,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(entry_id)
    .bind(payload.display_order)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Campaign schedule ordering error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to change programme display order.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Programme entry not found."))?;

    Ok(Json(entry))
}

// ADMIN: PUBLISH OR UNPUBLISH PROGRAMME ENTRY
pub async fn update_schedule_publication(
    State(state): State<AppState>,
    Path(entry_id): Path<Uuid>,
    Json(payload): Json<UpdateSchedulePublicationRequest>,
) -> Result<Json<CampaignScheduleEntry>, (StatusCode, Json<AdminApiError>)> {
    let entry = sqlx::query_as::<_, CampaignScheduleEntry>(
        r#"
        UPDATE campaign_schedule
        SET
            is_published = $2,
            published_at = CASE
                WHEN $2 = TRUE
                    THEN COALESCE(published_at, NOW())
                ELSE NULL
            END,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(entry_id)
    .bind(payload.is_published)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Campaign schedule publication error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update programme publication.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Programme entry not found."))?;

    Ok(Json(entry))
}

// ADMIN: DELETE PROGRAMME ENTRY
pub async fn delete_schedule_entry(
    State(state): State<AppState>,
    Path(entry_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<AdminApiError>)> {
    let result = sqlx::query("DELETE FROM campaign_schedule WHERE id = $1")
        .bind(entry_id)
        .execute(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Campaign schedule deletion error: {error}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to delete programme entry.",
            )
        })?;

    if result.rows_affected() == 0 {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Programme entry not found.",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

// PUBLIC: LIST PUBLISHED PROGRAMME ENTRIES
// period supports upcoming, past, or all. Defaults to upcoming.
pub async fn list_public_schedule(
    State(state): State<AppState>,
    Query(query): Query<PublicScheduleQuery>,
) -> Result<Json<Vec<CampaignScheduleEntry>>, (StatusCode, Json<AdminApiError>)> {
    let period = query
        .period
        .unwrap_or_else(|| "upcoming".to_string())
        .trim()
        .to_lowercase();

    if !matches!(period.as_str(), "upcoming" | "past" | "all") {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Period must be upcoming, past, or all.",
        ));
    }

    let entries = sqlx::query_as::<_, CampaignScheduleEntry>(
        r#"
        SELECT *
        FROM campaign_schedule
        WHERE is_published = TRUE
          AND published_at IS NOT NULL
          AND (
              $1 = 'all'
              OR ($1 = 'upcoming' AND scheduled_date >= CURRENT_DATE)
              OR ($1 = 'past' AND scheduled_date < CURRENT_DATE)
          )
        ORDER BY
            scheduled_date ASC,
            display_order ASC,
            scheduled_time ASC NULLS LAST
        "#,
    )
    .bind(&period)
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public campaign schedule listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve published programme entries.",
        )
    })?;

    Ok(Json(entries))
}

// PUBLIC: GET ONE PUBLISHED PROGRAMME ENTRY
pub async fn get_public_schedule_entry(
    State(state): State<AppState>,
    Path(entry_id): Path<Uuid>,
) -> Result<Json<CampaignScheduleEntry>, (StatusCode, Json<AdminApiError>)> {
    let entry = sqlx::query_as::<_, CampaignScheduleEntry>(
        r#"
        SELECT *
        FROM campaign_schedule
        WHERE id = $1
          AND is_published = TRUE
          AND published_at IS NOT NULL
        "#,
    )
    .bind(entry_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public programme retrieval error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve programme entry.",
        )
    })?
    .ok_or_else(|| {
        api_error(
            StatusCode::NOT_FOUND,
            "Published programme entry not found.",
        )
    })?;

    Ok(Json(entry))
}
