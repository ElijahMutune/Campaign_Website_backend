use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware,
    routing::{delete, get, patch, post, put},
};

use crate::handlers::campaign_poster_handler::{
    create_poster, delete_poster, get_poster, list_posters, update_poster, update_poster_order,
    update_poster_publication,
};
use crate::handlers::campaign_schedule_handler::{
    create_schedule_entry, delete_schedule_entry, get_schedule_entry, list_schedule_entries,
    update_schedule_entry, update_schedule_order, update_schedule_publication,
    update_schedule_status,
};
use crate::handlers::candidate_handler::{
    get_candidate, update_candidate, update_candidate_publication,
};
use crate::handlers::event_handler::{
    create_event, delete_event, get_event, list_events, update_event, update_event_publication,
    update_event_status,
};
use crate::handlers::headline_handler::{
    create_headline, delete_headline, get_headline, list_headlines, update_headline,
    update_headline_status,
};
use crate::handlers::manifesto_handler::{
    create_manifesto_item, delete_manifesto_item, get_manifesto_item, list_manifesto_items,
    update_manifesto_item, update_manifesto_status,
};
use crate::handlers::news_handler::{
    create_news, delete_news, get_news, list_news, update_news, update_news_status,
};
use crate::handlers::poster_category_handler::{
    create_poster_category, delete_poster_category, get_poster_category, list_poster_categories,
    update_poster_category,
};
use crate::handlers::site_settings_handler::{get_site_settings, update_site_settings};
use crate::handlers::talent_handler::{
    add_talent_application_attachment, create_talent_category, delete_talent_category,
    get_talent_application, list_talent_application_attachments, list_talent_applications,
    list_talent_categories, remove_talent_application_attachment, update_talent_category,
    update_talent_publication, update_talent_review,
};
use crate::{
    handlers::admin_handler::{admin_status, create_editor, list_editors, update_editor_status},
    middleware::auth_middleware::require_admin,
    state::AppState,
};

use crate::handlers::navigation_handler::{
    create_navigation_item, delete_navigation_item, get_navigation_item, list_navigation_items,
    update_navigation_item, update_navigation_order, update_navigation_visibility,
};
use crate::handlers::social_link_handler::{
    create_social_link, delete_social_link, get_social_link, list_social_links, update_social_link,
    update_social_order, update_social_visibility,
};

