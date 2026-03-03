//! Ollama TTS Adapter - Tauri 应用入口
//! 
//! 提供 Tauri 命令和应用初始化逻辑。

use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use std::sync::{Arc, Mutex};
use tracing_subscriber::prelude::*;

// 模块声明
pub mod api;
pub mod models;
pub mod server;
pub mod services;

use server::AppState;
use services::config_store::{ConfigStore, get_default_config_path};
use services::sse_broadcaster::SseBroadcaster;
use services::log_layer::BroadcasterLayer;

/// 字幕窗口状态
struct SubtitleState {
    click_through: bool,
}

const DEFAULT_SUBTITLE_WINDOW_X: i32 = 100;
const DEFAULT_SUBTITLE_WINDOW_Y: i32 = 100;
const DEFAULT_SUBTITLE_WINDOW_WIDTH: i32 = 800;
const DEFAULT_SUBTITLE_WINDOW_HEIGHT: i32 = 200;
const MIN_SUBTITLE_WINDOW_WIDTH: i32 = 400;
const MIN_SUBTITLE_WINDOW_HEIGHT: i32 = 120;

fn sanitize_subtitle_window_geometry(x: i32, y: i32, width: i32, height: i32) -> (i32, i32, i32, i32) {
    let safe_x = if x >= 0 { x } else { DEFAULT_SUBTITLE_WINDOW_X };
    let safe_y = if y >= 0 { y } else { DEFAULT_SUBTITLE_WINDOW_Y };
    let safe_width = if width >= MIN_SUBTITLE_WINDOW_WIDTH {
        width
    } else {
        DEFAULT_SUBTITLE_WINDOW_WIDTH
    };
    let safe_height = if height >= MIN_SUBTITLE_WINDOW_HEIGHT {
        height
    } else {
        DEFAULT_SUBTITLE_WINDOW_HEIGHT
    };

    (safe_x, safe_y, safe_width, safe_height)
}

/// 获取后端 URL
#[tauri::command]
fn get_backend_url() -> String {
    "http://localhost:8765".to_string()
}

/// 启动后端服务（代理服务器）
#[tauri::command]
async fn start_backend(app: tauri::AppHandle) -> Result<(), String> {
    log::info!("Starting backend (proxy server) via Tauri command...");
    let state = app.state::<AppState>();
    state.proxy_manager.start().await
}

/// 停止后端服务（代理服务器）
#[tauri::command]
async fn stop_backend(app: tauri::AppHandle) -> Result<(), String> {
    log::info!("Stopping backend (proxy server) via Tauri command...");
    let state = app.state::<AppState>();
    state.proxy_manager.stop().await
}

/// 创建独立字幕窗口
#[tauri::command]
async fn create_subtitle_window(app: tauri::AppHandle) -> Result<(), String> {
    let label = "subtitle";
    
    // 如果窗口已存在，直接显示
    if let Some(window) = app.get_webview_window(label) {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let app_state = app.state::<AppState>();
    let config = app_state.config_store.get();
    let (window_x, window_y, window_width, window_height) = sanitize_subtitle_window_geometry(
        config.subtitle_window_x,
        config.subtitle_window_y,
        config.subtitle_window_width,
        config.subtitle_window_height,
    );

    // 创建新的字幕窗口
    let _window = WebviewWindowBuilder::new(
        &app,
        label,
        WebviewUrl::App("/#/overlay".into())
    )
    .title("字幕")
    .inner_size(window_width as f64, window_height as f64)
    .position(window_x as f64, window_y as f64)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .resizable(true)
    .skip_taskbar(true)
    .accept_first_mouse(true)
    .build()
    .map_err(|e| format!("Failed to create subtitle window: {}", e))?;

    log::info!(
        "Subtitle window created with geometry: x={}, y={}, width={}, height={}",
        window_x,
        window_y,
        window_width,
        window_height
    );
    Ok(())
}

/// 关闭字幕窗口
#[tauri::command]
async fn close_subtitle_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("subtitle") {
        window.close().map_err(|e| e.to_string())?;
        log::info!("Subtitle window closed");
    }
    Ok(())
}

/// 显示字幕窗口
#[tauri::command]
async fn show_subtitle_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("subtitle") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    } else {
        create_subtitle_window(app).await?;
    }
    Ok(())
}

/// 隐藏字幕窗口
#[tauri::command]
async fn hide_subtitle_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("subtitle") {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 设置字幕窗口鼠标穿透（桌面穿透）
#[tauri::command]
async fn set_subtitle_click_through(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("subtitle") {
        window.set_ignore_cursor_events(enabled)
            .map_err(|e| format!("Failed to set click through: {}", e))?;
        
        // 保存状态
        let state = app.state::<Mutex<SubtitleState>>();
        let mut subtitle_state = state.lock().unwrap();
        subtitle_state.click_through = enabled;
        
        log::info!("Subtitle click through: {}", enabled);
    }
    Ok(())
}

/// 获取字幕窗口穿透状态
#[tauri::command]
fn get_subtitle_click_through(app: tauri::AppHandle) -> bool {
    let state = app.state::<Mutex<SubtitleState>>();
    let subtitle_state = state.lock().unwrap();
    subtitle_state.click_through
}

/// 设置字幕窗口位置
#[tauri::command]
async fn set_subtitle_position(app: tauri::AppHandle, x: f64, y: f64) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("subtitle") {
        window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
            x: x as i32,
            y: y as i32,
        })).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 设置字幕窗口大小
