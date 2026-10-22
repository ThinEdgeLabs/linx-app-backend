use diesel::{
    backend::Backend,
    migration::{self, Migration, MigrationSource},
    ConnectionResult,
};
use diesel_async::{
    pooled_connection::{
        bb8::{Pool, PooledConnection},
        AsyncDieselConnectionManager, ManagerConfig, PoolError,
    },
    AsyncPgConnection,
};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness};
use futures_util::{future::BoxFuture, FutureExt};
use std::{sync::Arc, time::Duration};

pub const DEFAULT_MAX_POOL_SIZE: u32 = 150;

// Database pool type aliases
pub type DbPool = Pool<AsyncPgConnection>;

// Database pool connection type aliases
pub type DbPoolConnection<'a> = PooledConnection<'a, AsyncPgConnection>;

// Establish a connection to the database
fn establish_connection(database_url: &str) -> BoxFuture<'_, ConnectionResult<AsyncPgConnection>> {
    use native_tls::{Certificate, TlsConnector};
    use postgres_native_tls::MakeTlsConnector;

    (async move {
        let (url, cert_path) = parse_and_clean_db_url(database_url);
        let cert = std::fs::read(cert_path.unwrap()).expect("Could not read certificate");

        let cert = Certificate::from_pem(&cert).expect("Could not parse certificate");
        let connector = TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .add_root_certificate(cert)
            .build()
            .expect("Could not build TLS connector");
        let connector = MakeTlsConnector::new(connector);

        let (client, connection) =
            tokio_postgres::connect(&url, connector).await.expect("Could not connect to database");
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });
        AsyncPgConnection::try_from(client).await
    })
    .boxed()
}

// Parse the database URL and remove the sslrootcert parameter
fn parse_and_clean_db_url(url: &str) -> (String, Option<String>) {
    let mut db_url = url::Url::parse(url).expect("Could not parse database url");
    let mut cert_path = None;

    let mut query = "".to_string();
    db_url.query_pairs().for_each(|(k, v)| {
        if k == "sslrootcert" {
            cert_path = Some(v.parse().unwrap());
        } else {
            query.push_str(&format!("{}={}&", k, v));
        }
    });
    db_url.set_query(Some(&query));

    (db_url.to_string(), cert_path)
}

// Create a new database pool
pub async fn new_db_pool(database_url: &str, max_pool_size: Option<u32>) -> Result<Arc<DbPool>, PoolError> {
    let (_url, cert_path) = parse_and_clean_db_url(database_url);

    let config = if cert_path.is_some() {
        let mut config = ManagerConfig::<AsyncPgConnection>::default();
        config.custom_setup = Box::new(|conn| Box::pin(establish_connection(conn)));
        AsyncDieselConnectionManager::<AsyncPgConnection>::new_with_config(database_url, config)
    } else {
        AsyncDieselConnectionManager::<AsyncPgConnection>::new(database_url)
    };
    let pool = Pool::builder()
        .max_size(max_pool_size.unwrap_or(DEFAULT_MAX_POOL_SIZE))
        .connection_timeout(Duration::from_secs(10))
        .build(config)
        .await?;

    Ok(Arc::new(pool))
}

// Diesel only accepts an owned migration source, and `EmbeddedMigrations` can't be cloned
struct StaticMigrations(&'static EmbeddedMigrations);

impl<DB: Backend> MigrationSource<DB> for StaticMigrations {
    fn migrations(&self) -> migration::Result<Vec<Box<dyn Migration<DB>>>> {
        MigrationSource::<DB>::migrations(self.0)
    }
}

// Run pending migrations
pub fn run_pending_migrations<DB: Backend>(
    conn: &mut impl MigrationHarness<DB>,
    migrations: &'static EmbeddedMigrations,
) {
    conn.run_pending_migrations(StaticMigrations(migrations)).expect("[Parser] Migrations failed!");
}
