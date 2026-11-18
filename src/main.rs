mod database;
mod handlers;
mod middleware;
mod models;
mod routes;
mod services;
mod setup;
mod state;

use axum::{
    Json, Router,
    http::{
        HeaderValue, Method,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
    routing::get,
};
use dotenvy::dotenv;
use serde_json::{Value, json};
use std::env;
use tokio::net::TcpListener;
use tower_http::cors::{AllowOrigin, CorsLayer};

use database::connect_database;
use state::AppState;

async fn health() -> Json<Value> {
    Json(json!({
        "success": true,
        "message": "Matungulu Campaign Backend is running",
        "backend": "Rust + Axum",
        "database": "connected"
    }))
}

#[tokio::main]
async fn main() {
    // Load local .env during development.
    // Production hosting supplies these variables independently.
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be configured");

    let jwt_secret = env::var("JWT_SECRET").expect("JWT_SECRET must be configured");

    if jwt_secret.len() < 32 {
        panic!("JWT_SECRET must contain at least 32 characters");
    }

    // Connect to PostgreSQL.
    let pool = connect_database(&database_url)
        .await
        .expect("Failed to connect to PostgreSQL");

    println!("PostgreSQL connected successfully");

    // Automatically apply pending database migrations.
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run database migrations");

    println!("Database migrations completed successfully");

    // Initial administrator setup command.
    if env::args().nth(1).as_deref() == Some("create-admin") {
        if let Err(error) = setup::admin::create_initial_admin(&pool).await {
            eprintln!("Administrator setup failed: {error}");
            std::process::exit(1);
        }

        return;
    }

    let state = AppState {
        db: pool,
        jwt_secret,
    };

    // Configure permitted frontend origins.
    let cors_origins = env::var("CORS_ALLOWED_ORIGINS").unwrap_or_else(|_| {
        concat!(
            "http://localhost:5173,",
            "http://127.0.0.1:5173,",
            "https://mjlab.co.ke,",
            "https://www.mjlab.co.ke,",
            "https://elijahmutune.github.io"
        )
        .to_string()
    });

    let allowed_origins: Vec<HeaderValue> = cors_origins
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(|origin| {
            origin.parse::<HeaderValue>().unwrap_or_else(|_| {
                panic!("Invalid CORS origin configured: {origin}");
            })
        })
        .collect();

    if allowed_origins.is_empty() {
        panic!("CORS_ALLOWED_ORIGINS must contain at least one origin");
    }

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed_origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE]);

    // Register existing application routes.
    let app = Router::new()
        .route("/api/health", get(health))
        .nest("/api/auth", routes::auth::router(state.clone()))
        .nest("/api/admin", routes::admin::router(state.clone()))
        .nest("/api/public", routes::public::router())
        .nest("/api/media", routes::media::router(state.clone()))
        .layer(cors)
        .with_state(state);

    // Hosting providers normally supply PORT automatically.
    let port = env::var("PORT").unwrap_or_else(|_| "5000".to_string());

    let bind_address = format!("0.0.0.0:{port}");

    let listener = TcpListener::bind(&bind_address)
        .await
        .expect("Failed to bind to server port");

    println!("Matungulu Campaign Backend started");
    println!("Listening on {bind_address}");
    println!("Health endpoint: /api/health");

    axum::serve(listener, app).await.expect("Server failed");
}
