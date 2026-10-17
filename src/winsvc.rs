//! Windows service mode (`--service`), driven by the Service Control Manager.

use std::{
    ffi::OsString,
    process::ExitCode,
    sync::{Arc, OnceLock},
    time::Duration,
};

use tokio::sync::Notify;
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
    service_dispatcher,
};

use crate::config::Config;

const SERVICE_NAME: &str = "ipv6who";

static CONFIG: OnceLock<Config> = OnceLock::new();

define_windows_service!(ffi_service_main, service_main);

/// Hands the main thread to the SCM dispatcher; returns when the service stops.
pub fn run(config: Config) -> ExitCode {
    let _ = CONFIG.set(config);
    match service_dispatcher::start(SERVICE_NAME, ffi_service_main) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("cannot connect to the service control manager: {e}");
            ExitCode::FAILURE
        }
    }
}

fn service_main(_args: Vec<OsString>) {
    if let Err(e) = run_service() {
        tracing::error!("service error: {e}");
    }
}

fn run_service() -> windows_service::Result<()> {
    let stop = Arc::new(Notify::new());
    let on_stop = stop.clone();
    let status = service_control_handler::register(SERVICE_NAME, move |control| match control {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            on_stop.notify_one();
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    })?;

    let config = CONFIG.get().expect("config set before dispatch").clone();
    set_status(
        &status,
        ServiceState::Running,
        ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        ServiceExitCode::NO_ERROR,
    )?;
    let shutdown = async move { stop.notified().await };
    let code = match crate::runtime().and_then(|rt| rt.block_on(crate::serve(config, shutdown))) {
        Ok(()) => None,
        Err(e) => {
            tracing::error!("{e}");
            Some(1)
        }
    };
    let exit = code.map_or(ServiceExitCode::NO_ERROR, ServiceExitCode::ServiceSpecific);
    set_status(
        &status,
        ServiceState::Stopped,
        ServiceControlAccept::empty(),
        exit,
    )
}

fn set_status(
    handle: &ServiceStatusHandle,
    state: ServiceState,
    accept: ServiceControlAccept,
    exit_code: ServiceExitCode,
) -> windows_service::Result<()> {
    handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: accept,
        exit_code,
        checkpoint: 0,
        wait_hint: Duration::ZERO,
        process_id: None,
    })
}
