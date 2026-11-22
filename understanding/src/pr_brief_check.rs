//! The claim check and the one retry of a PR brief (spec #334, ticket #338).
//!
//! After the code checks, a second sandboxed call reads the same facts and the written brief and says, per item,
//! whether the facts support it. Code-check flags and claim flags then go into one retry request that rewrites
//! only the flagged items; an item still flagged after it carries "not confirmed from the facts". Both calls use
//! the writer's model. Items are steps (with their check's question, options and yes/no text, and their risks),
//! the "also" lines and the decisions; the flags of a chapter or of the brief as a whole are not items a retry
//! can rewrite and are left as `check_brief` set them.

use crate::pr_brief::{
    Also, Brief, BriefRecord, BriefWriter, CallUsage, CheckContext, Decision, Facts, Step,
    check_brief, prompt,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// What an item still flagged after the retry carries.
pub const UNCONFIRMED: &str = "not confirmed from the facts";
const CLAIM_PREFIX: &str = "claim check: ";

const CHECK_INSTRUCTIONS: &str = r#"You check a written PR brief against the facts it was written from. You have no tools: everything you may rely on is in FACTS below.

For every item of the brief, decide whether FACTS support each claim it makes, at the strength it states it:
- a step: its heading and body, its law and covered_by, its list lines, every risk (part, trigger, check), and, when it holds a check, the check's question, its options and its yes and no text;
- an "also" line;
- a decision.
A claim is supported when FACTS (the PR text, spec, map, diff, tests or checks) say it or plainly show it. A claim that only sounds plausible, a test said to cover something the facts do not show it covering, a cause or an intent no section states, a number that is not there: not supported.

Reply with ONE JSON object and nothing else:

{ "items": [ { "id": "the item's id exactly as given", "supported": true | false, "phrase": "the unsupported phrase, copied from the item (empty when supported)", "why": "one short line: what the facts say instead, or that they say nothing" } ] }

Give one entry per item.
"#;

const RETRY_INSTRUCTIONS: &str = r#"RETRY. The brief above was written from FACTS and then checked. The items listed under FLAGGED each failed a check, with the reason. Rewrite ONLY those items so that every flag is fixed, keeping what was right in them; say only what FACTS support, and leave out what you cannot support. Do not touch any other item.

A step whose "k" is "Your turn" must hold a check that the rules above allow; when you cannot give one, change its "k" to another kind and omit the check.

Reply with ONE JSON object and nothing else, with one entry per flagged item, using the item's id exactly as given:

{ "items": [
  { "id": "step 3", "step": a full step, as in the rules above },
  { "id": "decision 1", "text": "the decision, one sentence" },
  { "id": "also 1", "line": "one line, what changed in the part" }
] }
"#;

#[derive(Clone, Copy)]
enum Item {
    Step(usize),
    Also(usize),
    Decision(usize),
}

fn items(brief: &Brief) -> Vec<(String, Item)> {
    let mut out: Vec<(String, Item)> = (0..brief.steps().count())
        .map(|i| (format!("step {}", i + 1), Item::Step(i)))
        .collect();
    out.extend((0..brief.also.len()).map(|i| (format!("also {}", i + 1), Item::Also(i))));
    out.extend(
        (0..brief.decisions.len()).map(|i| (format!("decision {}", i + 1), Item::Decision(i))),
    );
    out
}

fn flags_mut<'a>(brief: &'a mut Brief, item: Item) -> &'a mut Vec<String> {
    match item {
        Item::Step(i) => &mut brief.steps_mut().nth(i).expect("a step").flags,
        Item::Also(i) => &mut brief.also[i].flags,
        Item::Decision(i) => &mut brief.decisions[i].flags,
    }
}

fn flags_of(brief: &Brief, item: Item) -> &[String] {
    match item {
        Item::Step(i) => &brief.steps().nth(i).expect("a step").flags,
        Item::Also(i) => &brief.also[i].flags,
        Item::Decision(i) => &brief.decisions[i].flags,
    }
}

/// The item as the checker and the retry see it: its content, never its flags.
fn item_json(brief: &Brief, item: Item) -> Value {
    match item {
        Item::Step(i) => {
            let mut s = brief.steps().nth(i).expect("a step").clone();
            s.flags.clear();
            serde_json::to_value(s).unwrap_or_default()
        }
        Item::Also(i) => json!({ "part": brief.also[i].part, "line": brief.also[i].line }),
        Item::Decision(i) => json!({ "text": brief.decisions[i].text }),
    }
}

