use std::collections::HashMap;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::{oneshot, Mutex};
use serde_json;

use pardus_challenge::resolver::{ChallengeResolver, Resolution};
use pardus_challenge::detector::ChallengeInfo;

const CHALLENGE_MONITOR_JS: &str = r#"
(function() {
    if (window.__pardusChallengeActive) return;
    window.__pardusChallengeActive = true;

    var originalUrl = window.location.href;
    var pollCount = 0;
    var maxPolls = 600;

    function getCookies() {
        return document.cookie || '';
    }

    function emit(name, data) {
        try { window.__TAURI__.event.emit(name, data); } catch(e) {}
    }

    function checkSolved() {
        pollCount++;
        if (pollCount > maxPolls) {
            emit('challenge-timeout', { url: originalUrl });
            return;
        }

        var cookies = getCookies();
        var currentUrl = window.location.href;

        var urlChanged = currentUrl !== originalUrl
            && !currentUrl.includes('challenge')
            && !currentUrl.includes('captcha');
        var hasNewCookies = cookies.length > 50;

        if (urlChanged || hasNewCookies) {
            emit('challenge-cookies', {
                url: originalUrl,
                current_url: currentUrl,
                cookies: cookies,
                url_changed: urlChanged
            });
            return;
        }

        var hasCaptchaElement = document.querySelector(
            '.g-recaptcha, .h-captcha, .cf-turnstile, ' +
            'iframe[src*="captcha"], iframe[src*="challenge"]'
        );
        if (!hasCaptchaElement && cookies.length > 0) {
            emit('challenge-cookies', {
                url: originalUrl,
                current_url: currentUrl,
                cookies: cookies,
                url_changed: false
            });
            return;
        }

        setTimeout(checkSolved, 500);
    }

    setTimeout(checkSolved, 1000);

    var lastUrl = originalUrl;
    setInterval(function() {
        if (window.location.href !== lastUrl) {
            lastUrl = window.location.href;
            var cookies = getCookies();
            if (cookies.length > 0) {
                emit('challenge-cookies', {
                    url: originalUrl,
                    current_url: lastUrl,
                    cookies: cookies,
                    url_changed: true
                });
            }
        }
    }, 1000);
})();
"#;

struct PendingChallenge {
    url: String,
    window_label: String,
    tx: oneshot::Sender<Resolution>,
}

pub struct TauriChallengeResolver {
    app_handle: AppHandle,
    pending: Arc<Mutex<HashMap<String, PendingChallenge>>>,
}

impl TauriChallengeResolver {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn open_challenge_window(
        &self,
        info: &ChallengeInfo,
        tx: oneshot::Sender<Resolution>,
    ) -> Result<(), String> {
        let sanitized: String = info.url.chars().take(40).map(|c| {
            if c.is_alphanumeric() { c } else { '-' }
        }).collect();
        let label = format!("challenge-{}", sanitized);

        let parsed_url: url::Url = info.url.parse().map_err(|e: url::ParseError| e.to_string())?;

        let kind_str = info.kinds.iter().map(|k| k.to_string()).collect::<Vec<_>>().join(", ");
        let title = format!("Solve: {}", kind_str);

        WebviewWindowBuilder::new(
            &self.app_handle,
            &label,
            WebviewUrl::External(parsed_url),
        )
        .title(&title)
        .inner_size(500.0, 680.0)
        .resizable(true)
        .initialization_script(CHALLENGE_MONITOR_JS)
        .build()
        .map_err(|e| e.to_string())?;

        let pending = PendingChallenge {
            url: info.url.clone(),
            window_label: label,
            tx,
        };
        self.pending.lock().await.insert(info.url.clone(), pending);

        Ok(())
    }

    pub async fn handle_cookies(&self, challenge_url: String, cookies: String) {
        let mut pending = self.pending.lock().await;
        if let Some(challenge) = pending.remove(&challenge_url) {
            if let Some(window) = self.app_handle.get_webview_window(&challenge.window_label) {
                let _ = window.close();
            }

            let resolution = Resolution::ModifyHeaders {
                headers: HashMap::new(),
                cookies: Some(cookies),
            };
            let _ = challenge.tx.send(resolution);

            let _ = self.app_handle.emit("challenge-solved", serde_json::json!({
                "url": challenge_url,
            }));
        }
    }

    pub async fn handle_failed(&self, challenge_url: String, reason: String) {
        let mut pending = self.pending.lock().await;
        if let Some(challenge) = pending.remove(&challenge_url) {
            if let Some(window) = self.app_handle.get_webview_window(&challenge.window_label) {
                let _ = window.close();
            }

            let resolution = Resolution::Blocked(reason.clone());
            let _ = challenge.tx.send(resolution);

            let _ = self.app_handle.emit("challenge-failed", serde_json::json!({
                "challenge_url": challenge_url,
                "reason": reason,
            }));
        }
    }
}

#[async_trait::async_trait]
impl ChallengeResolver for TauriChallengeResolver {
    async fn resolve(&self, info: ChallengeInfo) -> Resolution {
        let (tx, rx) = oneshot::channel();

        let _ = self.app_handle.emit("challenge-detected", &info);

        if let Err(e) = self.open_challenge_window(&info, tx).await {
            tracing::error!(url = %info.url, error = %e, "failed to open challenge window");
            return Resolution::Blocked(e);
        }

        match rx.await {
            Ok(resolution) => resolution,
            Err(_) => Resolution::Blocked("challenge resolver dropped".to_string()),
        }
    }
}
