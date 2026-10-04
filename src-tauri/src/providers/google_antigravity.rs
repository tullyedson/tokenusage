//! Reads the existing Antigravity desktop service. It never starts a model,
//! opens its databases, or starts/stops an Antigravity process.
use reqwest::header::HeaderValue;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    process::Stdio,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command};
use zeroize::Zeroizing;

const MAX_BODY: usize = 256 * 1024;
const UNAVAILABLE: &str = "Open the Antigravity desktop app and sign in to your Google AI account, then refresh. This connection follows the account signed in to Antigravity.";

#[derive(Deserialize)]
struct Discovery {
    servers: Vec<Server>,
}
#[derive(Deserialize)]
struct Server {
    csrf: String,
    ports: Vec<u16>,
}
impl Drop for Server {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.csrf.zeroize();
    }
}

async fn cancelled(flag: &AtomicBool) {
    while !flag.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn discover() -> Result<Discovery, String> {
    let system = std::env::var_os("SystemRoot").ok_or(UNAVAILABLE)?;
    let mut command = Command::new(
        std::path::PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe"),
    );
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            include_str!("scripts/antigravity-discovery.ps1"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn().map_err(|_| UNAVAILABLE)?;
    let mut output = child.stdout.take().ok_or(UNAVAILABLE)?.take(32 * 1024 + 1);
    let mut bytes = Zeroizing::new(Vec::new());
    let result = async {
        output
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| UNAVAILABLE)?;
        if bytes.len() > 32 * 1024 {
            return Err(UNAVAILABLE.to_string());
        }
        let status = child.wait().await.map_err(|_| UNAVAILABLE)?;
        if !status.success() {
            return Err(UNAVAILABLE.to_string());
        }
        let value: Discovery = serde_json::from_slice(&bytes).map_err(|_| UNAVAILABLE)?;
        if value.servers.is_empty() || value.servers.len() > 4 {
            return Err(UNAVAILABLE.to_string());
        }
        Ok(value)
    }
    .await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

async fn summary(client: &reqwest::Client, url: &str, csrf: &str) -> Result<Value, String> {
    let mut header = HeaderValue::from_str(csrf).map_err(|_| UNAVAILABLE)?;
    header.set_sensitive(true);
    let mut response = client.post(url)
        .header("X-Codeium-Csrf-Token", header).header("Connect-Protocol-Version", "1")
        .json(&json!({"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en","ideVersion":"unknown"}}))
        .send().await.map_err(|_| UNAVAILABLE)?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|n| n > MAX_BODY as u64)
    {
        return Err(UNAVAILABLE.into());
    }
    let mut bytes = Zeroizing::new(Vec::new());
    while let Some(chunk) = response.chunk().await.map_err(|_| UNAVAILABLE)? {
        if bytes.len() + chunk.len() > MAX_BODY {
            return Err(UNAVAILABLE.into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| UNAVAILABLE)?;
    if !value["response"]["groups"].is_array() {
        return Err(UNAVAILABLE.into());
    }
    Ok(value)
}

pub async fn read(flag: &AtomicBool) -> Result<Value, String> {
    if flag.load(Ordering::Acquire) {
        return Err("Refresh cancelled.".into());
    }
    let task = async {
        let discovery = tokio::time::timeout(Duration::from_secs(10), discover())
            .await
            .map_err(|_| UNAVAILABLE)??;
        // Antigravity uses its own self-signed local certificate. This private
        // client can reach only freshly discovered, same-user Antigravity
        // loopback sockets below, with no proxy, redirects or configurable URL.
        let client = reqwest::Client::builder()
            .https_only(true)
            .no_proxy()
            .danger_accept_invalid_certs(true)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(4))
            .build()
            .map_err(|_| UNAVAILABLE)?;
        for server in discovery.servers {
            if server.csrf.is_empty() || server.csrf.len() > 512 || server.ports.len() > 4 {
                continue;
            }
            for port in &server.ports {
                if *port == 0 {
                    continue;
                }
                let url = format!("https://127.0.0.1:{port}/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary");
                if let Ok(value) = summary(&client, &url, &server.csrf).await {
                    return Ok(value);
                }
            }
        }
        Err(UNAVAILABLE.to_string())
    };
    tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(30), task) => result.unwrap_or_else(|_| Err("Antigravity usage refresh timed out. Keep Antigravity open and try again.".into())),
        _ = cancelled(flag) => Err("Refresh cancelled.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_does_not_inspect_processes() {
        assert_eq!(
            read(&AtomicBool::new(true)).await.unwrap_err(),
            "Refresh cancelled."
        );
    }
    #[tokio::test]
    #[ignore = "reads the signed-in local Antigravity account, without printing data"]
    async fn signed_in_antigravity_returns_quota() {
        use crate::{model::ProviderConfig, providers::IUsageProvider};
        let value = read(&AtomicBool::new(false)).await.unwrap();
        let mut config = ProviderConfig::default();
        config
            .fields
            .insert("connection".into(), "antigravity".into());
        let snapshot = super::super::google::Google.parse(value, &config).unwrap();
        assert!(!snapshot.meters.is_empty());
        assert!(snapshot
            .meters
            .iter()
            .all(|meter| meter.percent_left.is_some()));
    }

    #[tokio::test]
    async fn local_protocol_only_requests_quota_and_redacts_errors() {
        use axum::{
            extract::Json,
            http::{HeaderMap, StatusCode},
            routing::post,
            Router,
        };
        let app = Router::new()
            .route(
                "/usage",
                post(|headers: HeaderMap, Json(body): Json<Value>| async move {
                    assert_eq!(headers["X-Codeium-Csrf-Token"], "fictional-local-token");
                    assert_eq!(headers["Connect-Protocol-Version"], "1");
                    assert_eq!(body.as_object().unwrap().len(), 1);
                    assert!(body["metadata"].is_object());
                    Json(json!({"response":{"groups":[]}}))
                }),
            )
            .route(
                "/failure",
                post(|| async { (StatusCode::UNAUTHORIZED, "private-error-with-token") }),
            )
            .route("/large", post(|| async { "x".repeat(MAX_BODY + 1) }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let result = summary(
            &client,
            &format!("http://{address}/usage"),
            "fictional-local-token",
        )
        .await;
        let error = summary(
            &client,
            &format!("http://{address}/failure"),
            "fictional-local-token",
        )
        .await
        .unwrap_err();
        let large = summary(
            &client,
            &format!("http://{address}/large"),
            "fictional-local-token",
        )
        .await;
        server.abort();
        assert!(result.is_ok());
        assert!(!error.contains("private-error") && !error.contains("fictional-local-token"));
        assert!(large.is_err());
    }
}
