use super::CmdResult;
use crate::{
    cmd::StringifyErr as _,
    config::Config,
    core::{
        handle::Handle,
        proxy_view::{ProxyViewBuilder, ProxyViewInput, ProxyViewV1},
        tray::Tray,
    },
    process::AsyncHandler,
};
use clash_verge_logging::{Type, logging};
use std::sync::atomic::{AtomicBool, Ordering};

/// Merges one selection pair into fresh backend state to avoid stale-list overwrites.
#[tauri::command]
#[specta::specta]
pub async fn record_selected_node(group_name: String, node: String) -> CmdResult<()> {
    crate::config::profiles::record_selected_node(&group_name, &node)
        .await
        .stringify_err()
}

#[tauri::command]
#[specta::specta]
pub async fn forget_selected_node(group_name: String) -> CmdResult<()> {
    crate::config::profiles::forget_selected_node(&group_name)
        .await
        .stringify_err()
}

static TRAY_SYNC_RUNNING: AtomicBool = AtomicBool::new(false);
static TRAY_SYNC_PENDING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
#[specta::specta]
pub async fn get_proxy_view() -> CmdResult<ProxyViewV1> {
    let runtime = Config::runtime().await;
    let latest_runtime = runtime.latest_arc();

    let mihomo = Handle::mihomo();
    let (proxies, providers) = tokio::join!(mihomo.get_proxies(), mihomo.get_proxy_providers(),);
    let proxies = proxies.stringify_err()?;

    Ok(ProxyViewBuilder::build(ProxyViewInput::from_runtime_config(
        latest_runtime.config.as_ref(),
        proxies,
        providers.ok(),
    )))
}

#[tauri::command]
#[specta::specta]
pub async fn sync_tray_proxy_selection() -> CmdResult<()> {
    if TRAY_SYNC_RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        AsyncHandler::spawn(move || async move {
            run_tray_sync_loop().await;
        });
    } else {
        TRAY_SYNC_PENDING.store(true, Ordering::Release);
    }

    Ok(())
}

async fn run_tray_sync_loop() {
    // Permanent watcher: keeps syncing until a pass finds no pending sync request.
    loop {
        match Tray::global().update_menu().await {
            Ok(_) => {
                logging!(debug, Type::Cmd, "Tray proxy selection synced successfully");
            }
            Err(e) => {
                logging!(error, Type::Cmd, "Failed to sync tray proxy selection: {e:#}");
            }
        }

        if !TRAY_SYNC_PENDING.swap(false, Ordering::AcqRel) {
            TRAY_SYNC_RUNNING.store(false, Ordering::Release);

            if TRAY_SYNC_PENDING.swap(false, Ordering::AcqRel)
                && TRAY_SYNC_RUNNING
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                continue;
            }

            break;
        }
    }
}