#[tauri::command]
async fn set_subtitle_size(app: tauri::AppHandle, width: f64, height: f64) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("subtitle") {
        window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
            width: width as u32,
            height: height as u32,
        })).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 设置字幕窗口透明度
#[tauri::command]
async fn set_subtitle_opacity(app: tauri::AppHandle, opacity: f64) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("subtitle") {
        window.emit("set-opacity", opacity).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 将文本复制到系统剪贴板并根据粘贴模式模拟按键粘贴到当前焦点窗口
/// paste_mode: "direct" = Ctrl+V, "ahk" = Ctrl+Shift+V (AutoHotkey)
#[tauri::command]
async fn clipboard_paste(text: String, paste_mode: Option<String>) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        use arboard::Clipboard;
        use enigo::{Enigo, Keyboard, Key, Settings, Direction};

        let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard error: {}", e))?;
        clipboard.set_text(&text).map_err(|e| format!("Set clipboard error: {}", e))?;

        std::thread::sleep(std::time::Duration::from_millis(100));

        let mut enigo = Enigo::new(&Settings::default()).map_err(|e| format!("Enigo error: {}", e))?;

        let mode = paste_mode.unwrap_or_else(|| "direct".to_string());
        if mode == "ahk" {
            enigo.key(Key::Control, Direction::Press).map_err(|e| format!("Key error: {}", e))?;
            enigo.key(Key::Shift, Direction::Press).map_err(|e| format!("Key error: {}", e))?;
            enigo.key(Key::Other(0x56), Direction::Click).map_err(|e| format!("Key error: {}", e))?;
            enigo.key(Key::Shift, Direction::Release).map_err(|e| format!("Key error: {}", e))?;
            enigo.key(Key::Control, Direction::Release).map_err(|e| format!("Key error: {}", e))?;
        } else {
            enigo.key(Key::Control, Direction::Press).map_err(|e| format!("Key error: {}", e))?;
            enigo.key(Key::Other(0x56), Direction::Click).map_err(|e| format!("Key error: {}", e))?;
            enigo.key(Key::Control, Direction::Release).map_err(|e| format!("Key error: {}", e))?;
        }

        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build());

    // MCP 插件（仅在 debug 模式下启用）
    #[cfg(debug_assertions)]
    {
        builder = builder.plugin(tauri_plugin_mcp_gui::init_with_config(
            tauri_plugin_mcp_gui::PluginConfig::new("OllamaTTS".to_string())
                .start_socket_server(true)
                .tcp("127.0.0.1".to_string(), 9999)
        ));
    }

    builder
        .manage(Mutex::new(SubtitleState { click_through: false }))
        .invoke_handler(tauri::generate_handler![
            get_backend_url,
            start_backend,
            stop_backend,
            create_subtitle_window,
            close_subtitle_window,
            show_subtitle_window,
            hide_subtitle_window,
            set_subtitle_click_through,
            get_subtitle_click_through,
            set_subtitle_position,
            set_subtitle_size,
            set_subtitle_opacity,
            clipboard_paste
        ])
        .setup(|app| {
            // 初始化 SSE 广播器（需要先创建，用于 tracing layer）
            let broadcaster = Arc::new(SseBroadcaster::new(100));

            // 初始化 tracing subscriber，将日志转发到 SSE Broadcaster
            let broadcaster_layer = BroadcasterLayer::new(broadcaster.clone());
            let fmt_layer = tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_level(true);
            
            let subscriber = tracing_subscriber::registry()
                .with(tracing_subscriber::filter::LevelFilter::INFO)
                .with(fmt_layer)
                .with(broadcaster_layer);
            
            let _ = tracing::subscriber::set_global_default(subscriber);

            // 初始化日志插件（用于 log crate 兼容）
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // 初始化配置存储
            let config_path = get_default_config_path();
            tracing::info!("Loading config from: {:?}", config_path);
            let config_store = ConfigStore::new(config_path);

            // 启动 HTTP 服务器（只启动主 API，代理由用户控制）
            tracing::info!("Starting HTTP server...");
            let app_state = server::spawn_server_with_broadcaster(config_store, broadcaster);
            
            // 将 AppState 注册到 Tauri 状态管理
            app.manage(app_state);

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { .. } = event {
                let label = window.label();
                if label == "main" {
                    log::info!("Main window close requested, exiting application...");
                    
                    let app = window.app_handle();
                    
                    if let Some(subtitle_win) = app.get_webview_window("subtitle") {
                        let _ = subtitle_win.close();
                    }
                    
                    app.exit(0);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
