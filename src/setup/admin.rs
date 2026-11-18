use sqlx::PgPool;
use std::io::{self, Write};

use crate::services::auth_service::hash_password;

fn prompt_line(label: &str) -> Result<String, String> {
    print!("{label}");

    io::stdout().flush().map_err(|e| e.to_string())?;

    let mut input = String::new();

    io::stdin()
        .read_line(&mut input)
        .map_err(|e| e.to_string())?;

    Ok(input.trim().to_string())
}

pub async fn create_initial_admin(pool: &PgPool) -> Result<(), String> {
    // Check whether an administrator already exists.
    let existing_admins: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;

    if existing_admins > 0 {
        return Err("An administrator already exists. Initial setup is disabled.".to_string());
    }

    println!("\n=== MATUNGULU ADMIN SETUP ===\n");

    let full_name = prompt_line("Administrator full name: ")?;

    if full_name.is_empty() || full_name.chars().count() > 150 {
        return Err("Invalid full name.".to_string());
    }

    let username = prompt_line("Username: ")?.to_lowercase();

    let username_valid = (3..=50).contains(&username.chars().count())
        && username.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-' || b == b'.'
        });

    if !username_valid {
        return Err(
            "Username must be 3-50 characters and contain only lowercase letters, numbers, dots, hyphens or underscores."
                .to_string()
        );
    }

    let email_input = prompt_line("Email (optional, press Enter to skip): ")?;

    if email_input.len() > 255 {
        return Err("Email address is too long.".to_string());
    }

    let email = if email_input.is_empty() {
        None
    } else {
        Some(email_input)
    };

    let password = rpassword::prompt_password("Administrator password (minimum 12 characters): ")
        .map_err(|e| e.to_string())?;

    if !(12..=128).contains(&password.chars().count()) {
        return Err("Password must contain between 12 and 128 characters.".to_string());
    }

    let confirmation = rpassword::prompt_password("Confirm administrator password: ")
        .map_err(|e| e.to_string())?;

    if password != confirmation {
        return Err("Passwords do not match.".to_string());
    }

    // Hash password before storing it.
    let password_hash = hash_password(&password)?;

    // Serialize concurrent initial setup attempts.
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(2026111001_i64)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    // Check again while holding the database lock.
    let existing_admins: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

    if existing_admins > 0 {
        tx.rollback().await.map_err(|e| e.to_string())?;

        return Err("An administrator already exists. Setup cancelled.".to_string());
    }

    sqlx::query(
        r#"
        INSERT INTO users (
            full_name,
            username,
            email,
            password_hash,
            role,
            is_active
        )
        VALUES ($1, $2, $3, $4, 'admin', TRUE)
        "#,
    )
    .bind(&full_name)
    .bind(&username)
    .bind(&email)
    .bind(&password_hash)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("Failed to create administrator: {e}"))?;

    tx.commit().await.map_err(|e| e.to_string())?;

    println!("\nAdministrator created successfully!");
    println!("Username: {username}");
    println!("Role: admin");

    Ok(())
}
