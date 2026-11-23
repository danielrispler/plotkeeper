//! The contract's HTTP path against a local fake HTTP server (spec #320, "API-key writers"): the request's shape and schema,
//! a reply read into the contract, and each error status read as its failure kind. No model runs and no key is real.

use serde_json::{Value, json};
use std::{thread, time::Duration};
use studio_understanding::{FailureKind, WriterKind, WriterRequest, http_writer};

const KEY: &str = "sk-ant-planted-0000";

/// What the fake server saw of the one request it answered.
struct Seen {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Value,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        let found = self
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name));
        found.map(|(_, v)| v.as_str())
    }
}

/// A server on a free local port that answers one request with `status` and `body`, after `delay`; its base URL and
/// what it saw.
fn fake(status: u16, body: &str, delay: Duration) -> (String, thread::JoinHandle<Seen>) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    let body = body.to_string();
    let seen = thread::spawn(move || {
        let mut request = server.recv().unwrap();
        let mut text = String::new();
        request.as_reader().read_to_string(&mut text).unwrap();
        let seen = Seen {
            method: request.method().to_string(),
            url: request.url().to_string(),
            headers: (request.headers().iter())
                .map(|h| (h.field.to_string(), h.value.to_string()))
                .collect(),
            body: serde_json::from_str(&text).unwrap_or(Value::Null),
        };
        thread::sleep(delay);
        let _ = request.respond(tiny_http::Response::from_string(body).with_status_code(status));
        seen
    });
    (base, seen)
}

fn request(schema: Option<Value>) -> WriterRequest {
    WriterRequest {
        system: Some("You write summaries.".into()),
        user: "The turn.".into(),
        schema,
        model: Some("claude-sonnet-5-5".into()),
        effort: Some("low".into()),
        timeout: Duration::from_secs(10),
    }
}

fn message(text: &str) -> String {
    json!({
        "type": "message", "role": "assistant", "model": "claude-sonnet-5-5", "stop_reason": "end_turn",
        "content": [{ "type": "text", "text": text }],
        "usage": { "input_tokens": 12, "output_tokens": 3 }
    })
    .to_string()
}

#[test]
fn the_request_is_a_messages_call_with_the_key_the_model_the_effort_and_the_schema() {
    let schema = json!({ "type": "object", "properties": { "summary": { "type": "string" } },
        "required": ["summary"], "additionalProperties": false });
    let (base, seen) = fake(200, &message(r#"{"summary":"ok"}"#), Duration::ZERO);
    let reply = http_writer::complete(
        WriterKind::Anthropic,
        &base,
        Some(KEY),
        &request(Some(schema.clone())),
    );
    let seen = seen.join().unwrap();
    assert_eq!(
        (seen.method.as_str(), seen.url.as_str()),
        ("POST", "/v1/messages")
    );
    assert_eq!(seen.header("x-api-key"), Some(KEY));
    assert!(seen.header("anthropic-version").is_some());
    assert_eq!(seen.body["model"], "claude-sonnet-5-5");
    assert_eq!(seen.body["system"], "You write summaries.");
    assert_eq!(
        seen.body["messages"],
        json!([{ "role": "user", "content": "The turn." }])
    );
    assert!(seen.body["max_tokens"].as_u64().is_some_and(|n| n > 0));
    assert_eq!(seen.body["output_config"]["effort"], "low");
    assert_eq!(
        seen.body["output_config"]["format"],
        json!({ "type": "json_schema", "schema": schema })
    );

    // The reply, read into the contract as the process path reads its own.
    let reply = reply.unwrap();
    assert_eq!(reply.text, r#"{"summary":"ok"}"#);
    assert_eq!(reply.json, Some(json!({ "summary": "ok" })));
    assert_eq!(
        reply.usage.map(|u| (u.input_tokens, u.output_tokens)),
        Some((12, 3))
    );
}

#[test]
fn without_a_schema_or_an_effort_the_request_leaves_them_out_and_an_alias_is_the_first_model() {
    let (base, seen) = fake(200, &message("ok"), Duration::ZERO);
    let asked = WriterRequest {
        model: Some("haiku".into()),
        effort: Some(String::new()),
        ..request(None)
    };
    let reply = http_writer::complete(WriterKind::Anthropic, &base, Some(KEY), &asked).unwrap();
    let seen = seen.join().unwrap();
    assert_eq!(seen.body["model"], WriterKind::Anthropic.models()[0]);
    assert_eq!(seen.body.get("output_config"), None, "{}", seen.body);
    assert_eq!((reply.text.as_str(), reply.json), ("ok", None));
}

#[test]
fn each_error_status_is_read_as_its_failure_kind_and_no_body_reaches_the_error() {
    // The error bodies quote the key, as a body might: none of it may reach the error, which goes to the log.
    let body =
        json!({ "type": "error", "error": { "type": "x", "message": format!("bad key {KEY}") } })
            .to_string();
    for (status, kind) in [
        (401, FailureKind::NotLoggedIn),
        (403, FailureKind::NotLoggedIn),
        (429, FailureKind::Quota),
        (500, FailureKind::Other),
        (529, FailureKind::Other),
    ] {
        let (base, seen) = fake(status, &body, Duration::ZERO);
        let error = http_writer::complete(WriterKind::Anthropic, &base, Some(KEY), &request(None))
            .unwrap_err();
        seen.join().unwrap();
        assert_eq!(error.kind, kind, "{status}");
        for line in [
            error.to_string(),
            error.reason(),
            error.notice(),
            format!("{error:?}"),
        ] {
            assert!(
                !line.contains(KEY) && !line.contains("bad key"),
                "{status}: {line}"
            );
        }
    }
}

#[test]
fn a_reply_past_the_timeout_is_a_timeout() {
    let (base, _seen) = fake(200, &message("late"), Duration::from_secs(3));
    let quick = WriterRequest {
        timeout: Duration::from_millis(300),
        ..request(None)
    };
    let error = http_writer::complete(WriterKind::Anthropic, &base, Some(KEY), &quick).unwrap_err();
    assert_eq!(error.kind, FailureKind::Timeout, "{error}");
}

#[test]
fn no_key_set_is_not_logged_in_and_sends_nothing() {
    let error = http_writer::complete(
        WriterKind::Anthropic,
        "http://127.0.0.1:9",
        None,
        &request(None),
    )
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::NotLoggedIn);
    assert!(error.reason().contains("API key"), "{}", error.reason());
}

#[test]
fn an_empty_reply_is_empty_and_a_refusal_fails() {
    let (base, seen) = fake(200, &message(""), Duration::ZERO);
    let error =
        http_writer::complete(WriterKind::Anthropic, &base, Some(KEY), &request(None)).unwrap_err();
    seen.join().unwrap();
    assert_eq!(error.kind, FailureKind::Empty);
    let refused = json!({ "type": "message", "stop_reason": "refusal", "content": [] }).to_string();
    let (base, seen) = fake(200, &refused, Duration::ZERO);
    let error =
        http_writer::complete(WriterKind::Anthropic, &base, Some(KEY), &request(None)).unwrap_err();
    seen.join().unwrap();
    assert_eq!(error.kind, FailureKind::Other);
}
