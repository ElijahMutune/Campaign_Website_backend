use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{NaiveDate, NaiveTime};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    handlers::admin_handler::AdminApiError, models::event::CampaignEvent, state::AppState,
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

async fn validate_media(
    state: &AppState,
    media_id: Option<Uuid>,
) -> Result<(), (StatusCode, Json<AdminApiError>)> {
    let Some(media_id) = media_id else {
        return Ok(());
    };

    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM media
            WHERE id = $1
              AND media_type = 'image'
              AND deleted_at IS NULL
        )
        "#,
    )
    .bind(media_id)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Event media validation error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to validate selected event image.",
        )
    })?;

    if !exists {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Selected image does not exist in the active media library.",
        ));
    }

    Ok(())
}

fn validate_event_fields(
    title: &str,
    location: &Option<String>,
    start_time: Option<NaiveTime>,
    end_time: Option<NaiveTime>,
) -> Result<(), (StatusCode, Json<AdminApiError>)> {
    if title.is_empty() || title.chars().count() > 255 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Event title is required and must not exceed 255 characters.",
        ));
    }

    if location
        .as_ref()
        .is_some_and(|value| value.chars().count() > 255)
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Event location must not exceed 255 characters.",
        ));
    }

    if let (Some(start), Some(end)) = (start_time, end_time) {
        if end <= start {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "Event end time must be later than its start time.",
            ));
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct CreateEventRequest {
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub event_date: NaiveDate,
    pub start_time: Option<NaiveTime>,
    pub end_time: Option<NaiveTime>,
    pub media_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEventRequest {
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub event_date: NaiveDate,
    pub start_time: Option<NaiveTime>,
    pub end_time: Option<NaiveTime>,
    pub media_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEventStatusRequest {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEventPublicationRequest {
    pub is_published: bool,
}

#[derive(Debug, Deserialize)]
pub struct PublicEventQuery {
    pub period: Option<String>,
}

// ADMIN: CREATE EVENT
pub async fn create_event(
    State(state): State<AppState>,
    Json(payload): Json<CreateEventRequest>,
) -> Result<(StatusCode, Json<CampaignEvent>), (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();
    let description = clean_optional(payload.description);
    let location = clean_optional(payload.location);

    validate_event_fields(&title, &location, payload.start_time, payload.end_time)?;

    validate_media(&state, payload.media_id).await?;

    let event = sqlx::query_as::<_, CampaignEvent>(
        r#"
        INSERT INTO events (
            title,
            description,
            location,
            event_date,
            start_time,
            end_time,
            media_id,
            status,
            is_published,
            published_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'upcoming', FALSE, NULL)
        RETURNING
            id,
            title,
            description,
            location,
            event_date,
            start_time,
            end_time,
            media_id,
            status,
            created_at,
            updated_at,
            is_published,
            published_at
        "#,
    )
    .bind(&title)
    .bind(&description)
    .bind(&location)
    .bind(payload.event_date)
    .bind(payload.start_time)
    .bind(payload.end_time)
    .bind(payload.media_id)
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Event creation error: {error}");
        api_error(StatusCode::INTERNAL_SERVER_ERROR, "Unable to create event.")
    })?;

    Ok((StatusCode::CREATED, Json(event)))
}

// ADMIN: LIST ALL EVENTS
pub async fn list_events(
    State(state): State<AppState>,
) -> Result<Json<Vec<CampaignEvent>>, (StatusCode, Json<AdminApiError>)> {
    let events = sqlx::query_as::<_, CampaignEvent>(
        r#"
        SELECT
            id,
            title,
            description,
            location,
            event_date,
            start_time,
            end_time,
            media_id,
            status,
            created_at,
            updated_at,
            is_published,
            published_at
        FROM events
        ORDER BY event_date DESC, start_time DESC NULLS LAST, created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Event listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve events.",
        )
    })?;

    Ok(Json(events))
}

