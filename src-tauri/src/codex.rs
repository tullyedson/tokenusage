use serde_json::{json, Value};
use std::{
    path::PathBuf,
    process::Stdio,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

fn executable(configured: &str) -> Result<PathBuf, String> {
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        if path.is_absolute()
            && path.is_file()
            && path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
        {
            return Ok(path);
        }
        return Err("Select the absolute path to codex.exe.".into());
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let root = PathBuf::from(local).join("OpenAI/Codex/bin");
        let mut candidates: Vec<_> = std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path().join("codex.exe"))
            .filter(|p| p.is_file())
            .collect();
        candidates.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
        if let Some(path) = candidates.pop() {
            return Ok(path);
        }
    }
    for root in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let path = root.join("codex.exe");
        if path.is_file() {
            return Ok(path);
        }
    }
    Err("Codex was not found. Choose website sign-in, or install and sign in to Codex and set its executable path.".into())
}

pub async fn read_limits(configured: &str, cancelled: &AtomicBool) -> Result<Value, String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("Refresh cancelled.".into());
    }
    let mut command = Command::new(executable(configured)?);
    command
        .arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command
        .spawn()
        .map_err(|_| "Could not start the Codex usage reader.")?;
    let result = tokio::time::timeout(Duration::from_secs(35), async {
        let mut input = child.stdin.take().ok_or("Could not open the Codex reader input.")?;
        let output = child.stdout.take().ok_or("Could not read Codex output.")?;
        let init = json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"ai_usage_tray","title":"AI Usage","version":"0.1.0"},"capabilities":{"experimentalApi":false}}});
        input.write_all(format!("{init}\n").as_bytes()).await.map_err(|_| "Could not initialize Codex.")?;
        let mut lines = BufReader::new(output).lines();
        let mut initialized = false;
        for _ in 0..500 {
            let next = tokio::select! {
                next = lines.next_line() => next,
                _ = wait_for_cancellation(cancelled) => return Err("Refresh cancelled.".into()),
            };
            let line = next.map_err(|_| "Could not read the Codex usage response.")?.ok_or("Codex closed before returning usage. Check your Codex sign-in.")?;
            if line.len() > 1024 * 1024 { return Err("Codex returned an oversized response.".into()); }
            let Ok(value) = serde_json::from_str::<Value>(&line) else { continue; };
            if value["id"] == 1 && !initialized {
                if value["error"].is_object() { return Err("Codex could not initialize. Update Codex and try again.".into()); }
                input.write_all(b"{\"method\":\"initialized\",\"params\":{}}\n{\"id\":2,\"method\":\"account/rateLimits/read\"}\n").await.map_err(|_| "Could not request Codex limits.")?;
                initialized = true;
            }
            if value["id"] == 2 {
                if value["error"].is_object() { return Err("Codex could not read subscription limits. Sign in to Codex with your ChatGPT account, then refresh.".into()); }
                return Ok(value["result"].clone());
            }
        }
        Err("Codex did not return its usage limits.".into())
    }).await.unwrap_or_else(|_| Err("Codex usage refresh timed out. Try again.".into()));
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

async fn wait_for_cancellation(cancelled: &AtomicBool) {
    while !cancelled.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancelled_read_does_not_launch_a_process() {
        assert_eq!(
            read_limits("not-an-executable", &AtomicBool::new(true))
                .await
                .unwrap_err(),
            "Refresh cancelled."
        );
    }
}
