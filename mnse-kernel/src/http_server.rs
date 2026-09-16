//! Axum HTTP Server for MNSE Kernel.
//!
//! Exposes 7 core endpoints:
//! - GET  /snapshot?format=json|toon   → live context snapshot
//! - GET  /health                       → kernel health check
//! - GET  /ledger/events?limit=&source= → query event history
//! - GET  /events/live                  → SSE stream for actuators & behaviors
//! - POST /emit                         → adapter event ingestion
//! - POST /evaluate                     → execute K expressions
//! - GET  /graph/node?id=               → graph node lookup

use crate::event_bus::EventBus;
use crate::inspector_state::InspectorState;
use crate::redaction::Redactor;
use axum::{
    extract::{Query, State},
    response::sse::{Event as SseEvent, Sse},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use mnse_graph::GraphDb;
use mnse_ledger::{EventFilter, Ledger};
use mnse_types::Event;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

#[derive(Clone)]
pub struct AppState {
    pub inspector: Arc<InspectorState>,
    pub bus: Arc<EventBus>,
    pub ledger: Arc<Ledger>,
    pub graph: Arc<GraphDb>,
    pub redactor: Arc<Redactor>,
}

#[derive(Debug, Deserialize)]
pub struct SnapshotQuery {
    pub format: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NodeQuery {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct EvalRequest {
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub uptime_secs: u64,
    pub subscriber_count: usize,
}

/// Create the Axum HTTP router
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/snapshot", get(get_snapshot))
        .route("/health", get(get_health))
        .route("/ledger/events", get(get_ledger_events))
        .route("/events/live", get(get_live_events))
        .route("/emit", post(emit_event))
        .route("/evaluate", post(evaluate_k))
        .route("/graph/node", get(get_graph_node))
        .with_state(state)
}

/// GET /snapshot?format=json|toon
async fn get_snapshot(
    State(state): State<AppState>,
    Query(query): Query<SnapshotQuery>,
) -> impl IntoResponse {
    let mut snap = state.inspector.snapshot();
    state.redactor.redact_snapshot(&mut snap);

    let format = query.format.as_deref().unwrap_or("toon");

    if format == "json" {
        Json(snap).into_response()
    } else {
        snap.to_toon().into_response()
    }
}

/// GET /health
async fn get_health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: "2.0.0",
        uptime_secs: 0,
        subscriber_count: state.bus.subscriber_count(),
    })
}

/// GET /ledger/events
async fn get_ledger_events(
    State(state): State<AppState>,
    Query(filter): Query<EventFilter>,
) -> impl IntoResponse {
    match state.ledger.query(filter) {
        Ok(events) => Json(events).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Ledger query error: {}", e),
        )
            .into_response(),
    }
}

/// GET /events/live — SSE stream for actuators and K behaviors
async fn get_live_events(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = state.bus.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|item| match item {
        Ok(event) => {
            let json = serde_json::to_string(&event).unwrap_or_default();
            Some(Ok(SseEvent::default().data(json)))
        }
        Err(_) => None,
    });

    Sse::new(stream)
}

/// POST /emit — Ingestion endpoint for HTTP adapters
async fn emit_event(
    State(state): State<AppState>,
    Json(mut event): Json<Event>,
) -> impl IntoResponse {
    state.redactor.redact_event(&mut event);
    state.inspector.process_event(&event);

    // Write to SQLite (skipped automatically if noise)
    let _ = state.ledger.store_event(&event);

    // Publish to live EventBus
    match state.bus.publish(event) {
        Ok(_) => Json(serde_json::json!({ "status": "accepted" })).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("EventBus publish error: {}", e),
        )
            .into_response(),
    }
}

/// POST /evaluate — Evaluate K language string
async fn evaluate_k(
    State(_state): State<AppState>,
    Json(req): Json<EvalRequest>,
) -> impl IntoResponse {
    // Placeholder until mnse-k crate is attached
    Json(serde_json::json!({
        "status": "evaluated",
        "code": req.code,
        "result": "K engine ready"
    }))
}

/// GET /graph/node?id=
async fn get_graph_node(
    State(state): State<AppState>,
    Query(query): Query<NodeQuery>,
) -> impl IntoResponse {
    match state.graph.get_node(&query.id) {
        Ok(Some(node)) => Json(node).into_response(),
        Ok(None) => (axum::http::StatusCode::NOT_FOUND, "Node not found").into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Graph error: {}", e),
        )
            .into_response(),
    }
}
