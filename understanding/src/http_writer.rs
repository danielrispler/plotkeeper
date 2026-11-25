//! The contract's HTTP path (spec #320, "The contract gains an HTTP path"): an API-key writer makes a call as an HTTP
//! request instead of running a program. Per writer a request body (`request_body`, `openai_body`) and a reply extractor
//! (`read_body`, `read_openai`) differ; the reply ends in the same `writer::finish` as the process path's, and a failure is
//! one of the same kinds, so a caller cannot tell which path wrote. The client is blocking (`ureq`), as the server is
//! synchronous.
//!
//! A request's headers carry the key, and an error body may quote it: neither is ever kept in an error, which goes to the
//! log. An error says only the HTTP status.
use crate::writer::{
    FailureKind, Usage, WriterError, WriterKind, WriterReply, WriterRequest, finish, usage_of,
};
use serde_json::{Value, json};
use std::io;

/// Where the Anthropic Messages API answers.
pub const ANTHROPIC_URL: &str = "https://api.anthropic.com";

/// OpenAI's own chat API: the OpenAI-compatible writer's base URL when none is set (#367).
pub const OPENAI_URL: &str = "https://api.openai.com/v1";

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
    if let Some(effort) = effort(request) {
        body["output_config"]["effort"] = json!(effort);
    }
    if let Some(schema) = &request.schema {
        body["output_config"]["format"] = json!({ "type": "json_schema", "schema": schema });
    }
    body
}

/// The request body that makes `request` on an OpenAI-compatible chat API: one chat completion, the system prompt its first
/// message, the effort as `reasoning_effort` when the request gives one, and, with `schema`, the output held to the
/// request's schema (strict structured outputs). No token cap: OpenAI's and other endpoints' names for it differ.
fn openai_body(request: &WriterRequest, schema: bool) -> Value {
    let model = WriterKind::OpenAI.model(request.model.as_deref().unwrap_or_default());
    let mut messages = Vec::new();
    if let Some(system) = &request.system {
        messages.push(json!({ "role": "system", "content": system }));
    }
    messages.push(json!({ "role": "user", "content": request.user }));
    let mut body = json!({ "model": model, "messages": messages });
    if let Some(effort) = effort(request) {
        body["reasoning_effort"] = json!(effort);
    }
    if let (true, Some(schema)) = (schema, &request.schema) {
        body["response_format"] = json!({
            "type": "json_schema",
            "json_schema": { "name": "reply", "schema": schema, "strict": true },
        });
    }
    body
}

/// The request's effort, when it gives one.
fn effort(request: &WriterRequest) -> Option<&str> {
    request.effort.as_deref().filter(|e| !e.is_empty())
}

/// `request` made by writer `kind` against the API at `base` with `key`, and its reply read into the contract, or why
/// not. The Anthropic API needs a key: none is a failure of its own kind, and nothing is sent. An OpenAI-compatible API
/// is called without one when none is set, as a local Ollama needs none.
pub fn complete(
    kind: WriterKind,
    base: &str,
    key: Option<&str>,
    request: &WriterRequest,
) -> Result<WriterReply, WriterError> {
    let base = base.trim_end_matches('/');
    if kind == WriterKind::OpenAI {
        return complete_openai(base, key, request);
    }
    let Some(key) = key else {
        return Err(WriterError::new(
            FailureKind::NotLoggedIn,
            kind,
            "(no API key is set)",
        ));
    };
    let headers = [("x-api-key", key), ("anthropic-version", ANTHROPIC_VERSION)];
    let url = format!("{base}/v1/messages");
    match post(kind, &url, &headers, &request_body(kind, request), request) {
        Ok(body) => read_body(kind, request.schema.is_some(), &body),
        Err(failed) => Err(failed.into_error(kind)),
    }
}

