// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

mod commands {
    use std::{fs, path::PathBuf, time::Duration};

    use tauri::Manager;
    use tauri_plugin_dialog::DialogExt;
    use tauri_plugin_notification::NotificationExt;
    use url::Url;

    const KEYRING_SERVICE: &str = "com.duskcue.desktop";

    #[derive(serde::Serialize)]
    pub struct AppInfo {
        name: &'static str,
        version: &'static str,
    }

    #[derive(Clone, serde::Deserialize, serde::Serialize)]
    #[serde(rename_all = "snake_case")]
    pub enum NetworkMode {
        Local,
        RemoteVpn,
        Exposed,
    }

    impl NetworkMode {
        fn default_scheme(&self) -> &'static str {
            match self {
                Self::Exposed => "https",
                Self::Local | Self::RemoteVpn => "http",
            }
        }
    }

    #[derive(Clone, serde::Deserialize, serde::Serialize)]
    pub struct ServerProfile {
        origin: String,
        #[serde(default = "default_network_mode")]
        network_mode: NetworkMode,
        display_name: Option<String>,
        last_connected_at: Option<String>,
    }

    #[derive(Default, serde::Deserialize, serde::Serialize)]
    pub struct ServerConnectionState {
        saved_servers: Vec<ServerProfile>,
        last_server: Option<ServerProfile>,
    }

    #[derive(serde::Serialize)]
    pub struct ConnectionTestResult {
        origin: String,
        status: u16,
        healthy: bool,
        body: Option<serde_json::Value>,
    }

    #[derive(serde::Deserialize)]
    pub struct SessionTokenRequest {
        server_origin: String,
        token: String,
    }

    #[derive(serde::Deserialize)]
    pub struct SessionTokenLookup {
        server_origin: String,
    }

    #[derive(serde::Deserialize)]
    pub struct NativeNotificationRequest {
        title: String,
        body: String,
        link: Option<String>,
    }

    fn default_network_mode() -> NetworkMode {
        NetworkMode::Local
    }

    #[tauri::command]
    pub fn app_info() -> AppInfo {
        AppInfo {
            name: "Duskcue",
            version: env!("CARGO_PKG_VERSION"),
        }
    }

    #[tauri::command]
    pub fn normalize_server_origin(
        input: String,
        network_mode: NetworkMode,
    ) -> Result<String, String> {
        normalize_origin(&input, &network_mode)
    }

    #[tauri::command]
    pub fn read_server_connections(app: tauri::AppHandle) -> Result<ServerConnectionState, String> {
        read_state(&app)
    }

    #[tauri::command]
    pub fn write_session_token(req: SessionTokenRequest) -> Result<(), String> {
        let origin = normalize_origin(&req.server_origin, &NetworkMode::Local)?;
        let entry = keyring::Entry::new(KEYRING_SERVICE, &origin).map_err(|err| err.to_string())?;
        entry
            .set_password(&req.token)
            .map_err(|err| err.to_string())
    }

    #[tauri::command]
    pub fn read_session_token(req: SessionTokenLookup) -> Result<Option<String>, String> {
        let origin = normalize_origin(&req.server_origin, &NetworkMode::Local)?;
        let entry = keyring::Entry::new(KEYRING_SERVICE, &origin).map_err(|err| err.to_string())?;
        match entry.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    }

    #[tauri::command]
    pub fn clear_session_token(req: SessionTokenLookup) -> Result<(), String> {
        let origin = normalize_origin(&req.server_origin, &NetworkMode::Local)?;
        let entry = keyring::Entry::new(KEYRING_SERVICE, &origin).map_err(|err| err.to_string())?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(err.to_string()),
        }
    }

    #[tauri::command]
    pub async fn pick_library_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
        tauri::async_runtime::spawn_blocking(move || {
            let selected = app.dialog().file().blocking_pick_folder();
            selected
                .map(|path| {
                    path.into_path()
                        .map(|path| path.to_string_lossy().to_string())
                        .map_err(|_| "Selected folder is not a local filesystem path.".to_string())
                })
                .transpose()
        })
        .await
        .map_err(|err| err.to_string())?
    }

    #[tauri::command]
    pub fn show_native_notification(
        app: tauri::AppHandle,
        req: NativeNotificationRequest,
    ) -> Result<(), String> {
        let title = req.title.trim();
        let body = req.body.trim();
        if title.is_empty() && body.is_empty() {
            return Ok(());
        }

        let mut notification = app
            .notification()
            .builder()
            .title(if title.is_empty() { "Duskcue" } else { title })
            .body(body)
            .auto_cancel();
        if let Some(link) = req.link.filter(|link| !link.trim().is_empty()) {
            notification = notification.extra("link", link);
        }
        notification.show().map_err(|err| err.to_string())
    }

    #[tauri::command]
    pub fn save_server_connection(
        app: tauri::AppHandle,
        mut profile: ServerProfile,
    ) -> Result<ServerConnectionState, String> {
        profile.origin = normalize_origin(&profile.origin, &profile.network_mode)?;
        let mut state = read_state(&app)?;
        state
            .saved_servers
            .retain(|server| server.origin != profile.origin);
        state.saved_servers.insert(0, profile.clone());
        state.last_server = Some(profile);
        write_state(&app, &state)?;
        Ok(state)
    }

    #[tauri::command]
    pub async fn test_server_connection(
        input: String,
        network_mode: NetworkMode,
    ) -> Result<ConnectionTestResult, String> {
        let origin = normalize_origin(&input, &network_mode)?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|err| err.to_string())?;
        let response = client
            .get(format!("{origin}/health/ready"))
            .send()
            .await
            .map_err(|err| err.to_string())?;
        let status = response.status();
        let body = response.json::<serde_json::Value>().await.ok();

        Ok(ConnectionTestResult {
            origin,
            status: status.as_u16(),
            healthy: status.is_success(),
            body,
        })
    }

    fn normalize_origin(input: &str, network_mode: &NetworkMode) -> Result<String, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("Enter a server URL.".to_string());
        }

        let candidate = if trimmed.contains("://") {
            trimmed.to_string()
        } else {
            format!("{}://{trimmed}", network_mode.default_scheme())
        };
        let parsed =
            Url::parse(&candidate).map_err(|_| "Enter a valid http(s) server URL.".to_string())?;
        let scheme = parsed.scheme();
        if scheme != "http" && scheme != "https" {
            return Err("Duskcue server URLs must use http or https.".to_string());
        }
        if matches!(network_mode, NetworkMode::Exposed) && scheme != "https" {
            return Err("Exposed servers require HTTPS.".to_string());
        }

        let host = parsed
            .host_str()
            .ok_or_else(|| "Enter a valid server host.".to_string())?;
        if parsed.port() == Some(48028) {
            return Err(
                "Use the public Duskcue port 48027, not the internal API port 48028.".to_string(),
            );
        }
        if let Some(port) = parsed.port() {
            if port != 48027 {
                return Err("Duskcue clients connect through port 48027.".to_string());
            }
        }

        let mut origin =
            Url::parse(&format!("{scheme}://duskcue.invalid")).map_err(|err| err.to_string())?;
        origin
            .set_host(Some(host))
            .map_err(|_| "Enter a valid server host.".to_string())?;
        origin
            .set_port(Some(48027))
            .map_err(|_| "Enter a valid server port.".to_string())?;
        Ok(origin.as_str().trim_end_matches('/').to_string())
    }

    fn state_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
        let dir = app.path().app_data_dir().map_err(|err| err.to_string())?;
        fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        Ok(dir.join("server-connections.json"))
    }

    fn read_state(app: &tauri::AppHandle) -> Result<ServerConnectionState, String> {
        let path = state_path(app)?;
        if !path.exists() {
            return Ok(ServerConnectionState::default());
        }
        let value = fs::read_to_string(path).map_err(|err| err.to_string())?;
        serde_json::from_str(&value).map_err(|err| err.to_string())
    }

    fn write_state(app: &tauri::AppHandle, state: &ServerConnectionState) -> Result<(), String> {
        let path = state_path(app)?;
        let value = serde_json::to_string_pretty(state).map_err(|err| err.to_string())?;
        fs::write(path, value).map_err(|err| err.to_string())
    }
}

