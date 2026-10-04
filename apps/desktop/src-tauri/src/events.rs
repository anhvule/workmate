//! The engine's event stream, consumed in Rust.
//!
//! Rust subscribes rather than the webview for the reason cowork-z learned the
//! hard way: a backgrounded `WKWebView` throttles its listeners and misses
//! completion. Everything that must happen when a session finishes happens in
//! the [`Sink`] — which runs in Rust whether or not any window is watching —
//! and the webview gets a copy to render (ticket 023).
//!
//! The path is `/global/event`. On this engine line `/event` is bound to a
//! per-request lifecycle and severs after the first event.

use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use base64::Engine as _;
use serde::Serialize;
use serde_json::{json, Value};

use crate::engine::EngineAddress;

/// What the Rust side may need to react to, decided before the webview sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// The engine is asking whether something may happen; the permission
    /// surface answers (ticket 013).
    Permission,
    /// A session finished its turn. Completion is handled in Rust.
    Idle,
    Other,
}

/// One event, as delivered to the sink and emitted to the webview verbatim.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Emitted {
    /// Strictly increasing across the whole subscription, reconnects included,
    /// so a consumer can detect a gap or a reorder.
    pub seq: u64,
    /// Which connection this arrived on. A change in `epoch` means events may
    /// have been missed in between.
    pub epoch: u64,
    /// The Tauri event name: `sidecar:<type>`. Tauri rejects `.` in names, so
    /// dots in the engine's type become `_`.
    pub name: String,
    pub kind: Kind,
    pub envelope: Value,
}

/// Where events go. Runs on the subscriber thread, in order.
pub type Sink = Arc<dyn Fn(Emitted) + Send + Sync>;

/// Incremental parser for `text/event-stream`: feed lines, get whole events.
#[derive(Debug, Default)]
pub struct SseParser {
    data: Vec<String>,
}

impl SseParser {
    /// Feed one line (without its newline). Returns the event's data when a
    /// blank line completes one.
    pub fn push_line(&mut self, line: &str) -> Option<String> {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            if self.data.is_empty() {
                return None;
            }
            return Some(std::mem::take(&mut self.data).join("\n"));
        }
        if line.starts_with(':') {
            return None; // comment / keep-alive
        }
        if let Some(rest) = line.strip_prefix("data:") {
            self.data.push(rest.strip_prefix(' ').unwrap_or(rest).to_owned());
        }
        // `event:`, `id:` and `retry:` are not used by this engine.
        None
    }
}

/// Keys whose values must never reach the webview.
fn is_secret_key(k: &str) -> bool {
    let k = k.to_ascii_lowercase();
    ["password", "token", "secret", "apikey", "api_key", "authorization"]
        .iter()
        .any(|s| k.contains(s))
}

/// Replace every credential-bearing value, at any depth.
#[must_use]
pub fn scrub(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| {
                    let v = if is_secret_key(k) { json!("[redacted]") } else { scrub(v) };
                    (k.clone(), v)
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(scrub).collect()),
        other => other.clone(),
    }
}

/// The Tauri event name for an engine event type.
#[must_use]
pub fn event_name(ty: &str) -> String {
    format!("sidecar:{}", ty.replace('.', "_"))
}

fn kind_of(ty: &str) -> Kind {
    match ty {
        "permission.asked" | "permission.v2.asked" => Kind::Permission,
        "session.idle" => Kind::Idle,
        _ => Kind::Other,
    }
}

/// Parse one SSE payload into `(name, kind, scrubbed envelope)`.
///
/// `None` for anything that is not a `{ payload: { type } }` envelope: dropped
/// rather than forwarded, because the webview should only ever see the typed
/// union.
#[must_use]
pub fn classify(raw: &str) -> Option<(String, Kind, Value)> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let ty = value.get("payload")?.get("type")?.as_str()?.to_owned();
    Some((event_name(&ty), kind_of(&ty), scrub(&value)))
}

/// Reconnect timing. Exponential, capped.
#[derive(Debug, Clone, Copy)]
pub struct Backoff {
    pub initial: Duration,
    pub max: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self { initial: Duration::from_millis(500), max: Duration::from_secs(60) }
    }
}

