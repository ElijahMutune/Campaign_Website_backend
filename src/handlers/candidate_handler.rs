use axum::{Json, extract::State, http::StatusCode};

use serde::Deserialize;
use uuid::Uuid;

use crate::{
    handlers::admin_handler::AdminApiError, models::candidate::CandidateProfile, state::AppState,
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
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[derive(Debug, Deserialize)]
pub struct UpdateCandidateRequest {
    pub full_name: String,
    pub position: Option<String>,
    pub constituency: Option<String>,
    pub slogan: Option<String>,
    pub biography: Option<String>,
    pub profile_media_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct PublishCandidateRequest {
    pub is_published: bool,
}

// Retrieve candidate profile for administration.
pub async fn get_candidate(
    State(state): State<AppState>,
) -> Result<Json<CandidateProfile>, (StatusCode, Json<AdminApiError>)> {
    let candidate = sqlx::query_as::<_, CandidateProfile>(
        r#"
        SELECT
            id,
            full_name,
            position,
            constituency,
            slogan,
            biography,
            profile_media_id,
            is_published,
            published_at,
            created_at,
            updated_at
        FROM candidate_profile
        LIMIT 1
        "#,
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Candidate retrieval error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve candidate profile.",
        )
    })?;

    match candidate {
        Some(candidate) => Ok(Json(candidate)),
        None => Err(api_error(
            StatusCode::NOT_FOUND,
            "Candidate profile has not been created.",
        )),
    }
}

// Create or update the single candidate profile.
pub async fn update_candidate(
    State(state): State<AppState>,
    Json(payload): Json<UpdateCandidateRequest>,
) -> Result<Json<CandidateProfile>, (StatusCode, Json<AdminApiError>)> {
    let full_name = payload.full_name.trim().to_string();
    let position = clean_optional(payload.position);
    let constituency = clean_optional(payload.constituency);
    let slogan = clean_optional(payload.slogan);
    let biography = clean_optional(payload.biography);

    if full_name.is_empty() || full_name.chars().count() > 150 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Candidate name is required and must not exceed 150 characters.",
        ));
    }

    if position.as_ref().is_some_and(|v| v.chars().count() > 150)
        || constituency
            .as_ref()
            .is_some_and(|v| v.chars().count() > 150)
        || slogan.as_ref().is_some_and(|v| v.chars().count() > 255)
        || biography
            .as_ref()
            .is_some_and(|v| v.chars().count() > 20000)
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "One or more candidate profile fields exceed the permitted length.",
        ));
    }

    // Only active image records from the media library can be selected.
    if let Some(media_id) = payload.profile_media_id {
        let image_exists: bool = sqlx::query_scalar(
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
            eprintln!("Candidate image validation error: {error}");

            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to validate profile photograph.",
            )
        })?;

        if !image_exists {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "Selected profile photograph was not found in the media library.",
            ));
        }
    }

    let mut tx = state.db.begin().await.map_err(|error| {
        eprintln!("Candidate transaction error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to save candidate profile.",
        )
    })?;

    // Serialize profile creation and updates.
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(2026111103_i64)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            eprintln!("Candidate lock error: {error}");

            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to save candidate profile.",
            )
        })?;

    let existing_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id
        FROM candidate_profile
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| {
        eprintln!("Candidate lookup error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to save candidate profile.",
        )
    })?;

    let candidate_result = match existing_id {
        Some(id) => {
            sqlx::query_as::<_, CandidateProfile>(
                r#"
                UPDATE candidate_profile
                SET
                    full_name = $2,
                    position = $3,
                    constituency = $4,
                    slogan = $5,
                    biography = $6,
                    profile_media_id = $7,
                    updated_at = NOW()
                WHERE id = $1
                RETURNING
                    id,
                    full_name,
                    position,
                    constituency,
                    slogan,
                    biography,
                    profile_media_id,
                    is_published,
                    published_at,
                    created_at,
                    updated_at
                "#,
            )
            .bind(id)
            .bind(&full_name)
            .bind(&position)
            .bind(&constituency)
            .bind(&slogan)
            .bind(&biography)
            .bind(payload.profile_media_id)
            .fetch_one(&mut *tx)
            .await
        }

        None => {
            sqlx::query_as::<_, CandidateProfile>(
                r#"
                INSERT INTO candidate_profile (
                    full_name,
                    position,
                    constituency,
                    slogan,
                    biography,
                    profile_media_id
                )
                VALUES ($1, $2, $3, $4, $5, $6)
                RETURNING
                    id,
                    full_name,
                    position,
                    constituency,
                    slogan,
                    biography,
                    profile_media_id,
                    is_published,
                    published_at,
                    created_at,
                    updated_at
                "#,
            )
            .bind(&full_name)
            .bind(&position)
            .bind(&constituency)
            .bind(&slogan)
            .bind(&biography)
            .bind(payload.profile_media_id)
            .fetch_one(&mut *tx)
            .await
        }
    };

    let candidate = candidate_result.map_err(|error| {
        eprintln!("Candidate save error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to save candidate profile.",
        )
    })?;

    tx.commit().await.map_err(|error| {
        eprintln!("Candidate commit error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to save candidate profile.",
        )
    })?;

    Ok(Json(candidate))
}

// Publish or unpublish the candidate profile.
pub async fn update_candidate_publication(
    State(state): State<AppState>,
    Json(payload): Json<PublishCandidateRequest>,
) -> Result<Json<CandidateProfile>, (StatusCode, Json<AdminApiError>)> {
    let candidate = sqlx::query_as::<_, CandidateProfile>(
        r#"
        UPDATE candidate_profile
        SET
            is_published = $1,
            published_at = CASE
                WHEN $1 THEN NOW()
                ELSE NULL
            END,
            updated_at = NOW()
        WHERE id = (
            SELECT id
            FROM candidate_profile
            LIMIT 1
        )
        RETURNING
            id,
            full_name,
            position,
            constituency,
            slogan,
            biography,
            profile_media_id,
            is_published,
            published_at,
            created_at,
            updated_at
        "#,
    )
    .bind(payload.is_published)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Candidate publication error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to update candidate publication status.",
        )
    })?;

    match candidate {
        Some(candidate) => Ok(Json(candidate)),
        None => Err(api_error(
            StatusCode::NOT_FOUND,
            "Candidate profile has not been created.",
        )),
    }
}

// Public visitors can only retrieve a published profile.
pub async fn get_public_candidate(
    State(state): State<AppState>,
) -> Result<Json<CandidateProfile>, (StatusCode, Json<AdminApiError>)> {
    let candidate = sqlx::query_as::<_, CandidateProfile>(
        r#"
        SELECT
            id,
            full_name,
            position,
            constituency,
            slogan,
            biography,
            profile_media_id,
            is_published,
            published_at,
            created_at,
            updated_at
        FROM candidate_profile
        WHERE is_published = TRUE
        LIMIT 1
        "#,
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        eprintln!("Public candidate retrieval error: {error}");

        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to retrieve candidate profile.",
        )
    })?;

    match candidate {
        Some(candidate) => Ok(Json(candidate)),
        None => Err(api_error(
            StatusCode::NOT_FOUND,
            "No published candidate profile is available.",
        )),
    }
}