mod shell {
    use tauri::{
        AppHandle, Emitter, Manager,
        menu::{Menu, MenuItem, PredefinedMenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };
    use tauri_plugin_deep_link::DeepLinkExt;
    use url::Url;

    #[derive(Clone, serde::Serialize)]
    struct NavigationPayload {
        route: String,
        source: &'static str,
    }

    #[derive(Clone, serde::Serialize)]
    struct ShellStatusPayload {
        status: &'static str,
    }

    pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
        setup_tray(app.handle())?;
        setup_deep_links(app.handle());
        Ok(())
    }

    pub fn open_main_window(app: &AppHandle) {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }

    pub fn handle_possible_deep_links(app: &AppHandle, args: &[String]) {
        for arg in args {
            if let Some(route) = route_from_deep_link(arg) {
                open_main_window(app);
                emit_navigation(app, route, "deep_link");
            }
        }
    }

    fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
        let open = MenuItem::with_id(app, "open", "Open Duskcue", true, None::<&str>)?;
        let server_status =
            MenuItem::with_id(app, "server_status", "Server Status", true, None::<&str>)?;
        let notifications =
            MenuItem::with_id(app, "notifications", "Notifications", true, None::<&str>)?;
        let playback =
            MenuItem::with_id(app, "playback_toggle", "Play / Pause", true, None::<&str>)?;
        let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
        let separator = PredefinedMenuItem::separator(app)?;
        let menu = Menu::with_items(
            app,
            &[
                &open,
                &server_status,
                &notifications,
                &playback,
                &separator,
                &quit,
            ],
        )?;

