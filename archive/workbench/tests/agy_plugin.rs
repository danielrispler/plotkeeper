// Removed from workbench/tests/agy_plugin.rs in #415 with `agy_plugin::installed_is_valid`, which nothing else called.


#[test]
fn an_installed_copy_that_fails_validation_is_stale_though_it_matches_the_source() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let agy = fake_agy(tmp.path());
    install(&agy, &hook(), &home).unwrap();
    assert_eq!(status(&hook(), &home).unwrap(), Status::Current);
    assert!(installed_is_valid(&agy, &home));
    // agy now rejects the copy, and would skip it silently.
    fs::write(tmp.path().join("fail-validate"), "").unwrap();
    assert!(!installed_is_valid(&agy, &home));
}
