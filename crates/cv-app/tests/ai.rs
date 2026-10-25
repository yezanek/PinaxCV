//! AI settings, keys and consent against a real SQLite store.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cv_api::{AiConfig, AiProvider, ErrorCode, KeyStorage};
use cv_app::{App, ExportKind, FileDialog, SecretError, SecretStore, UlidIds};
use cv_store::{SqliteStore, SystemClock};

struct NoDialog;

impl FileDialog for NoDialog {
    fn choose_save_path(&self, _: &str, _: ExportKind) -> Option<PathBuf> {
        None
    }

    fn choose_open_path(&self, _: &[&str]) -> Option<PathBuf> {
        None
    }
}

/// A keychain in memory, shared so tests can look inside.
#[derive(Clone, Default)]
struct FakeKeychain(Arc<Mutex<HashMap<String, String>>>);

impl SecretStore for FakeKeychain {
    fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
        Ok(self.0.lock().expect("lock").get(account).cloned())
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError> {
        self.0
            .lock()
            .expect("lock")
            .insert(account.into(), secret.into());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        self.0.lock().expect("lock").remove(account);
        Ok(())
    }
}

fn app(keychain: Option<FakeKeychain>) -> App {
    let store = SqliteStore::open_in_memory(Box::new(SystemClock)).expect("store");
    let app = App::new(
        Box::new(store),
        Box::new(NoDialog),
        Box::new(UlidIds),
        "preview://localhost",
    );
    match keychain {
        Some(k) => app.with_secrets(Box::new(k)),
        None => app,
    }
}

fn config(provider: AiProvider, model: &str, key: Option<&str>) -> AiConfig {
    AiConfig {
        provider,
        model: model.into(),
        base_url: None,
        api_key: key.map(Into::into),
        send_name: false,
        send_location: true,
    }
}

#[test]
fn nothing_is_configured_at_first() {
    let app = app(None);
    let s = app.ai_status().expect("status");
    assert_eq!(s.provider, AiProvider::None);
    assert!(!s.configured && !s.consent_ok && !s.keychain_available);
    assert!(!s.send_name && s.send_location);
    assert_eq!(
        app.llm().map(|_| ()).expect_err("no llm").code,
        ErrorCode::AiNotConfigured
    );
}

#[test]
fn keys_go_to_the_keychain_and_never_to_the_database() {
    let keychain = FakeKeychain::default();
    let app = app(Some(keychain.clone()));
    let s = app
        .ai_configure(config(
            AiProvider::Anthropic,
            " claude-x ",
            Some("sk-secret"),
        ))
        .expect("configure");
    assert!(s.configured && s.keychain_available);
    assert_eq!(s.key, KeyStorage::Keychain);
    assert_eq!(s.model, "claude-x");
    assert_eq!(s.host.as_deref(), Some("api.anthropic.com"));
    assert_eq!(
        keychain
            .0
            .lock()
            .expect("lock")
            .get("anthropic")
            .map(String::as_str),
        Some("sk-secret")
    );
    let json = serde_json::to_string(&s).expect("json");
    assert!(!json.contains("sk-secret"));

    // Saving again without a key keeps the one in the keychain.
    let s = app
        .ai_configure(config(AiProvider::Anthropic, "claude-y", None))
        .expect("again");
    assert_eq!(s.key, KeyStorage::Keychain);

    let s = app.ai_forget_key().expect("forget");
    assert_eq!(s.key, KeyStorage::None);
    assert!(!s.configured);
    assert!(keychain.0.lock().expect("lock").is_empty());
}

#[test]
fn without_a_keychain_keys_live_in_memory_for_the_session() {
    let app = app(None);
    let s = app
        .ai_configure(config(AiProvider::Gemini, "gemini-x", Some("g-key")))
        .expect("configure");
    assert_eq!(s.key, KeyStorage::Memory);
    assert!(s.configured && !s.keychain_available);
}

#[test]
fn settings_are_checked() {
    let app = app(None);
    let err = app
        .ai_configure(config(AiProvider::Openai, " ", Some("k")))
        .expect_err("no model");
    assert_eq!(err.code, ErrorCode::Validation);
    let mut local = config(AiProvider::OpenaiCompatible, "llama3", None);
    local.base_url = Some("localhost:11434".into());
    assert_eq!(
        app.ai_configure(local.clone()).expect_err("no scheme").code,
        ErrorCode::Validation
    );
    local.base_url = Some("http://localhost:11434/v1/".into());
    let s = app.ai_configure(local).expect("local");
    assert!(s.configured, "a local server needs no key");
    assert_eq!(s.base_url.as_deref(), Some("http://localhost:11434/v1"));
    assert_eq!(s.host.as_deref(), Some("localhost:11434"));
}

#[test]
fn consent_covers_one_provider_and_host() {
    let app = app(Some(FakeKeychain::default()));
    assert_eq!(
        app.ai_consent().expect_err("no provider").code,
        ErrorCode::Validation
    );
    app.ai_configure(config(AiProvider::Openai, "gpt-x", Some("k")))
        .expect("configure");
    assert_eq!(
        app.llm().map(|_| ()).expect_err("consent first").code,
        ErrorCode::AiConsentRequired
    );
    assert!(app.ai_consent().expect("consent").consent_ok);
    assert!(app.llm().is_ok());

    // Another model at the same provider keeps the consent.
    let s = app
        .ai_configure(config(AiProvider::Openai, "gpt-y", None))
        .expect("model");
    assert!(s.consent_ok);
    // Another provider asks again.
    let s = app
        .ai_configure(config(AiProvider::Gemini, "gemini-x", Some("g")))
        .expect("provider");
    assert!(!s.consent_ok);
}

/// Serves one HTTP request on localhost with `body` as a 200 JSON reply.
fn serve_once(body: &'static str) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}/v1", listener.local_addr().expect("addr"));
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        // Read the whole request first: closing a socket with unread
        // bytes resets the connection on Windows before the reply lands.
        let mut req = Vec::new();
        let mut buf = [0u8; 8192];
        while let Ok(n) = stream.read(&mut buf) {
            if n == 0 {
                break;
            }
            req.extend_from_slice(&buf[..n]);
            let text = String::from_utf8_lossy(&req);
            if let Some(end) = text.find("\r\n\r\n") {
                let len = text[..end]
                    .lines()
                    .find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        k.eq_ignore_ascii_case("content-length")
                            .then(|| v.trim().parse::<usize>().ok())?
                    })
                    .unwrap_or(0);
                if req.len() >= end + 4 + len {
                    break;
                }
            }
        }
        let reply = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(reply.as_bytes());
        let _ = stream.flush();
        let _ = stream.shutdown(std::net::Shutdown::Write);
    });
    url
}

#[tokio::test]
async fn the_connection_test_needs_a_json_answer() {
    let app = app(None);
    assert_eq!(
        app.ai_test_connection().await.expect_err("unset").code,
        ErrorCode::AiNotConfigured
    );
    let url = serve_once(r#"{"choices":[{"message":{"content":"{\"ok\": true}"}}]}"#);
    let mut local = config(AiProvider::OpenaiCompatible, "llama3", None);
    local.base_url = Some(url);
    app.ai_configure(local).expect("configure");
    app.ai_test_connection().await.expect("connected");
}
