use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::pooled_connection::bb8::Pool;
use std::sync::Arc;

pub async fn create_test_pool() -> Arc<Pool<AsyncPgConnection>> {
    dotenvy::dotenv().ok();

    let user = std::env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string());
    let password = std::env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgres".to_string());
    let host = std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port = std::env::var("POSTGRES_PORT").unwrap_or_else(|_| "5432".to_string());
    let db = std::env::var("POSTGRES_DB").unwrap_or_else(|_| "bento_alephium".to_string());

    let database_url = format!("postgresql://{}:{}@{}:{}/{}", user, password, host, port, db);

    let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(&database_url);
    let pool = Pool::builder()
        .max_size(2)
        .build(config)
        .await
        .expect("Failed to create test DB pool. Is PostgreSQL running?");

    Arc::new(pool)
}
