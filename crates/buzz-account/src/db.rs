//! Database bootstrap: create the service's own database if it does not exist
//! yet, then apply the embedded migrations.

use sqlx::{postgres::PgPoolOptions, PgPool};

/// Connect to `database_url`, creating the database on first boot, and run
/// migrations. The compose bundle shares one Postgres instance with the relay
/// and only the relay's database is created by the image, so this service
/// creates its own.
pub async fn connect_and_migrate(database_url: &str) -> anyhow::Result<PgPool> {
    ensure_database(database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

async fn ensure_database(database_url: &str) -> anyhow::Result<()> {
    let mut target = url::Url::parse(database_url)?;
    let db_name = target.path().trim_start_matches('/').to_owned();
    anyhow::ensure!(
        !db_name.is_empty(),
        "ACCOUNT_DATABASE_URL has no database name"
    );
    anyhow::ensure!(
        db_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "ACCOUNT_DATABASE_URL database name must be [A-Za-z0-9_]"
    );

    // Same credentials, maintenance database.
    target.set_path("/postgres");
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(target.as_str())
        .await?;
    let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM pg_database WHERE datname = $1")
        .bind(&db_name)
        .fetch_optional(&admin)
        .await?;
    if exists.is_none() {
        tracing::info!(database = %db_name, "creating account database");
        // Identifier validated above; CREATE DATABASE cannot take a bind parameter.
        let created = sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "CREATE DATABASE \"{db_name}\""
        )))
        .execute(&admin)
        .await;
        if let Err(error) = created {
            // Two instances booting at once (or parallel tests) race here;
            // whoever lost the race is fine as long as the database now exists.
            let now_exists: Option<i32> =
                sqlx::query_scalar("SELECT 1 FROM pg_database WHERE datname = $1")
                    .bind(&db_name)
                    .fetch_optional(&admin)
                    .await?;
            if now_exists.is_none() {
                return Err(error.into());
            }
        }
    }
    admin.close().await;
    Ok(())
}