use crate::handlers::contact_handler::{
    delete_contact_message, get_contact_message, list_contact_messages, update_contact_message,
};
use crate::handlers::document_handler::{
    create_document, delete_document, get_document, list_documents, update_document,
    update_document_order, update_document_publication, upload_document_media,
};

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/status", get(admin_status))
        .route("/users/editors", get(list_editors).post(create_editor))
        .route(
            "/users/editors/{user_id}/status",
            patch(update_editor_status),
        )
        .route(
            "/manifesto",
            get(list_manifesto_items).post(create_manifesto_item),
        )
        .route(
            "/manifesto/{item_id}",
            get(get_manifesto_item)
                .put(update_manifesto_item)
                .delete(delete_manifesto_item),
        )
        .route(
            "/manifesto/{item_id}/status",
            patch(update_manifesto_status),
        )
        .route("/headlines", get(list_headlines).post(create_headline))
        .route(
            "/headlines/{headline_id}",
            get(get_headline)
                .put(update_headline)
                .delete(delete_headline),
        )
        .route(
            "/headlines/{headline_id}/status",
            patch(update_headline_status),
        )
        .route("/candidate", get(get_candidate).put(update_candidate))
        .route("/candidate/publish", patch(update_candidate_publication))
        .route(
            "/settings",
            get(get_site_settings).put(update_site_settings),
        )
        .route("/news", get(list_news).post(create_news))
        .route(
            "/news/{article_id}",
            get(get_news).put(update_news).delete(delete_news),
        )
        .route("/news/{article_id}/status", patch(update_news_status))
        .route("/events", get(list_events).post(create_event))
        .route(
            "/events/{event_id}",
            get(get_event).put(update_event).delete(delete_event),
        )
        .route(
            "/campaign-schedule",
            get(list_schedule_entries).post(create_schedule_entry),
        )
        .route(
            "/campaign-schedule/{entry_id}",
            get(get_schedule_entry)
                .put(update_schedule_entry)
                .delete(delete_schedule_entry),
        )
        .route(
            "/campaign-schedule/{entry_id}/status",
            patch(update_schedule_status),
        )
        .route(
            "/campaign-schedule/{entry_id}/publish",
            patch(update_schedule_publication),
        )
        .route(
            "/campaign-schedule/{entry_id}/order",
            patch(update_schedule_order),
        )
        .route("/posters", get(list_posters).post(create_poster))
        .route(
            "/posters/{poster_id}",
            get(get_poster).put(update_poster).delete(delete_poster),
        )
        .route(
            "/posters/{poster_id}/publish",
            patch(update_poster_publication),
        )
        .route("/posters/{poster_id}/order", patch(update_poster_order))
        .route("/events/{event_id}/status", patch(update_event_status))
        .route(
            "/events/{event_id}/publish",
            patch(update_event_publication),
        )
        .route(
            "/poster-categories",
            get(list_poster_categories).post(create_poster_category),
        )
        .route(
            "/poster-categories/{category_id}",
            get(get_poster_category)
                .put(update_poster_category)
                .delete(delete_poster_category),
        )
        // YOUTH TALENT CATEGORIES
        .route(
            "/talent/categories",
            get(list_talent_categories).post(create_talent_category),
        )
        .route(
            "/talent/categories/{category_id}",
            put(update_talent_category).delete(delete_talent_category),
        )
        // YOUTH TALENT APPLICATIONS
        .route("/talent/applications", get(list_talent_applications))
        .route(
            "/talent/applications/{application_id}",
            get(get_talent_application),
        )
        .route(
            "/talent/applications/{application_id}/review",
            patch(update_talent_review),
        )
        .route(
            "/talent/applications/{application_id}/publish",
            patch(update_talent_publication),
        )
        .route(
            "/talent/applications/{application_id}/attachments",
            get(list_talent_application_attachments).post(add_talent_application_attachment),
        )
        .route(
            "/talent/applications/{application_id}/attachments/{media_id}",
            delete(remove_talent_application_attachment),
        )
        // NAVIGATION MANAGEMENT
        .route(
            "/navigation",
            get(list_navigation_items).post(create_navigation_item),
        )
        .route(
            "/navigation/{item_id}",
            get(get_navigation_item)
                .put(update_navigation_item)
                .delete(delete_navigation_item),
        )
        .route(
            "/navigation/{item_id}/visibility",
            patch(update_navigation_visibility),
        )
        .route(
            "/navigation/{item_id}/order",
            patch(update_navigation_order),
        )
        // DOCUMENT MANAGEMENT
        .route("/documents", get(list_documents).post(create_document))
        .route("/documents/upload", post(upload_document_media))
        .route(
            "/documents/{document_id}",
            get(get_document)
                .put(update_document)
                .delete(delete_document),
        )
        .route(
            "/documents/{document_id}/publish",
            patch(update_document_publication),
        )
        .route(
            "/documents/{document_id}/order",
            patch(update_document_order),
        )
        // SOCIAL LINKS MANAGEMENT
        .route(
            "/social-links",
            get(list_social_links).post(create_social_link),
        )
        .route(
            "/social-links/{link_id}",
            get(get_social_link)
                .put(update_social_link)
                .delete(delete_social_link),
        )
        .route(
            "/social-links/{link_id}/visibility",
            patch(update_social_visibility),
        )
        .route("/social-links/{link_id}/order", patch(update_social_order))
        // CONTACT MESSAGE MANAGEMENT
        .route("/contact-messages", get(list_contact_messages))
        .route(
            "/contact-messages/{message_id}",
            get(get_contact_message)
                .patch(update_contact_message)
                .delete(delete_contact_message),
        )
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024 + 64 * 1024))
        .route_layer(middleware::from_fn_with_state(state, require_admin))
}
