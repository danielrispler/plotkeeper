//! Version 2 of a part: its `purpose`, the items it `holds` and their caps, the conversion of a version 1 map and the
//! prompt renderer.

use serde_json::{Value, json};

use crate::common::{FILES, base, base_v1, has, problems, project};

fn failing(map: Value) -> Vec<String> {
    let (_dir, root) = project(&map, &FILES);
    problems(&root)
}

fn with_holds(holds: Value) -> Value {
    let mut map = base();
    map["parts"][0]["holds"] = holds;
    map
}

#[test]
fn a_part_with_held_items_passes() {
    let map = with_holds(json!([
        { "name": "The reader", "text": "Reads the file.", "paths": ["alpha/a.rs"] },
        { "name": "The rules", "text": "What must hold." }
    ]));
    assert_eq!(failing(map), Vec::<String>::new());
}

#[test]
fn a_part_without_holds_fails() {
    let mut map = base();
    map["parts"][0].as_object_mut().unwrap().remove("holds");
    assert!(has(&failing(map), &["holds"]));
}

#[test]
fn a_version_1_map_fails_the_check_and_says_to_move_to_version_2() {
    assert!(has(&failing(base_v1()), &["version 1", "version 2"]));
}

#[test]
fn a_purpose_over_the_cap_fails_with_its_length_the_cap_and_where_the_detail_goes() {
    let mut map = base();
    map["parts"][0]["purpose"] = json!("a".repeat(3986));
    let found = failing(map);
    assert!(
        has(
            &found,
            &[
                "part \"alpha\"",
                "purpose is 3,986 characters",
                "the cap is 240",
                "keep the job here, move the detail into a held item or the area's doc under docs/studio/behaviour/, or split the part"
            ]
        ),
        "{found:?}"
    );
}

#[test]
fn a_purpose_at_the_cap_passes_counted_in_characters() {
    let mut map = base();
    map["parts"][0]["purpose"] = json!("é".repeat(240));
    assert_eq!(failing(map), Vec::<String>::new());
}

#[test]
fn a_held_text_over_the_cap_fails_with_its_length_and_the_cap() {
    let map = with_holds(json!([{ "name": "Reader", "text": "b".repeat(141) }]));
    let found = failing(map);
    assert!(
        has(
            &found,
            &[
                "part \"alpha\"",
                "\"Reader\"",
                "141 characters",
                "the cap is 140",
                "the area's doc under docs/studio/behaviour/"
            ]
        ),
        "{found:?}"
    );
}

#[test]
fn more_than_eight_held_items_fail_and_say_the_part_may_need_splitting() {
    let items: Vec<Value> = (0..9)
        .map(|n| json!({ "name": format!("Item {n}"), "text": "One thing." }))
        .collect();
    let found = failing(with_holds(Value::Array(items)));
    assert!(
        has(
            &found,
            &[
                "part \"alpha\"",
                "9 items",
                "the cap is 8",
                "the area's doc under docs/studio/behaviour/",
                "split"
            ]
        ),
        "{found:?}"
    );
}

#[test]
fn an_empty_purpose_name_or_text_fails() {
    let mut map = base();
    map["parts"][0]["purpose"] = json!("  ");
    assert!(has(&failing(map), &["part \"alpha\"", "purpose is empty"]));
    let found = failing(with_holds(json!([{ "name": "", "text": "One thing." }])));
    assert!(
        has(&found, &["part \"alpha\"", "name is empty"]),
        "{found:?}"
    );
    let found = failing(with_holds(json!([{ "name": "Reader", "text": "" }])));
    assert!(
        has(&found, &["part \"alpha\"", "\"Reader\"", "text is empty"]),
        "{found:?}"
    );
}

#[test]
fn two_held_items_of_one_name_fail() {
    let found = failing(with_holds(json!([
        { "name": "Reader", "text": "One." },
        { "name": "Reader", "text": "Two." }
    ])));
    assert!(
        has(&found, &["part \"alpha\"", "\"Reader\"", "more than once"]),
        "{found:?}"
    );
}

#[test]
fn a_held_items_path_must_match_a_file_its_part_owns() {
    let found = failing(with_holds(json!([
        { "name": "Reader", "text": "Reads.", "paths": ["beta/b.rs"] }
    ])));
    assert!(
        has(
            &found,
            &["part \"alpha\"", "\"Reader\"", "beta/b.rs", "owns"]
        ),
        "{found:?}"
    );
}

#[test]
fn an_unknown_field_of_a_held_item_fails() {
    let found = failing(with_holds(
        json!([{ "name": "Reader", "text": "Reads.", "colour": "red" }]),
    ));
    assert!(has(&found, &["colour"]), "{found:?}");
}

mod conversion {
    use serde_json::{Value, json};
    use system_map::{convert, parse_lenient};

    use crate::common::{base, base_v1};

    #[test]
    fn a_version_1_map_becomes_version_2_with_its_description_as_purpose_untruncated_and_no_held_items()
     {
        let mut old = base_v1();
        let long = "Reads the log. ".repeat(300);
        old["parts"][0]["description"] = json!(long);
        let new: Value = serde_json::from_str(&convert(&old.to_string()).unwrap()).unwrap();
        assert_eq!(new["version"], 2);
        assert_eq!(new["parts"][0]["purpose"], json!(long));
        assert_eq!(new["parts"][1]["purpose"], "The second part.");
        assert_eq!(new["parts"][0]["holds"], json!([]));
        assert!(new["parts"][0].get("description").is_none());
        assert_eq!(new["links"], old["links"]);
        assert_eq!(new["laws"], old["laws"]);
    }

    #[test]
    fn a_link_without_a_kind_keeps_none() {
        let new: Value = serde_json::from_str(&convert(&base_v1().to_string()).unwrap()).unwrap();
        assert_eq!(
            new["links"],
            json!([{ "id": "alpha-beta", "from": "alpha", "to": "beta" }])
        );
    }

    #[test]
    fn a_version_2_map_comes_back_as_it_was() {
        let text = format!("{:#}", base());
        assert_eq!(convert(&text).unwrap(), text);
    }

    #[test]
    fn text_that_is_not_json_fails() {
        assert!(convert("{ nope").unwrap_err().contains("not valid JSON"));
    }

    #[test]
    fn the_lenient_reader_reads_a_version_1_map() {
        assert!(parse_lenient(&base_v1().to_string()).is_ok());
    }
}

mod prompt {
    use serde_json::{Value, json};
    use system_map::{convert, part_text};

    use crate::common::{base, base_v1};

    #[test]
    fn a_part_reads_as_its_purpose_then_each_held_item_with_its_key_files() {
        let mut map = base();
        map["parts"][0]["holds"] = json!([
            { "name": "The reader", "text": "Reads the file.", "paths": ["alpha/a.rs", "alpha/b/"] },
            { "name": "The rules", "text": "What must hold." }
        ]);
        assert_eq!(
            part_text(&map["parts"][0]),
            "The first part.\n  - The reader: Reads the file. (key files: alpha/a.rs, alpha/b/)\n  - The rules: What must hold."
        );
    }

    #[test]
    fn a_converted_version_1_part_reads_as_its_description() {
        let map: Value = serde_json::from_str(&convert(&base_v1().to_string()).unwrap()).unwrap();
        assert_eq!(part_text(&map["parts"][1]), "The second part.");
    }
}
