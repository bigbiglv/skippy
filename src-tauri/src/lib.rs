mod commands;
mod models;
mod state;
mod storage;

use state::{AppState, RuntimeStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

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
            // 直接销毁 WebView 释放渲染内存，保持 Rust 托盘常驻
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

            app.manage(AppState {
                db: Mutex::new(conn),
                data_dir,
                status: Mutex::new(RuntimeStatus::default()),
                should_exit: AtomicBool::new(false),
            });

            // 注册系统托盘（GUI 关闭销毁后由此常驻唤醒）
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

            // 注册全局快捷键 Alt + Space
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
            commands::select_active_model,
            commands::update_app_setting,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() {
                let state = app_handle.state::<AppState>();
                if !state.should_exit.load(Ordering::Relaxed) {
                    // 最后一个窗口被关闭（WebView 已销毁），阻止进程退出以维持低内存托盘常驻
                    api.prevent_exit();
                }
            }
        }
    });
}
