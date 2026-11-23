//! The contract's HTTP path (spec #320, "The contract gains an HTTP path"): an API-key writer makes a call as an HTTP
//! request instead of running a program. Per writer a request body (`request_body`) and a reply extractor (`read_body`)
//! differ; the reply ends in the same `writer::finish` as the process path's, and a failure is one of the same kinds, so a
//! caller cannot tell which path wrote. The client is blocking (`ureq`), as the server is synchronous.
//!
//! A request's headers carry the key, and an error body may quote it: neither is ever kept in an error, which goes to the
//! log. An error says only the HTTP status.
use crate::writer::{
    FailureKind, WriterError, WriterKind, WriterReply, WriterRequest, finish, usage_of,
};
use serde_json::{Value, json};
use std::io;

/// Where the Anthropic Messages API answers.
pub const ANTHROPIC_URL: &str = "https://api.anthropic.com";

/// The API version the request is written for.
const ANTHROPIC_VERSION: &str = "2026-10-21";

/// The most a reply may hold: a summary is short, and a cap keeps a runaway reply cheap.
const MAX_TOKENS: u64 = 16_000;

/// The request body that makes `request` for writer `kind`: a Messages call, its output held to the schema when the call
/// has one (structured outputs), at the request's effort when it gives one.
pub fn request_body(kind: WriterKind, request: &WriterRequest) -> Value {
    let model = kind.model(request.model.as_deref().unwrap_or_default());
    let mut body = json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "messages": [{ "role": "user", "content": request.user }],
    });
    if let Some(system) = &request.system {
        body["system"] = json!(system);
    }
    let effort = request.effort.as_deref().filter(|e| !e.is_empty());
    if let Some(effort) = effort {
        body["output_config"]["effort"] = json!(effort);
    }
    if let Some(schema) = &request.schema {
        body["output_config"]["format"] = json!({ "type": "json_schema", "schema": schema });
    }
    body
}

/// `request` made by writer `kind` against the API at `base` with `key`, and its reply read into the contract, or why
/// not. No key is a failure of its own kind, and nothing is sent.
pub fn complete(
    kind: WriterKind,
    base: &str,
    key: Option<&str>,
    request: &WriterRequest,
) -> Result<WriterReply, WriterError> {
    let fail = |kind_of, detail: &str| Err(WriterError::new(kind_of, kind, detail));
    let Some(key) = key else {
        return fail(FailureKind::NotLoggedIn, "(no API key is set)");
    };
    let agent = ureq::AgentBuilder::new().timeout(request.timeout).build();
    let sent = agent
        .post(&format!("{}/v1/messages", base.trim_end_matches('/')))
        .set("x-api-key", key)
        .set("anthropic-version", ANTHROPIC_VERSION)
        .set("content-type", "application/json")
        .send_string(&request_body(kind, request).to_string());
    match sent {
        Ok(response) => match response.into_string() {
            Ok(body) => read_body(kind, request.schema.is_some(), &body),
            Err(e) if timed_out(&e) => fail(
                FailureKind::Timeout,
                &format!("(ran past {:?})", request.timeout),
            ),
            Err(_) => fail(FailureKind::Other, "(the reply could not be read)"),
        },
        // The error body is never read: it may quote the key.
        Err(ureq::Error::Status(status, _)) => {
            fail(status_kind(status), &format!("(HTTP {status})"))
        }
        Err(ureq::Error::Transport(transport)) => {
            let io_timeout = std::error::Error::source(&transport)
                .and_then(|e| e.downcast_ref::<io::Error>())
                .is_some_and(timed_out);
            match io_timeout {
                true => fail(
                    FailureKind::Timeout,
                    &format!("(ran past {:?})", request.timeout),
                ),
                false => fail(FailureKind::Other, "(the API could not be reached)"),
            }
        }
    }
}

/// Whether an I/O error is the request's timeout.
fn timed_out(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}

/// The failure kind of an error status: a refused key, a quota or rate limit (or no credit), a timeout, or another.
fn status_kind(status: u16) -> FailureKind {
    match status {
        401 | 403 => FailureKind::NotLoggedIn,
        402 | 429 => FailureKind::Quota,
        408 | 504 => FailureKind::Timeout,
        _ => FailureKind::Other,
    }
}

/// A successful reply's body read into the contract: its text blocks, joined, and its usage. A refusal is a failure.
fn read_body(kind: WriterKind, schema: bool, body: &str) -> Result<WriterReply, WriterError> {
    let Ok(reply) = serde_json::from_str::<Value>(body) else {
        return Err(WriterError::new(
            FailureKind::Other,
            kind,
            "(the reply is not JSON)",
        ));
    };
    if reply["stop_reason"] == "refusal" {
        return Err(WriterError::new(
            FailureKind::Other,
            kind,
            "(the model refused)",
        ));
    }
    let blocks = reply["content"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let text: String = (blocks.iter())
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect();
    finish(kind, schema, text, None, usage_of(&reply["usage"]))
}
