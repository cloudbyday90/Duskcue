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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::normalize_server_origin,
            commands::read_server_connections,
            commands::save_server_connection,
            commands::test_server_connection,
            commands::write_session_token,
            commands::read_session_token,
            commands::clear_session_token
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Duskcue desktop app");
}
