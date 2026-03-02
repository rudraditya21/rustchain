#![allow(dead_code)]

use std::future::Future;
use std::net::SocketAddr;

use axum::routing::post;
use axum::Router;
use tokio::net::TcpListener;

use crate::rpc::handlers::handle_rpc;
use crate::rpc::types::RpcState;

pub fn build_router(state: RpcState) -> Router {
    Router::new().route("/", post(handle_rpc)).with_state(state)
}

pub async fn serve(
    bind_addr: &str,
    state: RpcState,
    shutdown_signal: impl Future<Output = ()> + Send + 'static,
) -> Result<SocketAddr, std::io::Error> {
    let listener = TcpListener::bind(bind_addr).await?;
    let local_addr = listener.local_addr()?;
    let app = build_router(state);

    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal)
            .await;
    });

    Ok(local_addr)
}
