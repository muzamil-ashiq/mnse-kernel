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

use crate::adapter_registry::AdapterRegistry;
use crate::event_bus::EventBus;
use crate::gatekeeper::{Admission, Gatekeeper};
use crate::inspector_state::InspectorState;
use crate::redaction::Redactor;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{Path as AxumPath, Query, State},
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
    pub registry: Arc<AdapterRegistry>,
    pub gatekeeper: Arc<Gatekeeper>,
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
        .route("/ledger/stats", get(get_ledger_stats))
        .route("/events/live", get(get_live_events))
        .route("/emit", post(emit_event))
        .route("/evaluate", post(evaluate_k))
        .route("/graph/node", get(get_graph_node))
        .route("/adapters", get(get_adapters))
        .route("/adapters/:name/enable", post(enable_adapter))
        .route("/adapters/:name/disable", post(disable_adapter))
        .route("/ws", get(ws_handler))
        .with_state(state)
}

/// GET /snapshot?format=json|toon
async fn get_snapshot(
    State(state): State<AppState>,
    Query(query): Query<SnapshotQuery>,
) -> impl IntoResponse {
    let mut snap = state.inspector.snapshot();
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    if let Ok(stats) = state.ledger.daily_stats(&today) {
        snap.daily_stats = stats;
    }
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

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    pub date: Option<String>,
}

/// GET /ledger/stats?date=YYYY-MM-DD
async fn get_ledger_stats(
    State(state): State<AppState>,
    Query(query): Query<StatsQuery>,
) -> impl IntoResponse {
    let date_str = query
        .date
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());
    match state.ledger.daily_stats(&date_str) {
        Ok(stats) => Json(stats).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Ledger stats error: {}", e),
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

/// POST /emit — Ingestion endpoint for HTTP adapters with Gatekeeper admission control
async fn emit_event(
    State(state): State<AppState>,
    Json(mut event): Json<Event>,
) -> impl IntoResponse {
    match state.gatekeeper.admit(&mut event) {
        Admission::Admitted => {
            state.inspector.process_event(&event);
            let _ = state.ledger.store_event(&event);
            match state.bus.publish(event) {
                Ok(_) => Json(serde_json::json!({ "status": "accepted" })).into_response(),
                Err(e) => (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("EventBus publish error: {}", e),
                )
                    .into_response(),
            }
        }
        Admission::Dropped(reason) => (
            axum::http::StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "status": "dropped",
                "reason": reason,
            })),
        )
            .into_response(),
    }
}

/// GET /adapters — List all registered adapters with live telemetry
async fn get_adapters(State(state): State<AppState>) -> impl IntoResponse {
    let registered = state.registry.list();
    let snap = state.inspector.snapshot();
    let health_map: std::collections::HashMap<String, mnse_types::AdapterStatus> = snap
        .adapters
        .into_iter()
        .map(|a| (a.name.clone(), a))
        .collect();

    let list: Vec<serde_json::Value> = registered
        .into_iter()
        .map(|entry| {
            let live = health_map.get(&entry.name);
            let connected = live.map(|l| l.connected).unwrap_or(false);
            let event_count = live.map(|l| l.event_count).unwrap_or(0);
            let last_seen = live.and_then(|l| l.last_seen);
            serde_json::json!({
                "name": entry.name,
                "enabled": entry.enabled,
                "connected": connected,
                "event_count": event_count,
                "last_seen": last_seen,
                "description": entry.description,
            })
        })
        .collect();

    Json(list)
}

/// POST /adapters/:name/enable — Turn adapter ON
async fn enable_adapter(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> impl IntoResponse {
    match state.registry.set_enabled(&name, true) {
        Ok(_) => Json(serde_json::json!({ "adapter": name, "enabled": true, "status": "ok" })).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to enable adapter: {}", e),
        )
            .into_response(),
    }
}

/// POST /adapters/:name/disable — Turn adapter OFF
async fn disable_adapter(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> impl IntoResponse {
    match state.registry.set_enabled(&name, false) {
        Ok(_) => Json(serde_json::json!({ "adapter": name, "enabled": false, "status": "ok" })).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to disable adapter: {}", e),
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

/// GET /ws — Full-duplex WebSocket stream for AI agents & reactive actuators
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.bus.subscribe();
    loop {
        tokio::select! {
            // Push incoming EventBus events to connected WebSocket client
            res = rx.recv() => {
                match res {
                    Ok(event) => {
                        if let Ok(json) = serde_json::to_string(&event) {
                            if socket.send(Message::Text(json)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            // Ingest messages from WebSocket client (bi-directional intake)
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(mut event) = serde_json::from_str::<Event>(&text) {
                            if state.gatekeeper.admit(&mut event) == Admission::Admitted {
                                state.inspector.process_event(&event);
                                let _ = state.ledger.store_event(&event);
                                let _ = state.bus.publish(event);
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

