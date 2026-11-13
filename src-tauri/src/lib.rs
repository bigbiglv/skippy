mod commands;
mod llm;
mod models;
mod state;
mod storage;
mod worker;

use llm::llama_supervisor::LlamaSupervisor;
use state::{AppState, RuntimeStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use storage::chat_repo::{get_all_settings, resolve_local_storage_dir};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use worker::supervisor::WorkerSupervisor;

pub fn show_or_recreate_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    if let Some(window_config) = app.config().app.windows.first().cloned() {
        match WebviewWindowBuilder::from_config(app, &window_config) {
            Ok(builder) => {
                if let Ok(window) = builder.build() {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            Err(err) => {
                eprintln!("Failed to rebuild main window from config: {err}");
            }
        }
    }
}

pub fn toggle_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_focused().unwrap_or(false) {
            let _ = window.destroy();
            return;
        }
    }
    show_or_recreate_main_window(app);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_or_recreate_main_window(app);
        }))
        .setup(|app| {
            let data_dir = app
                .path()
                .app_local_data_dir()
                .expect("failed to resolve app local data dir");
            let db_path = data_dir.join("skippy.db");
            let conn = storage::init_db(&db_path).expect("failed to initialize sqlite db");

            let worker = WorkerSupervisor::new(18765);
            let python_env = worker.detect_env();
            let storage_dir = resolve_local_storage_dir(&conn, &data_dir);
            let llama_ready = LlamaSupervisor::find_binary(&storage_dir).is_some();
            let llama = LlamaSupervisor::new(18766);

            app.manage(AppState {
                db: Mutex::new(conn),
                data_dir,
                status: Mutex::new(RuntimeStatus::with_env(
                    python_env,
                    llama_ready,
                    storage_dir.to_string_lossy().to_string(),
                )),
                worker: Mutex::new(worker),
                llama: Mutex::new(llama),
                chat_cancel: Arc::new(AtomicBool::new(false)),
                download_cancel: Arc::new(AtomicBool::new(false)),
                should_exit: AtomicBool::new(false),
            });

            // 后台空闲超时回收循环（Idle TTL）：自动终止闲置的 Worker 与 llama-server 释放内存与显存
            let reaper_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(15)).await;
                    let state = reaper_handle.state::<AppState>();

                    let ttl_secs: u64 = state
                        .db
                        .lock()
                        .ok()
                        .and_then(|db| get_all_settings(&db).ok())
                        .and_then(|s| s.get("idle_unload_secs").cloned())
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(180);

                    if ttl_secs == 0 {
                        continue;
                    }

                    let worker_stopped = if let Ok(mut w) = state.worker.lock() {
                        if w.is_running()
                            && w.idle_elapsed_secs().is_some_and(|elapsed| elapsed >= ttl_secs)
                        {
                            w.stop()
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    let llama_stopped = if let Ok(mut l) = state.llama.lock() {
                        if l.is_running()
                            && l.idle_elapsed_secs().is_some_and(|elapsed| elapsed >= ttl_secs)
                        {
                            l.stop()
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if worker_stopped || llama_stopped {
                        if let Ok(mut st) = state.status.lock() {
                            if worker_stopped {
                                st.python_worker_running = false;
                            }
                            if llama_stopped {
                                st.llama_server_running = false;
                                st.llama_loaded_model = None;
                            }
                            if !st.python_worker_running && !st.llama_server_running {
                                st.gpu_vram_active_mb = 0;
                            }
                        }
                    }
                }
            });

            let open_item = MenuItem::with_id(app, "open", "打开 Skippy", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出 Skippy", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&open_item, &quit_item])?;

            let mut tray_builder = TrayIconBuilder::new()
                .menu(&tray_menu)
                .tooltip("Skippy - 待机常驻")
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_or_recreate_main_window(app),
                    "quit" => {
                        let state = app.state::<AppState>();
                        if let Ok(mut w) = state.worker.lock() {
                            w.stop();
                        }
                        if let Ok(mut l) = state.llama.lock() {
                            l.stop();
                        }
                        state.should_exit.store(true, Ordering::Relaxed);
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_or_recreate_main_window(tray.app_handle());
                    }
                });

            if let Some(icon) = app.default_window_icon().cloned() {
                tray_builder = tray_builder.icon(icon);
            }
            tray_builder.build(app)?;

            let shortcut = Shortcut::new(Some(Modifiers::ALT), Code::Space);
            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |app, sc, event| {
                        if sc == &shortcut && event.state() == ShortcutState::Pressed {
                            toggle_main_window(app);
                        }
                    })
                    .build(),
            )?;

            if let Err(err) = app.global_shortcut().register(shortcut) {
                eprintln!("Warning: could not register Alt+Space shortcut: {err}");
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_snapshot,
            commands::create_chat_session,
            commands::delete_chat_session,
            commands::get_session_messages,
            commands::send_text_message,
            commands::stop_generation,
            commands::select_active_model,
            commands::update_app_setting,
            commands::install_llama_runtime,
            commands::download_gguf_model,
            commands::cancel_download,
            commands::import_local_gguf,
            commands::delete_local_model,
            commands::start_llama_server,
            commands::stop_llama_server,
            commands::pick_local_gguf_file,
            commands::pick_storage_directory,
            commands::start_python_worker,
            commands::stop_python_worker,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() {
                let state = app_handle.state::<AppState>();
                if !state.should_exit.load(Ordering::Relaxed) {
                    api.prevent_exit();
                }
            }
        }
    });
}
