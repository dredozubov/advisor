use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

fn health() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() {
    // build our application with a route
    let app = Router::new().route("/health", get(health));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
