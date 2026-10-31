//! A small WebDriver client for the end-to-end smoke tests (spec section 14).
//! `tauri-driver` serves the W3C WebDriver protocol on port 4444 and starts
//! the app named in the session's `tauri:options`.
#![forbid(unsafe_code)]

use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// The key W3C WebDriver uses for element references.
const ELEMENT_KEY: &str = "element-6066-11e4-a52e-4f735466cecf";
/// How long `wait_for` keeps polling.
pub const WAIT: Duration = Duration::from_secs(60);

pub type Result<T> = std::result::Result<T, String>;

/// One WebDriver session: one running app.
pub struct Session {
    agent: ureq::Agent,
    base: String,
}

/// An element found on the page.
#[derive(Clone, Debug)]
pub struct Element(String);

impl Session {
    /// Starts the app at `app` through the driver at `driver` (for example
    /// `http://127.0.0.1:4444`).
    pub fn start(driver: &str, app: &Path) -> Result<Self> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(120)))
            .build()
            .into();
        let caps = json!({
            "capabilities": {
                "alwaysMatch": {
                    "browserName": "wry",
                    "tauri:options": { "application": app.to_string_lossy() }
                }
            }
        });
        let reply = post(&agent, &format!("{driver}/session"), &caps)?;
        let id = reply["sessionId"]
            .as_str()
            .ok_or_else(|| format!("no session id in {reply}"))?;
        Ok(Self {
            agent,
            base: format!("{driver}/session/{id}"),
        })
    }

    fn post(&self, path: &str, body: &Value) -> Result<Value> {
        post(&self.agent, &format!("{}{path}", self.base), body)
    }

    fn get(&self, path: &str) -> Result<Value> {
        let mut reply = self
            .agent
            .get(&format!("{}{path}", self.base))
            .call()
            .map_err(|e| e.to_string())?;
        value_of(reply.body_mut().read_json().map_err(|e| e.to_string())?)
    }

    /// Drags `from` with the mouse and lets go `dy` pixels below the centre
    /// of `to`. Two moves over the target, so pointer-event code sees it.
    pub fn drag(&self, from: &Element, to: &Element, dy: i64) -> Result<()> {
        let at = |el: &Element, x: i64, y: i64, ms: u64| json!({ "type": "pointerMove", "duration": ms, "origin": { ELEMENT_KEY: el.0 }, "x": x, "y": y });
        let body = json!({ "actions": [{
            "type": "pointer",
            "id": "mouse",
            "parameters": { "pointerType": "mouse" },
            "actions": [
                at(from, 0, 0, 0),
                { "type": "pointerDown", "button": 0 },
                at(to, 0, dy, 200),
                at(to, 1, dy, 100),
                { "type": "pointerUp", "button": 0 },
            ],
        }]});
        self.post("/actions", &body).map(|_| ())
    }

    /// The first element matching the XPath, if any.
    pub fn find(&self, xpath: &str) -> Result<Option<Element>> {
        let body = json!({ "using": "xpath", "value": xpath });
        match self.post("/element", &body) {
            Ok(v) => Ok(v[ELEMENT_KEY].as_str().map(|id| Element(id.to_owned()))),
            Err(e) if e.contains("no such element") => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Polls until an element matches the XPath.
    pub fn wait_for(&self, xpath: &str) -> Result<Element> {
        let start = Instant::now();
        loop {
            if let Some(el) = self.find(xpath)? {
                return Ok(el);
            }
            if start.elapsed() > WAIT {
                return Err(format!("timed out waiting for {xpath}"));
            }
            thread::sleep(Duration::from_millis(250));
        }
    }

    pub fn click(&self, el: &Element) -> Result<()> {
        self.post(&format!("/element/{}/click", el.0), &json!({}))
            .map(drop)
    }

    pub fn type_text(&self, el: &Element, text: &str) -> Result<()> {
        self.post(
            &format!("/element/{}/value", el.0),
            &json!({ "text": text }),
        )
        .map(drop)
    }

    pub fn text(&self, el: &Element) -> Result<String> {
        Ok(self
            .get(&format!("/element/{}/text", el.0))?
            .as_str()
            .unwrap_or_default()
            .to_owned())
    }

    /// Ends the session, which closes the app. Dropping the session does
    /// the same, so a failed test does not leave the app running.
    pub fn quit(mut self) -> Result<()> {
        self.end()
    }

    fn end(&mut self) -> Result<()> {
        if self.base.is_empty() {
            return Ok(());
        }
        let base = std::mem::take(&mut self.base);
        self.agent
            .delete(&base)
            .call()
            .map(drop)
            .map_err(|e| e.to_string())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.end();
    }
}

fn post(agent: &ureq::Agent, url: &str, body: &Value) -> Result<Value> {
    let mut reply = agent.post(url).send_json(body).map_err(|e| e.to_string())?;
    value_of(reply.body_mut().read_json().map_err(|e| e.to_string())?)
}

/// The `value` of a WebDriver reply, or its error as text.
pub fn value_of(reply: Value) -> Result<Value> {
    let value = reply.get("value").cloned().unwrap_or(Value::Null);
    match value.get("error").and_then(Value::as_str) {
        Some(error) => Err(format!(
            "{error}: {}",
            value["message"].as_str().unwrap_or_default()
        )),
        None => Ok(value),
    }
}

/// An XPath string literal for `text`, which may contain either quote.
pub fn xpath_literal(text: &str) -> String {
    if !text.contains('\'') {
        format!("'{text}'")
    } else if !text.contains('"') {
        format!("\"{text}\"")
    } else {
        let parts: Vec<String> = text.split('\'').map(|p| format!("'{p}'")).collect();
        format!("concat({})", parts.join(", \"'\", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_unwrap_values_and_errors() {
        assert_eq!(value_of(json!({ "value": 3 })), Ok(json!(3)));
        assert_eq!(
            value_of(json!({ "value": { "error": "no such element", "message": "gone" } })),
            Err("no such element: gone".into())
        );
        assert_eq!(value_of(json!({})), Ok(Value::Null));
    }

    #[test]
    fn xpath_literals_quote_safely() {
        assert_eq!(xpath_literal("Full name"), "'Full name'");
        assert_eq!(xpath_literal("it's"), "\"it's\"");
        assert_eq!(xpath_literal("a'b\"c"), "concat('a', \"'\", 'b\"c')");
    }
}
