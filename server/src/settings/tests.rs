use super::{
    brief_backfill_in, edit_text, plain_json, pr_briefs_effort_in, pr_briefs_model_in,
    repository_in, run_settings_in, save_explanations_in, save_writer_in, summary_writer_in,
    write_in,
};
use serde_json::json;
use studio_understanding::WriterKind;

#[test]
fn a_runs_settings_are_its_projects_own_and_nothing_is_borrowed() {
    let text = r#"{ // Zed's format
        "backgroundRuns": { "harness": " claude ", "model": "sonnet", },
        "checks": { "/w": ["make"], "/w/app/": ["cargo test", " npm run check ", "", 3] },
        "setup": { "/w/app": " npm ci " },
    }"#;
    let app = run_settings_in(text, "/w/app");
    assert_eq!(app.harness, "claude");
    assert_eq!(app.model.as_deref(), Some("sonnet"));
    assert_eq!(app.checks, ["cargo test", "npm run check"]);
    assert_eq!(app.setup.as_deref(), Some("npm ci"));
    // The longest folder that holds the project wins, on a path boundary.
    assert_eq!(run_settings_in(text, "/w/tool").checks, ["make"]);
    assert_eq!(run_settings_in(text, "/w/app2").checks, ["make"]);
    assert_eq!(run_settings_in(text, "/w/app2").setup, None);
    // No entry: no checks, and no default list.
    let none = run_settings_in(text, "/elsewhere");
    assert!(none.checks.is_empty());
    let empty = run_settings_in("{ broken", "/w/app");
    assert_eq!(
        (empty.harness.as_str(), empty.model, empty.checks.len()),
        ("claude", None, 0)
    );
}

