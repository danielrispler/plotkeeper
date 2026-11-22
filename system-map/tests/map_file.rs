//! What the check says about the file itself.

use serde_json::{Value, json};

use crate::common::{FILES, base, has, problems, project, project_text};

fn failing(map: Value) -> Vec<String> {
    let (_dir, root) = project(&map, &FILES);
    problems(&root)
}

#[test]
fn a_valid_map_passes() {
    let (_dir, root) = project(&base(), &FILES);
    assert_eq!(problems(&root), Vec::<String>::new());
}

#[test]
fn a_map_with_flows_extractors_and_a_zone_passes() {
    let mut map = base();
    map["extractors"] = json!([]);
    map["parts"][0]["zone"] = json!("left");
    map["flows"] = json!([{ "id": "walk", "name": "Walk",
        "hops": [{ "link": "alpha-beta", "text": "Alpha hands over." }] }]);
    map["laws"][0]["on"] = json!("walk");
    assert_eq!(failing(map), Vec::<String>::new());
}

#[test]
fn a_missing_file_fails_and_says_where_to_put_it() {
    let (dir, root) = project(&base(), &FILES);
    std::fs::remove_file(dir.path().join("system-map.json")).unwrap();
    assert!(has(&problems(&root), &["system-map.json", "missing"]));
}

#[test]
fn a_file_that_is_not_json_fails() {
    let (_dir, root) = project_text("parts: []", &FILES);
    assert!(has(&problems(&root), &["system-map.json", "not valid"]));
}

#[test]
fn an_unknown_field_fails_and_names_it() {
    let mut map = base();
    map["parts"][0]["colour"] = json!("red");
    assert!(has(&failing(map), &["colour"]));
}

#[test]
fn a_tier_outside_the_three_fails() {
    let mut map = base();
    map["parts"][0]["tier"] = json!("middling");
    assert!(has(&failing(map), &["middling"]));
}

#[test]
fn an_id_repeated_across_kinds_fails_and_names_it() {
    let mut map = base();
    map["actors"][0]["id"] = json!("alpha");
    assert!(has(&failing(map), &["alpha", "more than once"]));
}

#[test]
fn an_id_that_is_not_lowercase_words_fails() {
    for bad in ["Alpha", "two_words", "-lead", "trail-", "a--b", ""] {
        let mut map = base();
        map["laws"][0]["id"] = json!(bad);
        assert!(has(&failing(map), &["id", "lowercase words"]), "{bad:?}");
    }
}

#[test]
fn a_link_end_that_is_neither_part_nor_actor_fails() {
    let mut map = base();
    map["links"][0]["to"] = json!("nowhere");
    assert!(has(&failing(map), &["alpha-beta", "nowhere"]));
}

#[test]
fn a_link_to_an_actor_passes() {
    let mut map = base();
    map["links"][0]["to"] = json!("writer");
    assert_eq!(failing(map), Vec::<String>::new());
}

#[test]
fn a_law_on_something_absent_fails() {
    let mut map = base();
    map["laws"][0]["on"] = json!("ghost");
    assert!(has(&failing(map), &["stay-true", "ghost"]));
}

#[test]
fn a_hop_on_a_link_that_is_absent_fails() {
    let mut map = base();
    map["flows"] = json!([{ "id": "walk", "name": "Walk",
        "hops": [{ "link": "ghost-link", "text": "Nothing." }] }]);
    assert!(has(&failing(map), &["walk", "ghost-link"]));
}

#[test]
fn a_version_other_than_one_fails_and_names_it() {
    let mut map = base();
    map["version"] = json!(2);
    assert!(has(&failing(map), &["version", "2", "version 1"]));
}

mod lenient {
    use serde_json::json;
    use system_map::{parse, parse_lenient, touched_for_map};

    use crate::common::base;

    fn with_extras() -> serde_json::Value {
        let mut map = base();
        map["future"] = json!(true);
        map["parts"][0]["colour"] = json!("red");
        map["actors"][0]["icon"] = json!("x");
        map["links"][0]["weight"] = json!(2);
        map["laws"][0]["severity"] = json!("high");
        map["flows"] = json!([{ "id": "go", "name": "Go", "extra": 1,
            "hops": [{ "link": "alpha-beta", "text": "t", "extra": 1 }] }]);
        map
    }

    #[test]
    fn unknown_fields_are_ignored_by_the_lenient_parser_and_refused_by_the_strict() {
        let text = with_extras().to_string();
        assert!(parse(&text).is_err());
        let map = parse_lenient(&text).expect("lenient parse");
        let got = touched_for_map(&map, &["alpha/a.rs".to_string()]);
        assert_eq!(got.parts[0].id, "alpha");
    }

    #[test]
    fn the_lenient_parser_fails_clearly_on_bad_json_and_on_a_missing_core_field() {
        let error = parse_lenient("{ nope").unwrap_err();
        assert!(error.contains("not valid JSON"), "{error}");
        let mut map = base();
        map.as_object_mut().unwrap().remove("parts");
        let error = parse_lenient(&map.to_string()).unwrap_err();
        assert!(error.contains("parts"), "{error}");
    }
}
