use crate::handlers::campaign_poster_handler::{
    download_public_poster, get_public_poster, list_public_posters, serve_public_poster_preview,
};
use crate::handlers::campaign_schedule_handler::{get_public_schedule_entry, list_public_schedule};
use crate::handlers::contact_handler::create_contact_message;
use crate::handlers::document_handler::{
    download_public_document, get_public_document, list_public_documents,
};
use crate::handlers::event_handler::{get_public_event, list_public_events};
use crate::handlers::headline_handler::list_public_headlines;
use crate::handlers::manifesto_handler::list_public_manifesto_items;
use crate::handlers::media_handler::serve_public_media_file;
use crate::handlers::navigation_handler::list_public_navigation;
use crate::handlers::news_handler::{get_public_news_by_slug, list_public_news};
use crate::handlers::site_settings_handler::get_public_site_settings;
use crate::handlers::social_link_handler::list_public_social_links;
use crate::handlers::talent_handler::{
    create_talent_application, get_public_talent_showcase, list_public_talent_categories,
    list_public_talent_showcase,
};
use crate::handlers::talent_upload_handler::upload_talent_attachment;
use crate::{handlers::candidate_handler::get_public_candidate, state::AppState};

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/candidate", get(get_public_candidate))
        .route("/manifesto", get(list_public_manifesto_items))
        .route("/headlines", get(list_public_headlines))
        .route("/news", get(list_public_news))
        .route("/news/{slug}", get(get_public_news_by_slug))
        .route("/events", get(list_public_events))
        .route("/events/{event_id}", get(get_public_event))
        .route("/campaign-schedule", get(list_public_schedule))
        .route(
            "/campaign-schedule/{entry_id}",
            get(get_public_schedule_entry),
        )
        // PUBLIC MEDIA: only images attached to published
        // campaign content can be retrieved through this route.
        .route("/media/{media_id}/file", get(serve_public_media_file))
        // TALENT APPLICATIONS
        .route(
            "/talent/applications/{application_id}/attachments",
            post(upload_talent_attachment)
                .layer(DefaultBodyLimit::max(20 * 1024 * 1024 + 64 * 1024)),
        )
        // CAMPAIGN POSTERS
        .route("/posters", get(list_public_posters))
        .route("/posters/{poster_id}", get(get_public_poster))
        .route(
            "/posters/{poster_id}/preview",
            get(serve_public_poster_preview),
        )
        .route("/posters/{poster_id}/download", get(download_public_poster))
        // YOUTH TALENT PORTAL
        .route("/talent/categories", get(list_public_talent_categories))
        .route("/talent/applications", post(create_talent_application))
        .route("/talent-showcase", get(list_public_talent_showcase))
        .route(
            "/talent-showcase/{application_id}",
            get(get_public_talent_showcase),
        )
        // NAVIGATION
        .route("/navigation", get(list_public_navigation))
        // DOCUMENTS
        .route("/documents", get(list_public_documents))
        .route("/documents/{document_id}", get(get_public_document))
        .route(
            "/documents/{document_id}/download",
            get(download_public_document),
        )
        // PUBLIC WEBSITE CONFIGURATION
        .route("/settings", get(get_public_site_settings))
        // PUBLIC SOCIAL LINKS
        .route("/social-links", get(list_public_social_links))
        // PUBLIC CONTACT FORM
        .route("/contact-messages", post(create_contact_message))
}
