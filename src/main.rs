mod addrs;
mod allowlist;
mod config;
mod sysinfo;
#[cfg(windows)]
mod winsvc;

use std::{
    future::Future,
    io::{self, IsTerminal},
    net::SocketAddr,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{Arc, Mutex},
};

use axum::{
    Json, Router,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use clap::Parser;
use serde_json::json;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::{net::TcpListener, runtime::Runtime, sync::watch, task::JoinSet};
use tracing_subscriber::EnvFilter;

use crate::{
    allowlist::{Allowlist, AllowlistListener},
    config::Config,
};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Path to the configuration file.
    #[arg(short, long, default_value = config::DEFAULT_PATH)]
    config: PathBuf,

    /// Validate the configuration and exit.
    #[arg(long)]
    check_config: bool,

    /// Print every IPv6 address the OS reports, unfiltered, as JSON and exit.
    #[arg(long)]
    dump_addresses: bool,

    /// Append logs to this file instead of writing them to stderr.
    #[arg(long, value_name = "PATH")]
    log_file: Option<PathBuf>,

    /// Run under the Windows Service Control Manager.
    #[cfg(windows)]
    #[arg(long)]
    service: bool,
}

struct AppState {
    config: Config,
    system: sysinfo::SystemInfo,
}

impl AppState {
    fn system(&self) -> sysinfo::SystemInfo {
        sysinfo::SystemInfo {
            hostname: sysinfo::hostname(),
            ..self.system.clone()
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Err(e) = init_tracing(cli.log_file.as_deref()) {
        eprintln!("ipv6who: cannot open log file: {e}");
        return ExitCode::FAILURE;
    }
    if cli.dump_addresses {
        return dump_addresses();
    }
    #[cfg(windows)]
    if cli.service {
        return winsvc::run(cli.config);
    }

    let Some(config) = load_config(&cli.config) else {
        return ExitCode::FAILURE;
    };
    if cli.check_config {
        println!("configuration OK");
        return ExitCode::SUCCESS;
    }
    match runtime().and_then(|rt| rt.block_on(serve(config, shutdown_signal()))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn init_tracing(log_file: Option<&Path>) -> io::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    if let Some(path) = log_file {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        builder
            .with_ansi(false)
            .with_writer(Mutex::new(file))
            .init();
    } else if std::env::var_os("JOURNAL_STREAM").is_some() {
        // journald adds its own timestamps.
        builder.without_time().with_ansi(false).init();
    } else {
        // No color codes when stderr is redirected to a file.
        let color =
            io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty());
        builder.with_ansi(color).init();
    }
    Ok(())
}

fn runtime() -> io::Result<Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
}

fn load_config(path: &Path) -> Option<Config> {
    match Config::load(path) {
        Ok((config, found)) => {
            if !found {
                tracing::warn!(path = %path.display(), "config file not found, using loopback-only defaults");
            }
            Some(config)
        }
        Err(e) => {
            tracing::error!(path = %path.display(), "invalid configuration: {e}");
            None
        }
    }
}

fn dump_addresses() -> ExitCode {
    match runtime().and_then(|rt| rt.block_on(addrs::list())) {
        Ok(all) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&all).expect("serializable")
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            tracing::error!("failed to list addresses: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Binds a listener; an unspecified IPv6 address (`[::]`) also accepts IPv4
/// on every OS (Windows defaults to IPv6-only otherwise).
fn bind(addr: SocketAddr) -> io::Result<TcpListener> {
    let socket = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
    if addr.is_ipv6() && addr.ip().is_unspecified() {
        socket.set_only_v6(false)?;
    }
    // On Windows SO_REUSEADDR would allow port hijacking; elsewhere it only
    // avoids TIME_WAIT bind failures on restart.
    #[cfg(not(windows))]
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;
    TcpListener::from_std(socket.into())
}

async fn serve(config: Config, shutdown: impl Future<Output = ()>) -> io::Result<()> {
    let allow = Allowlist::new(&config.allow);
    if allow.is_empty() {
        tracing::warn!("`allow` is empty: every connection will be rejected");
    }
    let listen = config.listen.clone();
    let system = sysinfo::detect();
    tracing::info!(system = %system.name, arch = system.arch, "starting");
    let app = router(Arc::new(AppState { config, system }));

    let (shutdown_tx, shutdown_rx) = watch::channel(());
    let mut servers = JoinSet::new();
    for addr in listen {
        let listener = bind(addr)
            .map_err(|e| io::Error::new(e.kind(), format!("failed to bind {addr}: {e}")))?;
        tracing::info!(%addr, "listening");
        let mut rx = shutdown_rx.clone();
        let server = axum::serve(AllowlistListener::new(listener, allow.clone()), app.clone())
            .with_graceful_shutdown(async move {
                let _ = rx.changed().await;
            });
        servers.spawn(async move { server.await });
    }
    drop(shutdown_rx);

    tokio::select! {
        () = shutdown => {
            tracing::info!("shutting down");
            let _ = shutdown_tx.send(());
        }
        Some(res) = servers.join_next() => {
            // A server stopped on its own; take the others down with it.
            let _ = shutdown_tx.send(());
            res.map_err(io::Error::other)??;
        }
    }
    while let Some(res) = servers.join_next().await {
        res.map_err(io::Error::other)??;
    }
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        tokio::select! {
            _ = term.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}

fn router(state: Arc<AppState>) -> Router {
    let v1 = Router::new()
        .route("/ipv6", get(get_ipv6))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_token));
    Router::new()
        .nest("/v1", v1)
        .route("/healthz", get(|| async { "ok\n" }))
        .with_state(state)
}

async fn require_token(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let Some(expected) = state.config.token.as_deref() else {
        return next.run(req).await;
    };
    let presented = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match presented {
        Some(token) if constant_time_eq(token.as_bytes(), expected.as_bytes()) => {
            next.run(req).await
        }
        _ => (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            Json(json!({ "error": "missing or invalid bearer token" })),
        )
            .into_response(),
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn get_ipv6(State(state): State<Arc<AppState>>) -> Response {
    let raw = match addrs::list().await {
        Ok(raw) => raw,
        Err(e) => {
            tracing::error!("failed to list addresses: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to list addresses" })),
            )
                .into_response();
        }
    };
    let found = addrs::select(
        raw,
        &state.config.interfaces,
        state.config.include_temporary,
    );
    let Some(best) = found.first() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "no global IPv6 address found",
                "addresses": [],
                "system": state.system(),
            })),
        )
            .into_response();
    };
    Json(json!({
        "address": best.address,
        "interface": best.interface,
        "type": best.kind,
        "addresses": found,
        "system": state.system(),
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
