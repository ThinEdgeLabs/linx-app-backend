use crate::error::AppError;
use anyhow::Result;
use axum::{extract::State, response::IntoResponse, routing::get};
use bento_core::Client;
use bento_trait::stage::BlockProvider;
use bento_types::{repository::get_latest_block, ChainInfo, DbPool};
use handler::{BlockApiModule, EventApiModule, TransactionApiModule};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use utoipa::{openapi::Info, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_swagger_ui::SwaggerUi;

pub mod error;
pub mod handler;

#[derive(Clone, Debug)]
pub struct Config {
    pub db_client: Arc<DbPool>,
    pub node_client: Arc<Client>,
    pub api_host: String,
    pub api_port: u16,
}

impl Config {
    pub fn api_endpoint(&self) -> String {
        format!("{}:{}", self.api_host, self.api_port)
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<DbPool>,
    pub node_client: Arc<Client>,
}
use std::str::FromStr;

#[derive(Debug, Clone, Default, Deserialize, ToSchema, Serialize)]
pub struct Pagination {
    #[serde(
        default = "Pagination::default_offset",
        deserialize_with = "deserialize_number_from_string"
    )]
    pub offset: i64,

    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub limit: i64,
}

// Custom deserializer for string to i64 conversion
pub fn deserialize_number_from_string<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = String::deserialize(deserializer)?;
    if s.is_empty() {
        return Ok(0);
    }
    i64::from_str(&s).map_err(serde::de::Error::custom)
}

impl Pagination {
    pub fn get_offset(&self) -> i64 {
        if self.offset < 0 {
            return 0;
        }
        self.offset
    }

    pub fn get_limit(&self) -> i64 {
        if self.limit <= 0 || self.limit > 100 {
            return 10;
        }
        self.limit
    }

    pub fn default_offset() -> i64 {
        0
    }

    pub fn default_limit() -> i64 {
        10
    }
}

pub async fn start(config: Config, custom_router: Option<OpenApiRouter<AppState>>) -> Result<()> {
    let state = AppState { db: config.clone().db_client, node_client: config.clone().node_client };

    let (app, mut api) = configure_api(custom_router).with_state(state).split_for_parts();

    api.info = Info::new("REST API", "v1");
    api.info.description = Some("Bento Alephium Indexer REST API".to_string());
    let app = app
        .layer(CorsLayer::permissive())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", api.clone()));

    let addr = config.api_endpoint();
    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app).await?;

    Ok(())
}

async fn root() -> &'static str {
    "Hello Alephium Indexer API"
}

async fn health_check(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let from_group = 0;
    let to_group = 0;
    let chain_info: ChainInfo = state.node_client.get_chain_info(from_group, to_group).await?;
    let remote_height = chain_info.current_height;
    let block = get_latest_block(&state.db, from_group as i64, to_group as i64).await?;
    let block_height = block.map(|b| b.height);

    block_height.map_or(
        Err(AppError::Internal(anyhow::anyhow!("No block found in database"))),
        |h| {
            if remote_height - h > 3 {
                Err(AppError::Internal(anyhow::anyhow!("Indexer is too far behind the node")))
            } else {
                Ok(())
            }
        },
    )
}

#[allow(clippy::let_and_return)]
pub fn configure_api(custom_router: Option<OpenApiRouter<AppState>>) -> OpenApiRouter<AppState> {
    let router = OpenApiRouter::new()
        .nest("/v1/blocks", BlockApiModule::register())
        .nest("/v1/events", EventApiModule::register())
        .nest("/v1/transactions", TransactionApiModule::register())
        .route("/", get(root))
        .route("/v1/health", get(health_check));

    if let Some(custom_router) = custom_router {
        router.merge(custom_router)
    } else {
        router
    }
}