/// A running subscription. Dropping it stops it.
pub struct Subscriber {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Subscriber {
    /// Subscribe to `address`, delivering to `sink`, reconnecting until stopped.
    #[must_use]
    pub fn start(address: &EngineAddress, sink: Sink, backoff: Backoff) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let handle = {
            let (stop, address) = (Arc::clone(&stop), address.clone());
            std::thread::spawn(move || run(&address, &sink, backoff, &stop))
        };
        Self { stop, handle: Some(handle) }
    }

    /// Ask the thread to stop. It exits at its next wake-up; a read blocked on a
    /// silent connection ends when the engine closes it, which shutdown does.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Not joined: the thread may be parked on a socket read.
        self.handle.take();
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        self.stop();
    }
}

fn sleep_unless_stopped(total: Duration, stop: &AtomicBool) {
    let tick = Duration::from_millis(25);
    let mut remaining = total;
    while !remaining.is_zero() && !stop.load(Ordering::SeqCst) {
        let nap = tick.min(remaining);
        std::thread::sleep(nap);
        remaining = remaining.saturating_sub(nap);
    }
}

fn connect(address: &EngineAddress) -> Result<impl BufRead, ureq::Error> {
    let auth = base64::engine::general_purpose::STANDARD
        .encode(format!("opencode:{}", address.password));
    // No body timeout: the stream is meant to stay open. Only the wait for the
    // response head is bounded.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_recv_response(Some(Duration::from_secs(10)))
        .build()
        .into();
    let resp = agent
        .get(format!("{}/global/event", address.base_url))
        .header("authorization", format!("Basic {auth}"))
        .header("accept", "text/event-stream")
        .call()?;
    Ok(BufReader::new(resp.into_body().into_reader()))
}