// ADMIN: GET ONE EVENT
pub async fn get_event(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
) -> Result<Json<CampaignEvent>, (StatusCode, Json<AdminApiError>)> {
    let event = sqlx::query_as::<_, CampaignEvent>(
        r#"
        SELECT
            id,
            title,
            description,
            location,
            event_date,
            start_time,
            end_time,
            media_id,
            status,
            created_at,
            updated_at,
            is_published,
            published_at
        FROM events
        WHERE id = $1
        "#,
    )
    .bind(event_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Event retrieval error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve event.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Event not found."))?;

    Ok(Json(event))
}

// ADMIN: UPDATE EVENT
pub async fn update_event(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
    Json(payload): Json<UpdateEventRequest>,
) -> Result<Json<CampaignEvent>, (StatusCode, Json<AdminApiError>)> {
    let title = payload.title.trim().to_string();
    let description = clean_optional(payload.description);
    let location = clean_optional(payload.location);

    validate_event_fields(&title, &location, payload.start_time, payload.end_time)?;

    validate_media(&state, payload.media_id).await?;

    let event = sqlx::query_as::<_, CampaignEvent>(
        r#"
        UPDATE events
        SET
            title = $2,
            description = $3,
            location = $4,
            event_date = $5,
            start_time = $6,
            end_time = $7,
            media_id = $8,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id,
            title,
            description,
            location,
            event_date,
            start_time,
            end_time,
            media_id,
            status,
            created_at,
            updated_at,
            is_published,
            published_at
        "#,
    )
    .bind(event_id)
    .bind(&title)
    .bind(&description)
    .bind(&location)
    .bind(payload.event_date)
    .bind(payload.start_time)
    .bind(payload.end_time)
    .bind(payload.media_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Event update error: {error}");
        api_error(StatusCode::INTERNAL_SERVER_ERROR, "Unable to update event.")
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Event not found."))?;

    Ok(Json(event))
}

// ADMIN: UPDATE EVENT STATUS
pub async fn update_event_status(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
    Json(payload): Json<UpdateEventStatusRequest>,
) -> Result<Json<CampaignEvent>, (StatusCode, Json<AdminApiError>)> {
    let status = payload.status.trim().to_lowercase();

    if !matches!(status.as_str(), "upcoming" | "completed" | "cancelled") {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Status must be upcoming, completed, or cancelled.",
        ));
    }

    let event = sqlx::query_as::<_, CampaignEvent>(
        r#"
        UPDATE events
        SET
            status = $2,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id,
            title,
            description,
            location,
            event_date,
            start_time,
            end_time,
            media_id,
            status,
            created_at,
            updated_at,
            is_published,
            published_at
        "#,
    )
    .bind(event_id)
    .bind(&status)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Event status update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update event status.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Event not found."))?;

    Ok(Json(event))
}

// ADMIN: PUBLISH OR UNPUBLISH EVENT
pub async fn update_event_publication(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
    Json(payload): Json<UpdateEventPublicationRequest>,
) -> Result<Json<CampaignEvent>, (StatusCode, Json<AdminApiError>)> {
    let event = sqlx::query_as::<_, CampaignEvent>(
        r#"
        UPDATE events
        SET
            is_published = $2,
            published_at = CASE
                WHEN $2 = TRUE THEN COALESCE(published_at, NOW())
                ELSE NULL
            END,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id,
            title,
            description,
            location,
            event_date,
            start_time,
            end_time,
            media_id,
            status,
            created_at,
            updated_at,
            is_published,
            published_at
        "#,
    )
    .bind(event_id)
    .bind(payload.is_published)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Event publication update error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update event publication.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Event not found."))?;

    Ok(Json(event))
}

// ADMIN: DELETE EVENT
// The associated media file is not deleted.
pub async fn delete_event(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<AdminApiError>)> {
    let result = sqlx::query("DELETE FROM events WHERE id = $1")
        .bind(event_id)
        .execute(&state.db)
        .await
        .map_err(|error| {
            eprintln!("Event deletion error: {error}");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "Unable to delete event.")
        })?;

    if result.rows_affected() == 0 {
        return Err(api_error(StatusCode::NOT_FOUND, "Event not found."));
    }

    Ok(StatusCode::NO_CONTENT)
}

// PUBLIC: LIST PUBLISHED EVENTS
// period supports upcoming, past, or all. Defaults to upcoming.
pub async fn list_public_events(
    State(state): State<AppState>,
    Query(query): Query<PublicEventQuery>,
) -> Result<Json<Vec<CampaignEvent>>, (StatusCode, Json<AdminApiError>)> {
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

    let events = sqlx::query_as::<_, CampaignEvent>(
        r#"
        SELECT
            id,
            title,
            description,
            location,
            event_date,
            start_time,
            end_time,
            media_id,
            status,
            created_at,
            updated_at,
            is_published,
            published_at
        FROM events
        WHERE is_published = TRUE
          AND published_at IS NOT NULL
          AND (
              $1 = 'all'
              OR ($1 = 'upcoming' AND event_date >= CURRENT_DATE)
              OR ($1 = 'past' AND event_date < CURRENT_DATE)
          )
        ORDER BY event_date ASC, start_time ASC NULLS LAST
        "#,
    )
    .bind(&period)
    .fetch_all(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public event listing error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve published events.",
        )
    })?;

    Ok(Json(events))
}

// PUBLIC: GET ONE PUBLISHED EVENT
pub async fn get_public_event(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
) -> Result<Json<CampaignEvent>, (StatusCode, Json<AdminApiError>)> {
    let event = sqlx::query_as::<_, CampaignEvent>(
        r#"
        SELECT
            id,
            title,
            description,
            location,
            event_date,
            start_time,
            end_time,
            media_id,
            status,
            created_at,
            updated_at,
            is_published,
            published_at
        FROM events
        WHERE id = $1
          AND is_published = TRUE
          AND published_at IS NOT NULL
        "#,
    )
    .bind(event_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public event retrieval error: {error}");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve event.",
        )
    })?
    .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Published event not found."))?;

    Ok(Json(event))
}
