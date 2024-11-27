use advisor::{
    core::{config::AdvisorConfig, init},
    edgar::filing,
    eval,
    memory::{ConversationChainManager, ConversationManager},
};
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures::StreamExt;
use hyper::server::Server;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize)]
struct QueryRequest {
    conversation_id: Option<Uuid>,
    input: String,
}

#[derive(Debug, Serialize)]
struct QueryResponse {
    content: String,
    summary: String,
}

struct AppState {
    conversation_manager: Arc<RwLock<ConversationManager>>,
    chain_manager: Arc<ConversationChainManager>,
    store: Arc<langchain_rust::vectorstore::pgvector::Store>,
    http_client: reqwest::Client,
    stream_chain: Arc<langchain_rust::chain::ConversationalChain>,
    query_chain: Arc<langchain_rust::chain::ConversationalChain>,
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            conversation_manager: Arc::clone(&self.conversation_manager),
            chain_manager: Arc::clone(&self.chain_manager),
            store: Arc::clone(&self.store),
            http_client: self.http_client.clone(),
            stream_chain: Arc::clone(&self.stream_chain),
            query_chain: Arc::clone(&self.query_chain),
        }
    }
}

async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}

async fn query(
    State(state): State<AppState>,
    Json(request): Json<QueryRequest>,
) -> Result<Json<QueryResponse>, (StatusCode, Json<ErrorResponse>)> {
    let conversation = if let Some(conv_id) = request.conversation_id {
        state
            .conversation_manager
            .read()
            .await
            .get_conversation(&conv_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: e.to_string(),
                    }),
                )
            })?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: "Conversation not found".to_string(),
                    }),
                )
            })?
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "conversation_id is required".to_string(),
            }),
        ));
    };

    let (mut stream, summary) = eval::eval(
        &request.input,
        &conversation,
        &state.http_client,
        &state.stream_chain,
        &state.query_chain,
        Arc::clone(&state.store),
        state.conversation_manager.clone(),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    let mut content = String::new();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(c) => content.push_str(&c),
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Stream error: {}", e),
                    }),
                ))
            }
        }
    }

    Ok(Json(QueryResponse { content, summary }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    env_logger::init();

    let config = AdvisorConfig::from_env()?;
    
    let llm = init::initialize_openai(&config).await?;
    let store = init::initialize_vector_store(&config).await?;

    let pg_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(16)
        .connect(&config.database_url)
        .await?;

    let (stream_chain, query_chain) = init::initialize_chains(llm.clone()).await?;

    let http_client = reqwest::Client::builder()
        .user_agent(filing::USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?;

    let conversation_manager = ConversationManager::new(pg_pool.clone());
    let chain_manager = ConversationChainManager::new(pg_pool);

    let app_state = AppState {
        conversation_manager: Arc::new(RwLock::new(conversation_manager)),
        chain_manager: Arc::new(chain_manager),
        store,
        http_client,
        stream_chain: Arc::new(stream_chain),
        query_chain: Arc::new(query_chain),
    };

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/query", post(query))
        .layer(CorsLayer::permissive())
        .with_state(app_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    log::info!("Starting server on {}", addr);
    
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