fn run(address: &EngineAddress, sink: &Sink, backoff: Backoff, stop: &AtomicBool) {
    let (mut seq, mut epoch) = (0u64, 0u64);
    let mut delay = backoff.initial;
    let mut deliver = |epoch: u64, name: String, kind: Kind, envelope: Value| {
        seq += 1;
        sink(Emitted { seq, epoch, name, kind, envelope });
    };
    while !stop.load(Ordering::SeqCst) {
        match connect(address) {
            Ok(mut reader) => {
                epoch += 1;
                delay = backoff.initial;
                if epoch > 1 {
                    // Events sent while disconnected are gone from the stream.
                    // The engine owns the transcript, so consumers resync from
                    // it; this marker says when they must.
                    deliver(epoch, event_name("stream.resumed"), Kind::Other, json!({"missed": true}));
                }
                let mut parser = SseParser::default();
                let mut line = String::new();
                while !stop.load(Ordering::SeqCst) {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    if let Some(raw) = parser.push_line(line.trim_end_matches('\n')) {
                        if let Some((name, kind, env)) = classify(&raw) {
                            deliver(epoch, name, kind, env);
                        }
                    }
                }
            }
            Err(e) => crate::logs::warn(&format!("events: connect failed: {e}")),
        }
        sleep_unless_stopped(delay, stop);
        delay = (delay * 2).min(backoff.max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;

    fn feed(lines: &[&str]) -> Vec<String> {
        let mut p = SseParser::default();
        lines.iter().filter_map(|l| p.push_line(l)).collect()
    }

    #[test]
    fn an_event_is_complete_only_at_the_blank_line() {
        assert_eq!(feed(&["data: {\"a\":1}"]), Vec::<String>::new());
        assert_eq!(feed(&["data: {\"a\":1}", ""]), ["{\"a\":1}"]);
    }

    #[test]
    fn multi_line_data_is_joined_and_comments_are_skipped() {
        assert_eq!(feed(&[": keep-alive", "data: a", "data: b", "", ""]), ["a\nb"]);
    }

    #[test]
    fn crlf_and_missing_space_after_the_colon_are_tolerated() {
        assert_eq!(feed(&["data:x\r", "\r"]), ["x"]);
    }

    #[test]
    fn dotted_types_become_tauri_safe_names() {
        assert_eq!(event_name("message.part.updated"), "sidecar:message_part_updated");
        assert_eq!(event_name("permission.v2.asked"), "sidecar:permission_v2_asked");
    }

    #[test]
    fn permission_requests_and_completion_are_recognised_for_rust() {
        let perm = classify(r#"{"payload":{"type":"permission.asked","properties":{}}}"#).unwrap();
        assert_eq!((perm.0.as_str(), perm.1), ("sidecar:permission_asked", Kind::Permission));
        let idle = classify(r#"{"payload":{"type":"session.idle","properties":{}}}"#).unwrap();
        assert_eq!(idle.1, Kind::Idle);
        let other = classify(r#"{"payload":{"type":"file.edited","properties":{}}}"#).unwrap();
        assert_eq!(other.1, Kind::Other);
    }

    #[test]
    fn credentials_are_scrubbed_at_any_depth_before_the_webview_sees_them() {
        let (_, _, env) = classify(
            r#"{"payload":{"type":"x.y","properties":{"info":{"apiKey":"sk-1","nested":[{"Authorization":"Bearer z"}]},"ok":"keep"}}}"#,
        )
        .unwrap();
        let text = env.to_string();
        assert!(!text.contains("sk-1") && !text.contains("Bearer z"), "{text}");
        assert!(text.contains("keep"));
    }

    #[test]
    fn anything_that_is_not_an_envelope_is_dropped() {
        assert!(classify("not json").is_none());
        assert!(classify(r#"{"type":"naked"}"#).is_none());
        assert!(classify(r#"{"payload":{"no":"type"}}"#).is_none());
    }

    /// A one-purpose SSE server: the first connection delivers two events and
    /// closes; the second delivers one and stays open.
    fn serve() -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let heads = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&heads);
        std::thread::spawn(move || {
            for (n, conn) in listener.incoming().enumerate() {
                let mut conn = conn.unwrap();
                let mut buf = [0u8; 2048];
                let got = conn.read(&mut buf).unwrap();
                seen.lock().unwrap().push(String::from_utf8_lossy(&buf[..got]).into_owned());
                let ev = |ty: &str| format!("data: {{\"payload\":{{\"type\":\"{ty}\"}}}}\n\n");
                let body = if n == 0 { ev("a.one") + &ev("a.two") } else { ev("a.three") };
                let _ = conn.write_all(
                    format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{body}").as_bytes(),
                );
                if n == 0 {
                    continue; // drop => close
                }
                std::thread::sleep(Duration::from_secs(5));
            }
        });
        (url, heads)
    }

    #[test]
    fn it_reconnects_in_order_marks_the_gap_and_authenticates_every_connection() {
        let (base_url, heads) = serve();
        let got = Arc::new(Mutex::new(Vec::<Emitted>::new()));
        let sink: Sink = {
            let got = Arc::clone(&got);
            Arc::new(move |e| got.lock().unwrap().push(e))
        };
        let addr = EngineAddress { base_url, password: "pw".into() };
        let _sub = Subscriber::start(
            &addr,
            sink,
            Backoff { initial: Duration::from_millis(50), max: Duration::from_millis(200) },
        );
        for _ in 0..100 {
            if got.lock().unwrap().len() >= 4 {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let got = got.lock().unwrap();
        let names: Vec<_> = got.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            ["sidecar:a_one", "sidecar:a_two", "sidecar:stream_resumed", "sidecar:a_three"]
        );
        assert_eq!(got.iter().map(|e| e.seq).collect::<Vec<_>>(), [1, 2, 3, 4]);
        assert_eq!(got.iter().map(|e| e.epoch).collect::<Vec<_>>(), [1, 1, 2, 2]);
        let heads = heads.lock().unwrap();
        assert!(heads[0].starts_with("GET /global/event "), "{}", heads[0]);
        let want = base64::engine::general_purpose::STANDARD.encode("opencode:pw");
        assert!(heads.iter().all(|h| h.to_lowercase().contains(&format!("authorization: basic {want}").to_lowercase())));
    }
}
