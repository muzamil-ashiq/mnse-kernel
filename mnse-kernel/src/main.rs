//! MNSE Kernel Daemon Main Entrypoint.

use mnse_graph::{GraphBuilder, GraphDb, SessionManager};
use mnse_kernel::{
    config::KernelConfig, create_router, AppState, EventBus, InspectorState, Redactor,
};
use mnse_ledger::Ledger;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("🚀 Starting MNSE Kernel v2.0.0...");

    let config = KernelConfig::load();

    let ledger = Arc::new(Ledger::open(&config.storage.db_path)?);
    tracing::info!("✅ SQLite Ledger opened at {}", config.storage.db_path);

    let graph = Arc::new(GraphDb::new(&config.storage.graph_path)?);
    tracing::info!("✅ Knowledge Graph DB opened at {}", config.storage.graph_path);

    let inspector = Arc::new(InspectorState::new(config.redaction.privacy_mode));
    let redactor = Arc::new(Redactor::new(config.redaction.privacy_mode));
    let bus = Arc::new(EventBus::new(config.event_bus.capacity));

    // Rehydrate inspector state from recent events
    if let Ok(recent) = ledger.recent_events(100) {
        tracing::info!("Rehydrating live context snapshot from {} recent events...", recent.len());
        for ev in &recent {
            inspector.process_event(ev);
        }
    }

    // Initialize SessionManager & GraphBuilder background workers
    let session_mgr = Arc::new(SessionManager::new(graph.clone()));
    let graph_builder = Arc::new(GraphBuilder::new(graph.clone()));

    let mut bus_rx = bus.subscribe();
    let s_mgr = session_mgr.clone();
    let g_builder = graph_builder.clone();

    tokio::spawn(async move {
        while let Ok(event) = bus_rx.recv().await {
            s_mgr.process_event(&event).await;
            g_builder.process_event(&event).await;
        }
    });

    let state = AppState {
        inspector,
        bus,
        ledger: ledger.clone(),
        graph: graph.clone(),
        redactor,
    };

    // Spawn Unix Socket Server
    let sock_path = config.network.socket_path.clone();
    let sock_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = mnse_kernel::socket::start_socket_server(&sock_path, sock_state).await {
            tracing::error!("Unix Socket server error: {}", e);
        }
    });

    // Spawn HTTP Server
    let addr: SocketAddr = config.network.http_bind.parse()?;
    let router = create_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("✅ HTTP Server listening at http://{}", addr);

    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    // Handle Graceful Shutdown
    signal::ctrl_c().await?;
    tracing::info!("🛑 Shutting down MNSE Kernel gracefully...");

    server_handle.abort();
    Ok(())
}
