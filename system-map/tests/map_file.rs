//! What the check says about the file itself.

use serde_json::{Value, json};

use crate::support::{FILES, base, has, problems, project, project_text};

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
