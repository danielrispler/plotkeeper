// Removed from core/tests/landed.rs in #415 with `Studio::set_task_safe`, which only tests called. `facts_of_a_task_no_session_works_in_are_dropped` now reads `task_facts_at` in its place.


#[test]
fn the_git_half_alone_clears_and_restores_the_mark() {
    let mut studio = Studio::default();
    at_your_turn(&mut studio, "a", TASK);
    assert!(!studio.set_task_safe(TASK, true), "no facts to change yet");
    studio.set_task_facts(TASK, Some(SHIPPED));
    assert!(studio.set_task_safe(TASK, false), "a new commit or file");
    assert!(!landed(&studio, "a"));
    assert!(studio.set_task_safe(TASK, true));
    assert!(landed(&studio, "a"));
    assert!(!studio.set_task_safe(TASK, true));
}
