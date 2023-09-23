mod ws_host;

use crate::ws_host::WebSocketHost;
use anyhow::Context;
use axum::routing::get;
use axum::Router;
use clap::Parser;
use clap_derive::Parser;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::future::ready;
use std::net::SocketAddr;
use tokio::select;
use tokio::signal::unix::{signal, SignalKind};
use tracing::info;

#[derive(Parser, Debug)]
struct Args {
    #[arg(
        long,
        value_name = "listen-address",
        help = "HTTP listen address",
        default_value = "0.0.0.0:8080"
    )]
    listen_address: String,

    #[arg(
        long,
        value_name = "rpc-server",
        help = "RPC server address",
        default_value = "tcp://0.0.0.0:7899"
    )]
    rpc_server: String,

    #[arg(
        long,
        value_name = "narrative-server",
        help = "Narrative server address",
        default_value = "tcp://0.0.0.0:7898"
    )]
    narrative_server: String,
}

fn setup_metrics_recorder() -> anyhow::Result<PrometheusHandle> {
    PrometheusBuilder::new()
        .install_recorder()
        .with_context(|| "Unable to install Prometheus recorder")
}

fn mk_routes(ws_host: WebSocketHost) -> anyhow::Result<Router> {
    let recorder_handle = setup_metrics_recorder()?;

    let websocket_router = Router::new()
        .route("/connect", get(ws_host::ws_connect_handler))
        .route("/create", get(ws_host::ws_create_handler))
        .with_state(ws_host);

    Ok(Router::new()
        .nest("/ws", websocket_router)
        // .nest("/properties", property_router)
        .route("/metrics", get(move || ready(recorder_handle.render()))))
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), anyhow::Error> {
    let args: Args = Args::parse();

    let main_subscriber = tracing_subscriber::fmt()
        .compact()
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(main_subscriber)
        .expect("Unable to set configure logging");

    let mut hup_signal =
        signal(SignalKind::hangup()).expect("Unable to register HUP signal handler");
    let mut stop_signal =
        signal(SignalKind::interrupt()).expect("Unable to register STOP signal handler");

    let ws_host = WebSocketHost::new(args.rpc_server, args.narrative_server);

    let main_router = mk_routes(ws_host).expect("Unable to create main router");

    let address = &args.listen_address.parse::<SocketAddr>().unwrap();
    info!(address=?address, "Listening");
    let axum_server = tokio::spawn(
        axum::Server::bind(address)
            .serve(main_router.into_make_service_with_connect_info::<SocketAddr>()),
    );

    select! {
        _ = hup_signal.recv() => {
            info!("HUP received, stopping...");
            axum_server.abort();
        },
        _ = stop_signal.recv() => {
            info!("STOP received, stopping...");
            axum_server.abort();
        }
    }
    Ok(())
}