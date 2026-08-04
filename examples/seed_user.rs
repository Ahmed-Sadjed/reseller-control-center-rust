use reseller_control_center_rust::{config::Settings, db, utils::crypto::hash_password};

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let settings = Settings::from_env();
    let pool = db::create_pool(&settings.database_url).await?;

    let username = std::env::args().nth(1).unwrap_or_else(|| "admin".into());
    let password = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "admin12345".into());
    let role = std::env::args().nth(3).unwrap_or_else(|| "ADMIN".into());

    let hash = hash_password(&password).map_err(anyhow::Error::msg)?;

    sqlx::query(
        "INSERT INTO users (username, email, password_hash, role, is_active, is_staff, is_superuser) \
         VALUES ($1, $2, $3, $4, true, true, true) \
         ON CONFLICT (username) DO UPDATE SET password_hash = EXCLUDED.password_hash, role = EXCLUDED.role",
    )
    .bind(&username)
    .bind(format!("{username}@example.com"))
    .bind(&hash)
    .bind(&role)
    .execute(&pool)
    .await?;

    println!("seeded user {username} with role {role}");
    Ok(())
}
