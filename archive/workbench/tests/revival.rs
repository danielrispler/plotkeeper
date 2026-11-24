// Removed from workbench/tests/revival.rs in #415 with `start`, the picker's command, which nothing else called.


#[test]
fn the_picker_starts_agy_and_codex_with_a_first_prompt_and_nothing_else() {
    use studio_workbench::start;
    assert_eq!(start("agy", ""), argv(&["agy"]));
    assert_eq!(
        start("agy", "-p fix it"),
        argv(&["agy", "--prompt-interactive=-p fix it"])
    );
    assert_eq!(start("codex", ""), argv(&["codex"]));
    assert_eq!(start("codex", "-p x"), argv(&["codex", "--", "-p x"]));
    assert_eq!(start("vim", ""), None);
}
