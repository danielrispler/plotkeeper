//! Reading a reply: the writer, its envelope and its usage.

use super::{Brief, cut_to};
use crate::ClaudeWriter;
use serde::{Deserialize, Serialize};
/// What a call cost.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CallUsage {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cost_usd: f64,
    /// The call's wall time, from asking the writer to its reply (danielrispler/plotkeeper#334, port decision Q5).
    #[serde(default)]
    pub duration_ms: u64,
    /// Of `input_tokens`, those read from the cache and those written to it (#450, the measurement of caching).
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: u64,
}

/// A writer's reply: the text, and what the call cost when it says.
#[derive(Clone, Debug, Default)]
pub struct BriefReply {
    pub text: String,
    pub usage: CallUsage,
}

/// Which call of a write runs (#450): the writer's and the retry's write the brief and run at the brief effort
/// (`prBriefs.effort`); the claim check always runs at `claude`'s own, so a lower effort never weakens the checker.
/// `Plain` is any other call through `BriefWriter::write` (pre-flight, a law draft), also at `claude`'s own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    Write,
    Check,
    Plain,
}

/// What writes a brief: `ClaudeWriter` in the app, a canned reply in a test.
pub trait BriefWriter {
    /// The reply to `prompt` at `model`, or one line for the log when the call failed.
    fn write(&self, model: &str, prompt: &str) -> Result<BriefReply, String>;

    /// The reply to `prompt` with `facts`, a write's shared facts block, as the system text every call of the write shares
    /// (#450), for a call of kind `kind`. By default the two go as one prompt, the facts first.
    fn write_with_facts(
        &self,
        model: &str,
        kind: CallKind,
        facts: &str,
        prompt: &str,
    ) -> Result<BriefReply, String> {
        let _ = kind;
        self.write(model, &format!("{facts}{prompt}"))
    }
}

/// `writer`'s reply to `prompt` at `model`, with the call's wall time (`duration_ms`) set on its usage.
pub fn timed_write(
    writer: &(impl BriefWriter + ?Sized),
    model: &str,
    prompt: &str,
) -> Result<BriefReply, String> {
    let started = std::time::Instant::now();
    let mut reply = writer.write(model, prompt)?;
    reply.usage.duration_ms = started.elapsed().as_millis() as u64;
    Ok(reply)
}

/// `timed_write` with `facts` as the shared system text (`BriefWriter::write_with_facts`).
pub fn timed_write_facts(
    writer: &(impl BriefWriter + ?Sized),
    model: &str,
    kind: CallKind,
    facts: &str,
    prompt: &str,
) -> Result<BriefReply, String> {
    let started = std::time::Instant::now();
    let mut reply = writer.write_with_facts(model, kind, facts, prompt)?;
    reply.usage.duration_ms = started.elapsed().as_millis() as u64;
    Ok(reply)
}

/// Reads `claude -p --output-format json`'s output: the `result` text and the usage. A reply that is not that
/// envelope is taken as the text itself.
pub fn read_envelope(output: &str) -> Result<BriefReply, String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(output) else {
        return Ok(BriefReply {
            text: output.to_string(),
            usage: CallUsage::default(),
        });
    };
    let Some(result) = v.get("result").and_then(|r| r.as_str()) else {
        return Ok(BriefReply {
            text: output.to_string(),
            usage: CallUsage::default(),
        });
    };
    if v["is_error"].as_bool() == Some(true) {
        return Err(format!(
            "`claude -p` reported an error: {}",
            cut_to(result, 300)
        ));
    }
    let usage = CallUsage {
        role: String::new(),
        model: v["modelUsage"]
            .as_object()
            .map(|m| m.keys().cloned().collect::<Vec<_>>().join(", "))
            .unwrap_or_default(),
        input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0)
            + v["usage"]["cache_creation_input_tokens"]
                .as_u64()
                .unwrap_or(0)
            + v["usage"]["cache_read_input_tokens"].as_u64().unwrap_or(0),
        output_tokens: v["usage"]["output_tokens"].as_u64().unwrap_or(0),
        cost_usd: v["total_cost_usd"].as_f64().unwrap_or(0.0),
        duration_ms: 0,
        cache_read_tokens: v["usage"]["cache_read_input_tokens"].as_u64().unwrap_or(0),
        cache_write_tokens: v["usage"]["cache_creation_input_tokens"]
            .as_u64()
            .unwrap_or(0),
    };
    Ok(BriefReply {
        text: result.to_string(),
        usage,
    })
}

impl BriefWriter for ClaudeWriter {
    fn write(&self, model: &str, prompt: &str) -> Result<BriefReply, String> {
        let out = self.brief_stdout(model, CallKind::Plain, None, prompt)?;
        read_envelope(&out)
    }

    fn write_with_facts(
        &self,
        model: &str,
        kind: CallKind,
        facts: &str,
        prompt: &str,
    ) -> Result<BriefReply, String> {
        let out = self.brief_stdout(model, kind, Some(facts), prompt)?;
        read_envelope(&out)
    }
}

/// The brief in a writer's reply text (or in the whole `claude` envelope): the first `{` to the last `}`.
pub fn parse_reply(text: &str) -> Result<Brief, String> {
    object_in(text)
}

/// The `{ ... }` object in a reply's text (or in the whole `claude` envelope), read as a `T`, or why not: the PR brief's
/// and its claim check's one reading of a reply.
pub(crate) fn object_in<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, String> {
    let text = read_envelope(text)?.text;
    crate::read_object(&text).map_err(|e| match e {
        None => "the reply holds no JSON object".into(),
        Some(e) => format!("the reply's JSON does not read: {e}"),
    })
}
