//! #21: the studio plugin reached through `checkout` in the data folder, so the profile's
//! `CLAUDE_CODE_PLUGIN_DIRS` survives a moved checkout, and the profile fix that points it there.

use std::{
    fs,
    path::{Path, PathBuf},
};
use studio_workbench::{fixed_profile, link_checkout, loads_plugin};

const LINK: &str = "$HOME/Library/Application Support/dev.falinks.studio/checkout/plugin";

/// A checkout in `dir` with the studio plugin's manifest.
fn checkout(dir: &Path) -> PathBuf {
    fs::create_dir_all(dir.join("plugin/.claude-plugin")).unwrap();
    fs::write(
        dir.join("plugin/.claude-plugin/plugin.json"),
        r#"{ "name": "falinks-studio" }"#,
    )
    .unwrap();
    dir.to_path_buf()
}

#[test]
fn the_link_follows_the_checkout_and_never_replaces_a_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data with space");
    let (old, new) = (
        checkout(&tmp.path().join("old")),
        checkout(&tmp.path().join("new")),
    );
    let plugin = link_checkout(&data, &old).unwrap();
    assert_eq!(plugin, data.join("checkout/plugin"));
    assert_eq!(
        fs::canonicalize(&plugin).unwrap(),
        fs::canonicalize(old.join("plugin")).unwrap()
    );
    link_checkout(&data, &new).unwrap();
    assert_eq!(
        fs::canonicalize(&plugin).unwrap(),
        fs::canonicalize(new.join("plugin")).unwrap()
    );

    let real = tmp.path().join("real");
    fs::create_dir_all(real.join("checkout")).unwrap();
    assert!(link_checkout(&real, &new).is_err());
    assert!(real.join("checkout").is_dir());
}

#[test]
fn an_entry_loads_the_plugin_by_its_real_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let ours = checkout(&tmp.path().join("ours")).join("plugin");
    let linked = link_checkout(&tmp.path().join("data"), &tmp.path().join("ours")).unwrap();
    let missing = tmp.path().join("gone/plugin");
    let (ours_s, linked_s, missing_s) = (ours.display(), linked.display(), missing.display());
    assert!(loads_plugin(&format!("{ours_s}"), &ours));
    assert!(loads_plugin(&format!("{linked_s}"), &ours));
    assert!(loads_plugin(&format!("{missing_s}:{linked_s}"), &ours));
    assert!(!loads_plugin(&format!("{missing_s}"), &ours));
    assert!(!loads_plugin("", &ours));
}

#[test]
fn the_fix_drops_other_studio_entries_and_keeps_the_rest_as_written() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let other_studio = checkout(&home.join("other")).join("plugin");
    checkout(&home.join("old"));
    let profile = format!(
        "export PATH=/x:$PATH\nif true; then\n  export CLAUDE_CODE_PLUGIN_DIRS=\"/mine:/gone/plugin:{}:$HOME/old/plugin:~/p\"\nfi\n",
        other_studio.display(),
    );
    let fixed = fixed_profile(&profile, LINK, home).unwrap();
    assert_eq!(
        fixed,
        format!(
            "export PATH=/x:$PATH\nif true; then\n  export CLAUDE_CODE_PLUGIN_DIRS=\"/mine:/gone/plugin:~/p:{LINK}\"\nfi\n"
        ),
    );
    assert_eq!(fixed_profile(&fixed, LINK, home).unwrap(), fixed);
}

#[test]
fn the_fix_reads_a_bare_value_and_appends_a_line_when_there_is_none() {
    let home = Path::new("/nowhere");
    assert_eq!(
        fixed_profile("export CLAUDE_CODE_PLUGIN_DIRS=/gone/plugin\n", LINK, home).unwrap(),
        format!("export CLAUDE_CODE_PLUGIN_DIRS=\"/gone/plugin:{LINK}\"\n"),
    );
    let appended = fixed_profile("alias g=git", LINK, home).unwrap();
    assert_eq!(
        appended,
        format!(
            "alias g=git\n# The studio plugin's hooks, for every claude session (#21).\nexport CLAUDE_CODE_PLUGIN_DIRS=\"{LINK}\"\n"
        ),
    );
    assert_eq!(fixed_profile(&appended, LINK, home).unwrap(), appended);
}

#[test]
fn the_fix_leaves_a_line_with_an_expansion_alone() {
    let profile = "export CLAUDE_CODE_PLUGIN_DIRS=\"${CLAUDE_CODE_PLUGIN_DIRS:+$CLAUDE_CODE_PLUGIN_DIRS:}/opt/p\"\n";
    assert_eq!(fixed_profile(profile, LINK, Path::new("/nowhere")), None);
}