/// The claim check's request: the rules, the facts, and every item with its id.
pub fn claim_prompt(facts: &Facts, brief: &Brief) -> String {
    let list: Vec<Value> = items(brief)
        .into_iter()
        .map(|(id, item)| json!({ "id": id, "item": item_json(brief, item) }))
        .collect();
    format!(
        "{CHECK_INSTRUCTIONS}\nFACTS\n\n{}\n\nBRIEF ITEMS\n\n{}\n",
        facts.text,
        serde_json::to_string_pretty(&list).unwrap_or_default()
    )
}

/// The `{ ... }` object in a reply's text (or in the whole `claude` envelope).
fn object_in(text: &str) -> Result<Value, String> {
    let text = crate::pr_brief::read_envelope(text)?.text;
    let (a, b) = text
        .find('{')
        .zip(text.rfind('}'))
        .ok_or("the reply holds no JSON object")?;
    serde_json::from_str(&text[a..=b]).map_err(|e| format!("the reply's JSON does not read: {e}"))
}

/// A claim the checker found unsupported.
#[derive(Clone, Debug, PartialEq)]
pub struct ClaimFlag {
    pub id: String,
    pub text: String,
}

/// The unsupported claims in the checker's reply. A reply that is not `{"items": [...]}` with a `supported`
/// verdict on every entry is malformed, and then nothing at all is taken from it. An id the brief does not
/// have is ignored.
pub fn read_claim_reply(text: &str, brief: &Brief) -> Result<Vec<ClaimFlag>, String> {
    let v = object_in(text)?;
    let entries = v
        .get("items")
        .and_then(Value::as_array)
        .ok_or("the reply has no \"items\" list")?;
    let known: BTreeSet<String> = items(brief).into_iter().map(|(id, _)| id).collect();
    let mut flags = Vec::new();
    for e in entries {
        let supported = e
            .get("supported")
            .and_then(Value::as_bool)
            .ok_or("an entry of the reply has no true or false \"supported\"")?;
        let id = e.get("id").and_then(Value::as_str).unwrap_or_default();
        if supported || !known.contains(id) {
            continue;
        }
        let text_of = |k: &str| e.get(k).and_then(Value::as_str).unwrap_or_default().trim();
        let (phrase, why) = (text_of("phrase"), text_of("why"));
        let text = match (phrase.is_empty(), why.is_empty()) {
            (false, false) => format!("{CLAIM_PREFIX}\"{phrase}\": {why}"),
            (false, true) => format!("{CLAIM_PREFIX}\"{phrase}\""),
            (true, false) => format!("{CLAIM_PREFIX}{why}"),
            (true, true) => format!("{CLAIM_PREFIX}not supported"),
        };
        flags.push(ClaimFlag {
            id: id.to_string(),
            text,
        });
    }
    Ok(flags)
}

fn add_claim_flags(brief: &mut Brief, claims: &[ClaimFlag], skip: &BTreeSet<String>) {
    let by_id: BTreeMap<String, Item> = items(brief).into_iter().collect();
    for c in claims.iter().filter(|c| !skip.contains(&c.id)) {
        if let Some(item) = by_id.get(&c.id) {
            flags_mut(brief, *item).push(c.text.clone());
        }
    }
}

/// The retry's request: the brief's whole prompt, the brief as written, and each flagged item with its flags.
/// `None` when no item is flagged.
pub fn retry_prompt(facts: &Facts, brief: &Brief) -> Option<String> {
    let flagged: Vec<Value> = items(brief)
        .into_iter()
        .filter(|(_, item)| !flags_of(brief, *item).is_empty())
        .map(|(id, item)| {
            json!({ "id": id, "flags": flags_of(brief, item), "item": item_json(brief, item) })
        })
        .collect();
    if flagged.is_empty() {
        return None;
    }
    let mut whole = brief.clone();
    for (_, item) in items(&whole) {
        flags_mut(&mut whole, item).clear();
    }
    Some(format!(
        "{}\n\nTHE BRIEF AS WRITTEN\n\n{}\n\n{RETRY_INSTRUCTIONS}\nFLAGGED\n\n{}\n",
        prompt(facts),
        serde_json::to_string_pretty(&whole).unwrap_or_default(),
        serde_json::to_string_pretty(&flagged).unwrap_or_default(),
    ))
}

