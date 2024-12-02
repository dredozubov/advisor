use advisor::{
    auth::AuthUser,
    core::config::AdvisorConfig,
    memory::ConversationManager,
};
use axum::{
    extract::State,
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

// Request/Response types
#[derive(Deserialize)]
struct CreateConversationRequest {
    summary: String,
    tickers: Vec<String>,
}

#[derive(Serialize)]
struct CreateConversationResponse {
    id: Uuid,
}

#[derive(Serialize)]
struct ConversationResponse {
    id: Uuid,
    summary: String,
    tickers: Vec<String>,
    created_at: time::OffsetDateTime,
    updated_at: time::OffsetDateTime,
}

// Shared application state
struct AppState {
    conversation_manager: Arc<RwLock<ConversationManager>>,
}

// Health check endpoint
async fn health() -> &'static str {
    "OK"
}

// Create new conversation
async fn create_conversation(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Json(req): Json<CreateConversationRequest>,
) -> Result<Json<CreateConversationResponse>, (StatusCode, String)> {
    let conversation_id = state
        .conversation_manager
        .write()
        .await
        .create_conversation(req.summary, req.tickers)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(CreateConversationResponse {
        id: conversation_id,
    }))
}

// List conversations
async fn list_conversations(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
) -> Result<Json<Vec<ConversationResponse>>, (StatusCode, String)> {
    let conversations = state
        .conversation_manager
        .read()
        .await
        .list_conversations()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(
        conversations
            .into_iter()
            .map(|c| ConversationResponse {
                id: c.id,
                summary: c.summary,
                tickers: c.tickers,
                created_at: c.created_at,
                updated_at: c.updated_at,
            })
            .collect(),
    ))
}

// Delete conversation
async fn delete_conversation(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    path: axum::extract::Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let conversation_id = path.0;

    // First verify the conversation exists and belongs to the user
    if state
        .conversation_manager
        .read()
        .await
        .get_conversation(&conversation_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .is_none()
    {
        return Err((StatusCode::NOT_FOUND, "Conversation not found".to_string()));
    }

    // TODO: Implement conversation deletion in ConversationManager
    Ok(StatusCode::NO_CONTENT)
}

// Switch to conversation
async fn switch_conversation(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    path: axum::extract::Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let conversation_id = path.0;

    state
        .conversation_manager
        .write()
        .await
        .switch_conversation(&conversation_id)
        .await
        .map_err(|e| match e.to_string().as_str() {
            "Conversation not found" => (StatusCode::NOT_FOUND, e.to_string()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        })?;

    Ok(StatusCode::OK)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    let config = AdvisorConfig::from_env()?;

    // Initialize database connection
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(16)
        .connect(&config.database_url)
        .await?;

    // Initialize conversation manager with a default user ID
    // The actual user ID will be taken from JWT for each request
    let conversation_manager = ConversationManager::new(pool, Uuid::nil());
    let app_state = Arc::new(AppState {
        conversation_manager: Arc::new(RwLock::new(conversation_manager)),
    });

    // Build router with all routes
    let app = Router::new()
        .route("/health", get(health))
        .route("/conversations", post(create_conversation))
        .route("/conversations", get(list_conversations))
        .route("/conversations/:id", delete(delete_conversation))
        .route("/conversations/:id/switch", post(switch_conversation))
        .with_state(app_state);

    // Run server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    println!("Server running on http://0.0.0.0:8000");
    axum::serve(listener, app).await?;

    Ok(())
}