        let mut builder = TrayIconBuilder::with_id("main")
            .tooltip("Duskcue")
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(|app, event| match event.id().as_ref() {
                "open" => open_main_window(app),
                "server_status" => {
                    open_main_window(app);
                    emit_navigation(app, "/settings".to_string(), "tray");
                    let _ = app.emit(
                        "duskcue://server-status",
                        ShellStatusPayload { status: "open" },
                    );
                }
                "notifications" => {
                    open_main_window(app);
                    emit_navigation(app, "/settings/notifications".to_string(), "tray");
                }
                "playback_toggle" => {
                    let _ = app.emit("duskcue://playback-toggle", ());
                }
                "quit" => app.exit(0),
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    open_main_window(&tray.app_handle());
                }
            });

        if let Some(icon) = app.default_window_icon() {
            builder = builder.icon(icon.clone());
        }

        builder.build(app)?;
        Ok(())
    }

    fn setup_deep_links(app: &AppHandle) {
        let handle = app.clone();
        app.deep_link().on_open_url(move |event| {
            for url in event.urls() {
                if let Some(route) = route_from_url(&url) {
                    open_main_window(&handle);
                    emit_navigation(&handle, route, "deep_link");
                }
            }
        });
    }

    fn emit_navigation(app: &AppHandle, route: String, source: &'static str) {
        let _ = app.emit("duskcue://navigate", NavigationPayload { route, source });
    }

    fn route_from_deep_link(raw: &str) -> Option<String> {
        Url::parse(raw).ok().and_then(|url| route_from_url(&url))
    }

    fn route_from_url(url: &Url) -> Option<String> {
        if url.scheme() != "duskcue" {
            return None;
        }

        let host = url.host_str().unwrap_or_default();
        let mut segments = url
            .path_segments()
            .map(|segments| {
                segments
                    .filter(|segment| !segment.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        match host {
            "play" => segments
                .first()
                .map(|id| format!("/play/{}", sanitize_segment(id))),
            "media" => segments
                .first()
                .map(|id| format!("/media/{}", sanitize_segment(id))),
            "library" | "libraries" => segments
                .first()
                .map(|id| format!("/libraries/{}", sanitize_segment(id)))
                .or_else(|| Some("/libraries".to_string())),
            "notifications" => Some("/settings/notifications".to_string()),
            "settings" => {
                if let Some(section) = segments.pop() {
                    Some(format!("/settings/{}", sanitize_segment(section)))
                } else {
                    Some("/settings".to_string())
                }
            }
            "dashboard" | "home" => Some("/dashboard".to_string()),
            "auth" if segments.first() == Some(&"link") => Some("/auth/link".to_string()),
            _ => None,
        }
    }

    fn sanitize_segment(segment: &str) -> String {
        segment
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
            .collect()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            shell::open_main_window(app);
            shell::handle_possible_deep_links(app, &args);
        }))
        .setup(shell::setup)
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::normalize_server_origin,
            commands::read_server_connections,
            commands::save_server_connection,
            commands::test_server_connection,
            commands::write_session_token,
            commands::read_session_token,
            commands::clear_session_token,
            commands::pick_library_folder,
            commands::show_native_notification
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Duskcue desktop app");
}