#[test]
fn micro_worlds_read_dsh_visuals_folder_with_the_home_folder_for_a_tilde() {
    let home = std::path::Path::new("/Users/d");
    let at = |text: &str| super::micro_worlds_dsh_visual_in(text, home);
    assert_eq!(
        at(r#"{ "microWorlds": { "dshVisual": "~/work/dsh-visual/scripts", }, }"#),
        Some("/Users/d/work/dsh-visual/scripts".into())
    );
    assert_eq!(
        at(r#"{ "microWorlds": { "dshVisual": " /opt/dsh/scripts " } }"#),
        Some("/opt/dsh/scripts".into())
    );
    for unset in [
        r#"{}"#,
        r#"{ "microWorlds": { "dshVisual": "" } }"#,
        r#"{ "microWorlds": { "dshVisual": 3 } }"#,
        "{ broken",
    ] {
        assert_eq!(at(unset), None, "{unset}");
    }
}

#[test]
fn the_explanations_switch_writes_its_one_key_and_keeps_the_rest() {
    let home = tempfile::tempdir().unwrap();
    let file = home.path().join("settings.json");
    std::fs::write(&file, "{ // mine\n  \"checks\": {},\n}").unwrap();
    let before = std::fs::read_to_string(&file).unwrap();
    let off = save_explanations_in(&file, Some(&before), false).unwrap();
    let json: serde_json::Value = serde_json::from_str(&plain_json(&off)).unwrap();
    assert_eq!(json["explanations"], json!(false), "{off}");
    assert!(off.contains("// mine"), "{off}");
    let on = save_explanations_in(&file, Some(&off), true).unwrap();
    let json: serde_json::Value = serde_json::from_str(&plain_json(&on)).unwrap();
    assert_eq!(json["explanations"], json!(true), "{on}");
    // A page that read an older text is refused.
    assert!(save_explanations_in(&file, Some(&before), false).is_err());
}

#[test]
fn a_saved_writer_is_read_back_as_the_same_writer() {
    let home = tempfile::tempdir().unwrap();
    let file = home.path().join("settings.json");
    for kind in WriterKind::ALL {
        let text = save_writer_in(&file, None, kind, "auto", "").unwrap();
        assert_eq!(summary_writer_in(&text), kind, "{text}");
        std::fs::remove_file(&file).unwrap();
    }
}

#[test]
fn a_fresh_home_gets_no_summaries_until_a_writer_passes_test() {
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("plotkeeper");
    assert!(!super::summaries_open(&data));
    // The start then makes the writer's folder; the next start is still gated.
    std::fs::create_dir_all(data.join("card-writer")).unwrap();
    assert!(!super::summaries_open(&data));
    super::open_summaries(&data).unwrap();
    assert!(super::summaries_open(&data));
}

#[test]
fn a_data_folder_an_earlier_plotkeeper_used_writes_summaries_with_no_test() {
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("plotkeeper");
    std::fs::create_dir_all(data.join("card-writer")).unwrap();
    std::fs::write(data.join("events.jsonl"), "").unwrap();
    assert!(super::summaries_open(&data));
    assert!(super::summaries_open(&data), "and at every start after");
}

#[test]
fn a_settings_file_that_names_the_writer_writes_summaries_with_no_test() {
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("plotkeeper");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(
        data.join("settings.json"),
        "{ \"summaries\": { \"writer\": \"agy\", }, }",
    )
    .unwrap();
    assert!(super::summaries_open(&data));
    // A key that names no writer is not that.
    std::fs::write(
        data.join("settings.json"),
        r#"{ "summaries": { "model": "haiku" } }"#,
    )
    .unwrap();
    assert!(!super::summaries_open(&data));
}

fn path(keys: &[&str]) -> Vec<String> {
    keys.iter().map(ToString::to_string).collect()
}

fn parsed(text: &str) -> serde_json::Value {
    serde_json::from_str(&plain_json(text)).unwrap()
}

#[test]
fn the_brief_backfill_is_two_hours_unless_the_settings_say_otherwise() {
    let default = studio_core::CatchUp::default();
    assert_eq!(default.window_ms, 7_200_000);
    assert_eq!(brief_backfill_in(""), default);
    assert_eq!(
        brief_backfill_in("{ \"catchUp\": { \"hours\": \"x\" } }"),
        default
    );
    let set = brief_backfill_in("{ // wider\n \"catchUp\": { \"hours\": 6, }, }");
    assert_eq!(set.window_ms, 21_600_000);
}

#[test]
fn the_brief_backfill_reads_its_new_key_first_and_the_old_catch_up_key_when_it_is_absent() {
    let new = brief_backfill_in(r#"{ "briefBackfill": { "hours": 4 } }"#);
    assert_eq!(new.window_ms, 14_400_000);
    let both =
        brief_backfill_in(r#"{ "briefBackfill": { "hours": 1 }, "catchUp": { "hours": 6 } }"#);
    assert_eq!(both.window_ms, 3_600_000, "the new key wins");
    let old = brief_backfill_in(r#"{ "catchUp": { "hours": 6 } }"#);
    assert_eq!(old.window_ms, 21_600_000, "the old key still works");
}

#[test]
fn the_pr_briefs_model_is_opus_unless_the_settings_name_another() {
    assert_eq!(pr_briefs_model_in(""), "opus");
    assert_eq!(pr_briefs_model_in("not json"), "opus");
    assert_eq!(
        pr_briefs_model_in(r#"{ "prBriefs": { "model": "" } }"#),
        "opus"
    );
    assert_eq!(
        pr_briefs_model_in(r#"{ "prBriefs": { "model": 3 } }"#),
        "opus"
    );
    let set = "{ // the briefs\n \"prBriefs\": { \"model\": \"sonnet\", }, }";
    assert_eq!(pr_briefs_model_in(set), "sonnet");
    // The summaries' model is its own setting.
    assert_eq!(
        pr_briefs_model_in(r#"{ "summaries": { "model": "haiku" } }"#),
        "opus"
    );
}

#[test]
fn the_pr_briefs_effort_is_low_unless_the_settings_name_another_or_claudes_own() {
    assert_eq!(pr_briefs_effort_in(""), "low");
    assert_eq!(
        pr_briefs_effort_in(r#"{ "prBriefs": { "model": "opus" } }"#),
        "low"
    );
    let medium = "{ // slower, more thought\n \"prBriefs\": { \"effort\": \"medium\", }, }";
    assert_eq!(pr_briefs_effort_in(medium), "medium");
    // Claude's own effort: no --effort at all.
    assert_eq!(
        pr_briefs_effort_in(r#"{ "prBriefs": { "effort": "default" } }"#),
        ""
    );
    assert_eq!(
        pr_briefs_effort_in(r#"{ "prBriefs": { "effort": "" } }"#),
        ""
    );
    assert_eq!(
        pr_briefs_effort_in(r#"{ "prBriefs": { "effort": "turbo" } }"#),
        "low"
    );
    assert_eq!(
        pr_briefs_effort_in(r#"{ "summaries": { "effort": "high" } }"#),
        "low"
    );
}

#[test]
fn the_summary_writer_is_claude_unless_the_settings_say_agy() {
    let agy = "{ // cards on my subscription\n \"summaries\": { \"writer\": \"agy\", }, }";
    assert_eq!(summary_writer_in(agy), WriterKind::Agy);
    assert_eq!(
        summary_writer_in(r#"{ "summaries": { "model": "haiku" } }"#),
        WriterKind::Claude
    );
    assert_eq!(
        summary_writer_in(r#"{ "summaries": { "writer": "nope" } }"#),
        WriterKind::Claude
    );
    assert_eq!(summary_writer_in("not json"), WriterKind::Claude);
    assert_eq!(summary_writer_in(""), WriterKind::Claude);
}

#[test]
fn a_comment_and_a_trailing_comma_beside_the_edited_key_survive_byte_for_byte() {
    let text = "{\n  // The sounds.\n  \"sounds\": true, /* kept */\n  \"zoom\": 1,\n}\n";
    let edited = edit_text(text, &path(&["sounds"]), Some(&json!(false))).unwrap();
    assert_eq!(
        edited,
        "{\n  // The sounds.\n  \"sounds\": false, /* kept */\n  \"zoom\": 1,\n}\n"
    );
}

#[test]
fn a_value_is_replaced_in_place() {
    let text = "{ \"zoom\": 1, \"terminal\": { \"font_size\": 12 } }";
    let edited = edit_text(text, &path(&["terminal", "font_size"]), Some(&json!(15))).unwrap();
    assert_eq!(
        edited,
        "{ \"zoom\": 1, \"terminal\": { \"font_size\": 15 } }"
    );
    let edited = edit_text(text, &path(&["zoom"]), Some(&json!(1.2))).unwrap();
    assert_eq!(
        edited,
        "{ \"zoom\": 1.2, \"terminal\": { \"font_size\": 12 } }"
    );
    let edited = edit_text(
        r#"{ "summaries": { "model": "haiku" } }"#,
        &path(&["summaries", "model"]),
        Some(&json!("opus")),
    )
    .unwrap();
    assert_eq!(edited, r#"{ "summaries": { "model": "opus" } }"#);
}

#[test]
fn a_key_whose_parent_is_missing_is_inserted_with_it() {
    let text = "{\n  // Mine.\n  \"zoom\": 1.2, // kept\n}\n";
    let edited = edit_text(text, &path(&["terminal", "font_size"]), Some(&json!(15))).unwrap();
    assert!(
        edited.starts_with("{\n  // Mine.\n  \"zoom\": 1.2, // kept\n"),
        "{edited}"
    );
    assert_eq!(
        parsed(&edited),
        json!({ "zoom": 1.2, "terminal": { "font_size": 15 } })
    );
    // A key missing from an existing parent is added beside its siblings.
    let edited = edit_text(
        r#"{ "terminal": { "other": 1 } }"#,
        &path(&["terminal", "font_size"]),
        Some(&json!(15)),
    )
    .unwrap();
    assert_eq!(
        parsed(&edited),
        json!({ "terminal": { "other": 1, "font_size": 15 } })
    );
}

#[test]
fn reset_removes_the_key_and_leaves_the_rest_of_the_text() {
    let text = "{\n  // Mine.\n  \"zoom\": 1.2,\n  \"sounds\": false, // quiet\n}\n";
    let edited = edit_text(text, &path(&["zoom"]), None).unwrap();
    assert_eq!(edited, "{\n  // Mine.\n  \"sounds\": false, // quiet\n}\n");
    // A key that is not there is already reset: the text comes back as it was.
    assert_eq!(
        edit_text(text, &path(&["terminal", "font_size"]), None).unwrap(),
        text
    );
}

#[test]
fn a_text_that_does_not_parse_is_refused_with_its_reason() {
    for (text, key, reason) in [
        ("{ \"zoom\": ", &["zoom"][..], "could not be parsed"),
        ("{ \"zoom\" 1 }", &["zoom"], "could not be parsed"),
        ("[1, 2]", &["zoom"], "settings.json is not an object"),
        (
            "{ \"terminal\": 5 }",
            &["terminal", "font_size"],
            "terminal is not an object",
        ),
    ] {
        let refused = edit_text(text, &path(key), Some(&json!(1))).unwrap_err();
        assert!(refused.contains(reason), "{text}: {refused}");
    }
}

#[test]
fn an_empty_text_is_the_defaults() {
    for text in ["", "  \n"] {
        let edited = edit_text(text, &path(&["zoom"]), Some(&json!(1.5))).unwrap();
        assert_eq!(parsed(&edited), json!({ "zoom": 1.5 }), "{text:?}");
    }
}

#[test]
fn a_write_saves_the_edited_text_and_returns_it() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.json");
    let text = "{\n  // Mine.\n  \"sounds\": true,\n}\n";
    std::fs::write(&file, text).unwrap();
    let written = write_in(&file, Some(text), &path(&["sounds"]), Some(&json!(false))).unwrap();
    let on_disk = "{\n  // Mine.\n  \"sounds\": false,\n}\n";
    assert_eq!(written, on_disk);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), on_disk);
}

#[test]
fn a_file_that_differs_from_the_expected_text_is_refused_and_left_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.json");
    let saved = "{ \"zoom\": 1.5 } // saved from the editor";
    std::fs::write(&file, saved).unwrap();
    let refused = write_in(
        &file,
        Some("{ \"zoom\": 1 }"),
        &path(&["sounds"]),
        Some(&json!(false)),
    )
    .unwrap_err();
    assert!(refused.contains("changed"), "{refused}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), saved);
    // Nor does a file appear where the page read none.
    let refused = write_in(&file, None, &path(&["sounds"]), Some(&json!(false))).unwrap_err();
    assert!(refused.contains("changed"), "{refused}");
}

#[test]
fn a_file_that_does_not_parse_is_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.json");
    let broken = "{ \"zoom\": 1,, }";
    std::fs::write(&file, broken).unwrap();
    let refused =
        write_in(&file, Some(broken), &path(&["sounds"]), Some(&json!(false))).unwrap_err();
    assert!(refused.contains("could not be parsed"), "{refused}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), broken);
}

#[test]
fn a_text_that_is_more_than_jsonc_is_refused_and_the_file_left_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.json");
    for text in [
        "{\n \"zoom\": 1\n \"sounds\": true\n}",
        "{ zoom: 1 }",
        "{ 'zoom': 1 }",
        "{ \"zoom\": 0x10 }",
        "{ \"zoom\": +1 }",
        "{ \"zoom\": .5 }",
        "{ \"zoom\": NaN }",
    ] {
        std::fs::write(&file, text).unwrap();
        let refused =
            write_in(&file, Some(text), &path(&["sounds"]), Some(&json!(false))).unwrap_err();
        assert!(refused.contains("could not be parsed"), "{text}: {refused}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
    }
}

#[test]
fn a_write_replaces_the_file_whole_and_leaves_nothing_beside_it() {
    use std::os::unix::fs::MetadataExt;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.json");
    std::fs::write(&file, "{ \"sounds\": true }").unwrap();
    let before = std::fs::metadata(&file).unwrap().ino();
    write_in(
        &file,
        Some("{ \"sounds\": true }"),
        &path(&["sounds"]),
        Some(&json!(false)),
    )
    .unwrap();
    // A new file renamed over the old one, so a crash mid-write never leaves a truncated settings.json.
    assert_ne!(std::fs::metadata(&file).unwrap().ino(), before);
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["settings.json"]);
}

#[test]
fn a_settings_file_that_is_a_symlink_is_written_through_and_stays_a_link() {
    let dir = tempfile::tempdir().unwrap();
    let dotfiles = dir.path().join("dotfiles");
    std::fs::create_dir(&dotfiles).unwrap();
    let target = dotfiles.join("settings.json");
    std::fs::write(&target, "{ \"sounds\": true }").unwrap();
    let link = dir.path().join("settings.json");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    write_in(
        &link,
        Some("{ \"sounds\": true }"),
        &path(&["sounds"]),
        Some(&json!(false)),
    )
    .unwrap();
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(std::fs::read_link(&link).unwrap(), target);
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "{ \"sounds\": false }"
    );
    // A relative link, as dotfile managers make, is followed from the link's folder.
    let relative = dir.path().join("keymap.json");
    std::os::unix::fs::symlink("dotfiles/keymap.json", &relative).unwrap();
    std::fs::write(dotfiles.join("keymap.json"), "{}").unwrap();
    write_in(&relative, Some("{}"), &path(&["zoom"]), Some(&json!(1.5))).unwrap();
    assert!(std::fs::symlink_metadata(&relative).unwrap().is_symlink());
    assert_eq!(
        parsed(&std::fs::read_to_string(dotfiles.join("keymap.json")).unwrap()),
        json!({ "zoom": 1.5 })
    );
}

#[test]
fn a_missing_file_is_created_with_the_one_key() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("data").join("settings.json");
    let written = write_in(
        &file,
        None,
        &path(&["terminal", "font_size"]),
        Some(&json!(15)),
    )
    .unwrap();
    assert_eq!(parsed(&written), json!({ "terminal": { "font_size": 15 } }));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), written);
    // Resetting in a missing file creates nothing.
    let none = dir.path().join("none.json");
    write_in(&none, None, &path(&["zoom"]), None).unwrap();
    assert!(!none.exists());
}

#[test]
fn the_repository_setting_reads_zeds_format() {
    let text = r#"{
        // The engine, not the studio's own checkout.
        "repository": "/Users/d/work/Falinks", /* a comment */
        "editor": ["zed", "{path}//x"],
    }"#;
    assert_eq!(
        repository_in(text).as_deref(),
        Some("/Users/d/work/Falinks")
    );
    assert_eq!(
        repository_in(r#"{ "repository": "/a//b,}" }"#).as_deref(),
        Some("/a//b,}")
    );
}

#[test]
fn an_unset_empty_or_wrong_repository_setting_is_none() {
    for text in [
        "",
        "{}",
        r#"{"repository": ""}"#,
        r#"{"repository": 3}"#,
        "{ broken",
    ] {
        assert_eq!(repository_in(text), None, "{text}");
    }
}

/// The OpenAI-compatible writer's base URL (#367): a non-empty string under `summaries`, in Zed's format.
#[test]
fn the_base_url_is_read_from_summaries() {
    use super::base_url_in;
    let read = |text: &str| base_url_in(text);
    assert_eq!(
        read("{ // Ollama\n \"summaries\": { \"base_url\": \"http://localhost:11434/v1\", } }")
            .as_deref(),
        Some("http://localhost:11434/v1")
    );
    for text in [
        "",
        "{}",
        r#"{ "summaries": { "base_url": "" } }"#,
        r#"{ "summaries": { "base_url": 1 } }"#,
        r#"{ "base_url": "http://x" }"#,
    ] {
        assert_eq!(read(text), None, "{text}");
    }
}

/// The Keychain through a stand-in `security` that keeps one entry in a file: the key goes in on standard input, never
/// in the arguments, and only plotkeeper's own entry is named. The stand-in splits its input line as `security -i`
/// does (`split_line` in Apple's `SecurityTool/macOS/security.c`): whitespace between words, a word that starts with
/// `"` or `'` runs to the same quote, and a backslash takes the next character as it is. The real Keychain is never
/// touched.
#[test]
fn the_key_is_saved_replaced_and_removed_in_plotkeepers_own_keychain_entry() {
    use super::{ApiKeys, Keychain};
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let (store, log) = (dir.path().join("store"), dir.path().join("log"));
    let program = dir.path().join("security");
    let split = dir.path().join("split.pl");
    std::fs::write(
        &split,
        r#"my $line = <STDIN>; chomp $line; my (@words, $word, $quote); my $state = "space";
for my $c (split //, $line) {
  if ($state eq "space") { next if $c =~ /\s/;
if ($c eq '"' || $c eq "'") { ($quote, $word, $state) = ($c, "", "quoted") }
elsif ($c eq "\\") { ($word, $state) = ("", "escaped") } else { ($word, $state) = ($c, "word") } }
  elsif ($state eq "word") { if ($c =~ /\s/) { push @words, $word; $state = "space" }
elsif ($c eq "\\") { $state = "escaped" } else { $word .= $c } }
  elsif ($state eq "escaped") { $word .= $c; $state = "word" }
  elsif ($state eq "quoted") { if ($c eq $quote) { push @words, $word; $state = "space" }
elsif ($c eq "\\") { $state = "quoted-escaped" } else { $word .= $c } }
  else { $word .= $c; $state = "quoted" } }
push @words, $word if $state ne "space"; print "$words[-1]\n";
"#,
    )
    .unwrap();
    std::fs::write(
        &program,
        format!(
            r#"#!/bin/sh
echo "ARGS $*" >> '{log}'
case "$1" in
  -i) /usr/bin/perl '{split}' > '{store}';;
  find-generic-password) [ -f '{store}' ] && cat '{store}' || exit 44;;
  delete-generic-password) [ -f '{store}' ] && rm '{store}' || exit 44;;
esac
"#,
            log = log.display(),
            store = store.display(),
            split = split.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let keys = Keychain::with_program(program);
    use studio_understanding::WriterKind::{Anthropic, OpenAI};

    assert_eq!(keys.get(Anthropic), None);
    keys.set(Anthropic, "sk-ant-first-0001").unwrap();
    assert_eq!(keys.get(Anthropic).as_deref(), Some("sk-ant-first-0001"));
    keys.set(Anthropic, "sk-ant-second-0002").unwrap();
    assert_eq!(keys.get(Anthropic).as_deref(), Some("sk-ant-second-0002"));
    keys.remove(Anthropic).unwrap();
    assert_eq!(keys.get(Anthropic), None);
    keys.remove(Anthropic).unwrap();
    assert!(keys.set(Anthropic, "not a key; rm -rf").is_err());
    // A key with a dot, a quote or a backslash goes in whole.
    for key in [
        "sk-ant-dotted.0004",
        r#"sk-ant-"quoted"-0005"#,
        "sk-ant-'single'-0006",
        r"sk-ant-back\slash-0007",
    ] {
        assert_eq!(keys.set(Anthropic, key), Ok(()), "{key}");
        assert_eq!(keys.get(Anthropic).as_deref(), Some(key));
    }
    keys.remove(Anthropic).unwrap();
    // The OpenAI-compatible API's key, in its own entry (#367).
    keys.set(OpenAI, "sk-proj-third-0003").unwrap();
    assert_eq!(keys.get(OpenAI).as_deref(), Some("sk-proj-third-0003"));
    keys.remove(OpenAI).unwrap();

    let log = std::fs::read_to_string(log).unwrap();
    for line in log.lines() {
        assert!(
            !line.contains("sk-ant") && !line.contains("sk-proj"),
            "a key in the arguments: {line}"
        );
        assert!(
            line == "ARGS -i"
                || line.contains("-s plotkeeper -a anthropic-api-key")
                || line.contains("-s plotkeeper -a openai-api-key"),
            "{line}"
        );
    }
}
