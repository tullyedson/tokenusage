//! Bounded GET-only transport. No redirects, account mutations or response-body errors.
use reqwest::{
    header::{HeaderValue, AUTHORIZATION},
    StatusCode,
};
use serde_json::Value;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use zeroize::Zeroizing;

const MAX_BODY: usize = 128 * 1024;

pub async fn get_usage(
    url: &'static str,
    key: &str,
    cancelled: &AtomicBool,
    forbidden: &'static str,
) -> Result<Value, String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("Refresh cancelled.".into());
    }
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "Could not initialize the secure usage connection.")?;
    let authorization = Zeroizing::new(format!("Bearer {key}"));
    let mut header = HeaderValue::from_str(&authorization)
        .map_err(|_| "The saved key has an invalid format.")?;
    header.set_sensitive(true);
    let request = async {
        let mut response = client
            .get(url)
            .header(AUTHORIZATION, header)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(network_error)?;
        check_status(response.status(), forbidden)?;
        if response
            .content_length()
            .is_some_and(|size| size > MAX_BODY as u64)
        {
            return Err("The usage response is too large.".into());
        }
        let mut body = Zeroizing::new(Vec::new());
        while let Some(chunk) = response.chunk().await.map_err(network_error)? {
            if body.len() + chunk.len() > MAX_BODY {
                return Err("The usage response is too large.".into());
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body)
            .map_err(|_| "The provider returned an unsupported usage response.".into())
    };
    tokio::select! {
        result = request => result,
        _ = async { while !cancelled.load(Ordering::Acquire) { tokio::time::sleep(Duration::from_millis(100)).await; } } => Err("Refresh cancelled.".into()),
    }
}

fn network_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "The usage request timed out. Try again."
    } else {
        "Could not reach the provider securely. Check your connection and try again."
    }
    .into()
}
fn check_status(status: StatusCode, forbidden: &str) -> Result<(), String> {
    if status.is_success() {
        return Ok(());
    }
    Err(match status {
        StatusCode::UNAUTHORIZED => "The key was rejected. Save a valid key in Settings.".into(),
        StatusCode::FORBIDDEN => forbidden.into(),
        StatusCode::TOO_MANY_REQUESTS => {
            "The provider is limiting usage checks. Wait before refreshing.".into()
        }
        _ if status.is_redirection() => {
            "The usage endpoint moved. Update the app before reconnecting.".into()
        }
        _ => format!(
            "The provider could not return usage (HTTP {}).",
            status.as_u16()
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancelled_requests_do_not_connect_and_http_is_rejected() {
        assert_eq!(
            get_usage(
                "https://example.invalid",
                "fictional",
                &AtomicBool::new(true),
                "Forbidden"
            )
            .await
            .unwrap_err(),
            "Refresh cancelled."
        );
        let error = get_usage(
            "http://127.0.0.1:1",
            "fictional-sensitive-value",
            &AtomicBool::new(false),
            "Forbidden",
        )
        .await
        .unwrap_err();
        assert!(!error.contains("fictional"));
        assert!(error.contains("securely"));
    }
    #[test]
    fn auth_and_redirect_failures_are_actionable_without_echoing_response_bodies() {
        assert!(check_status(StatusCode::UNAUTHORIZED, "")
            .unwrap_err()
            .contains("key was rejected"));
        assert_eq!(
            check_status(StatusCode::FORBIDDEN, "Management key required").unwrap_err(),
            "Management key required"
        );
        assert!(check_status(StatusCode::FOUND, "")
            .unwrap_err()
            .contains("endpoint moved"));
        assert!(check_status(StatusCode::TOO_MANY_REQUESTS, "")
            .unwrap_err()
            .contains("Wait"));
    }
}