/// `complete` on an OpenAI-compatible chat API. An endpoint that refuses the schema (400) is asked once more without it,
/// and the reply's object is then read from its text and checked, as for a writer that takes no schema.
fn complete_openai(
    base: &str,
    key: Option<&str>,
    request: &WriterRequest,
) -> Result<WriterReply, WriterError> {
    let kind = WriterKind::OpenAI;
    let bearer = key.map(|key| format!("Bearer {key}"));
    let headers: Vec<(&str, &str)> = bearer
        .iter()
        .map(|b| ("authorization", b.as_str()))
        .collect();
    let url = format!("{base}/chat/completions");
    let schema = request.schema.is_some();
    let mut sent = post(kind, &url, &headers, &openai_body(request, true), request);
    if schema && matches!(sent, Err(Failed::Status(400))) {
        sent = post(kind, &url, &headers, &openai_body(request, false), request);
    }
    match sent {
        Ok(body) => read_openai(schema, &body),
        Err(failed) => Err(failed.into_error(kind)),
    }
}

/// Why a request got no reply: an error status, or a failure already typed.
enum Failed {
    Status(u16),
    Error(WriterError),
}

impl Failed {
    fn into_error(self, kind: WriterKind) -> WriterError {
        match self {
            Failed::Status(status) => {
                WriterError::new(status_kind(status), kind, format!("(HTTP {status})"))
            }
            Failed::Error(error) => error,
        }
    }
}

/// `body` posted to `url` with `headers` within `request`'s timeout, and the reply's body, or why not.
fn post(
    kind: WriterKind,
    url: &str,
    headers: &[(&str, &str)],
    body: &Value,
    request: &WriterRequest,
) -> Result<String, Failed> {
    let fail = |kind_of, detail: &str| Failed::Error(WriterError::new(kind_of, kind, detail));
    let late = || {
        fail(
            FailureKind::Timeout,
            &format!("(ran past {:?})", request.timeout),
        )
    };
    let agent = ureq::AgentBuilder::new().timeout(request.timeout).build();
    let mut call = agent.post(url).set("content-type", "application/json");
    for (name, value) in headers {
        call = call.set(name, value);
    }
    match call.send_string(&body.to_string()) {
        Ok(response) => response.into_string().map_err(|e| {
            if timed_out(&e) {
                late()
            } else {
                fail(FailureKind::Other, "(the reply could not be read)")
            }
        }),
        // The error body is never read: it may quote the key.
        Err(ureq::Error::Status(status, _)) => Err(Failed::Status(status)),
        Err(ureq::Error::Transport(transport)) => {
            let io_timeout = std::error::Error::source(&transport)
                .and_then(|e| e.downcast_ref::<io::Error>())
                .is_some_and(timed_out);
            Err(if io_timeout {
                late()
            } else {
                fail(FailureKind::Other, "(the API could not be reached)")
            })
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

/// A body that is not JSON, or a refusal: a failure of kind other.
fn other(kind: WriterKind, detail: &str) -> Result<WriterReply, WriterError> {
    Err(WriterError::new(FailureKind::Other, kind, detail))
}

/// A successful reply's body read into the contract: its text blocks, joined, and its usage. A refusal is a failure.
fn read_body(kind: WriterKind, schema: bool, body: &str) -> Result<WriterReply, WriterError> {
    let Ok(reply) = serde_json::from_str::<Value>(body) else {
        return other(kind, "(the reply is not JSON)");
    };
    if reply["stop_reason"] == "refusal" {
        return other(kind, "(the model refused)");
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

/// A chat completion read into the contract: its first choice's text, and its usage. A refusal, or a reply its endpoint's
/// filter stopped, is a failure.
fn read_openai(schema: bool, body: &str) -> Result<WriterReply, WriterError> {
    let kind = WriterKind::OpenAI;
    let Ok(reply) = serde_json::from_str::<Value>(body) else {
        return other(kind, "(the reply is not JSON)");
    };
    let choice = &reply["choices"][0];
    let message = &choice["message"];
    if message["refusal"].is_string() || choice["finish_reason"] == "content_filter" {
        return other(kind, "(the model refused)");
    }
    let text = message["content"].as_str().unwrap_or_default().to_string();
    let usage = &reply["usage"];
    let usage = usage["prompt_tokens"].as_u64().map(|input_tokens| Usage {
        input_tokens,
        output_tokens: usage["completion_tokens"].as_u64().unwrap_or_default(),
    });
    finish(kind, schema, text, None, usage)
}