/// Puts the retry's rewrites into the items that were flagged and returns their ids. A rewrite of an item that
/// was not flagged, or of one the brief does not have, or one that does not read, is ignored.
fn apply_retry(brief: &mut Brief, text: &str) -> Result<BTreeSet<String>, String> {
    let v = object_in(text)?;
    let entries = v
        .get("items")
        .and_then(Value::as_array)
        .ok_or("the reply has no \"items\" list")?;
    let by_id: BTreeMap<String, Item> = items(brief).into_iter().collect();
    let mut done = BTreeSet::new();
    for e in entries {
        let id = e.get("id").and_then(Value::as_str).unwrap_or_default();
        let Some(&item) = by_id.get(id) else { continue };
        if flags_of(brief, item).is_empty() || done.contains(id) {
            continue;
        }
        let text_of = |k: &str| e.get(k).and_then(Value::as_str).map(str::to_string);
        let ok = match item {
            Item::Step(i) => match e
                .get("step")
                .map(|s| serde_json::from_value::<Step>(s.clone()))
            {
                Some(Ok(mut step)) => {
                    step.flags.clear();
                    *brief.steps_mut().nth(i).expect("a step") = step;
                    true
                }
                _ => false,
            },
            Item::Also(i) => text_of("line").is_some_and(|line| {
                let Also { part, .. } = brief.also[i].clone();
                brief.also[i] = Also {
                    part,
                    line,
                    flags: vec![],
                };
                true
            }),
            Item::Decision(i) => text_of("text").is_some_and(|text| {
                let Decision { id, .. } = brief.decisions[i].clone();
                brief.decisions[i] = Decision {
                    id,
                    text,
                    flags: vec![],
                };
                true
            }),
        };
        if ok {
            done.insert(id.to_string());
        }
    }
    Ok(done)
}

fn mark_unconfirmed(brief: &mut Brief) {
    for (_, item) in items(brief) {
        if !flags_of(brief, item).is_empty() {
            flags_mut(brief, item).push(UNCONFIRMED.into());
        }
    }
}

fn with_role(mut usage: CallUsage, role: &str) -> CallUsage {
    usage.role = role.into();
    usage
}

fn labelled(claims: &[ClaimFlag], skip: &BTreeSet<String>) -> Vec<String> {
    claims
        .iter()
        .filter(|c| !skip.contains(&c.id))
        .map(|c| format!("{}: {}", c.id, c.text))
        .collect()
}

/// Runs the claim check on `brief` (already code-checked), then the one retry, and fills the record's claim
/// fields and calls. A call that fails or a reply that does not read is recorded and logged and leaves the
/// brief as it was.
pub fn claim_check_and_retry(
    writer: &(impl BriefWriter + ?Sized),
    model: &str,
    facts: &Facts,
    ctx: &CheckContext,
    brief: &mut Brief,
    record: &mut BriefRecord,
    log: &mut impl FnMut(String),
) {
    let mut claims = Vec::new();
    match writer.write(model, &claim_prompt(facts, brief)) {
        Err(e) => record.claim_error = Some(e),
        Ok(reply) => {
            record.calls.push(with_role(reply.usage, "claim check"));
            match read_claim_reply(&reply.text, brief) {
                Ok(found) => claims = found,
                Err(e) => record.claim_error = Some(e),
            }
        }
    }
    if let Some(e) = &record.claim_error {
        log(format!("claim check of #{}: {e}", record.pr));
    }
    let none = BTreeSet::new();
    record.claim_flags_before = labelled(&claims, &none);
    add_claim_flags(brief, &claims, &none);

    let Some(request) = retry_prompt(facts, brief) else {
        return;
    };
    let mut rewritten = BTreeSet::new();
    match writer.write(model, &request) {
        Err(e) => record.retry_error = Some(e),
        Ok(reply) => {
            record.calls.push(with_role(reply.usage, "retry"));
            match apply_retry(brief, &reply.text) {
                Ok(done) => rewritten = done,
                Err(e) => record.retry_error = Some(e),
            }
        }
    }
    if let Some(e) = &record.retry_error {
        log(format!("retry of the brief of #{}: {e}", record.pr));
    }
    // Rewritten items are checked again by code; the claim flags of the rest stand, since the claim check
    // does not run twice.
    check_brief(brief, ctx);
    add_claim_flags(brief, &claims, &rewritten);
    record.claim_flags_after = labelled(&claims, &rewritten);
    mark_unconfirmed(brief);
}
