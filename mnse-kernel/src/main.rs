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

    let registry = Arc::new(mnse_kernel::AdapterRegistry::load_default());
    let gatekeeper = Arc::new(mnse_kernel::Gatekeeper::new(
        registry.clone(),
        redactor.clone(),
        config.network.rate_limit_per_conn,
    ));

    let state = AppState {
        inspector,
        bus,
        ledger: ledger.clone(),
        graph: graph.clone(),
        redactor,
        registry,
        gatekeeper,
    };

    // Spawn Unix Socket Server
    let sock_path = config.network.socket_path.clone();
    let sock_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = mnse_kernel::socket::start_socket_server(&sock_path, sock_state).await {
            tracing::error!("Unix Socket server error: {}", e);
        }
    });

    // Spawn SlotBus Shared Memory Consumer (<500ns zero-copy intake)
    let slotbus_state = state.clone();
    tokio::spawn(async move {
        match mnse_slotbus::SlotBusConsumer::new(
            mnse_slotbus::DEFAULT_SHM_NAME,
            mnse_slotbus::DEFAULT_CAPACITY,
            mnse_slotbus::DEFAULT_SLOT_SIZE,
        ) {
            Ok(consumer) => {
                tracing::info!("SlotBus shared memory active at {}", mnse_slotbus::DEFAULT_SHM_NAME);
                loop {
                    let mut drained = 0;
                    while let Some(mut event) = consumer.poll_next_event() {
                        drained += 1;
                        if slotbus_state.gatekeeper.admit(&mut event) == mnse_kernel::Admission::Admitted {
                            slotbus_state.inspector.process_event(&event);
                            let _ = slotbus_state.ledger.store_event(&event);
                            let _ = slotbus_state.bus.publish(event);
                        }
                        if drained >= 128 {
                            break;
                        }
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                }
            }
            Err(e) => {
                tracing::warn!("SlotBus shm initialization skipped: {}", e);
            }
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
