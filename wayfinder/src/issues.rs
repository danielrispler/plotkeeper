//! Reading the issues: a map's open tickets, every open issue repo-wide, the map to list and a ticket's blockers.

use crate::{Result, require};
use serde_json::{Value, json};

pub(crate) fn open_tickets(pages: &Value) -> Result<Vec<Value>> {
    let mut rows = vec![];
    for page in pages
        .as_array()
        .ok_or("expected paginated ticket metadata")?
    {
        for ticket in page.as_array().ok_or("expected a ticket page")? {
            if ticket["state"] != "open" {
                continue;
            }
            let title = ticket["title"].as_str().ok_or("missing ticket title")?;
            let blockers = ticket["issue_dependencies_summary"]["blocked_by"]
                .as_u64()
                .ok_or_else(|| format!("Unknown blockers: {title}"))?;
            let assignees = logins(&ticket["assignees"])?;
            rows.push(json!({"number":ticket["number"],"title":title,"url":ticket["html_url"],"assignees":assignees,"blocked_by":blockers,"frontier":blockers==0 && assignees.is_empty()}));
        }
    }
    Ok(rows)
}
/// Rows for every open issue, by number, from paginated GraphQL `issues` pages.
/// Truncated assignee or blocker lists are errors, so a row never hides a blocker.
pub(crate) fn repo_tickets(pages: &Value) -> Result<Vec<Value>> {
    let mut rows = vec![];
    for page in pages.as_array().ok_or("expected paginated issues")? {
        let issues = page["data"]["repository"]["issues"]["nodes"]
            .as_array()
            .ok_or("expected an issue page")?;
        for issue in issues {
            let number = issue["number"].as_u64().ok_or("missing issue number")?;
            let list = |field: &str, key: &str| -> Result<Vec<Value>> {
                let nodes = issue[field]["nodes"]
                    .as_array()
                    .ok_or_else(|| format!("#{number}: missing {field}"))?;
                require(
                    issue[field]["totalCount"].as_u64() == Some(nodes.len() as u64),
                    &format!("#{number}: {field} list truncated or missing totalCount"),
                )?;
                nodes
                    .iter()
                    .filter(|node| field != "blockedBy" || node["state"] == "OPEN")
                    .map(|node| {
                        (!node[key].is_null())
                            .then(|| node[key].clone())
                            .ok_or_else(|| format!("#{number}: {field} entry without {key}").into())
                    })
                    .collect()
            };
            let assignees = list("assignees", "login")?;
            let blockers = list("blockedBy", "number")?;
            let frontier = blockers.is_empty() && assignees.is_empty();
            rows.push(json!({"number":number,"title":issue["title"],"url":issue["url"],"parent":issue["parent"]["number"],"assignees":assignees,"blocked_by":blockers,"frontier":frontier}));
        }
    }
    rows.sort_by_key(|row| row["number"].as_u64());
    Ok(rows)
}
fn list_maps(maps: &[&Value]) -> String {
    if maps.is_empty() {
        return " none".into();
    }
    maps.iter()
        .map(|map| {
            let closed = if map["state"] == "OPEN" {
                ""
            } else {
                " (closed)"
            };
            format!(
                "\n  #{} {}{closed}",
                map["number"],
                map["title"].as_str().unwrap_or("")
            )
        })
        .collect()
}
/// Pick the requested issue if open, else the newest open `wayfinder:map`; never a closed map.
/// `fetch_maps` fetches the map list; an open requested issue never calls it.
pub(crate) fn select_map(
    requested: Option<&Value>,
    fetch_maps: impl FnOnce() -> Result<Value>,
) -> Result<u64> {
    if let Some(issue) = requested
        && issue["state"] == "OPEN"
    {
        return issue["number"]
            .as_u64()
            .ok_or_else(|| "missing map number".into());
    }
    let maps = fetch_maps()?;
    let maps = maps.as_array().ok_or("expected a map list")?;
    let mut open = maps
        .iter()
        .filter(|map| map["state"] == "OPEN")
        .collect::<Vec<_>>();
    open.sort_by_key(|map| std::cmp::Reverse(map["number"].as_u64()));
    if let Some(issue) = requested {
        let number = issue["number"].as_u64().ok_or("missing map number")?;
        return Err(format!("Map #{number} is closed. Open maps:{}", list_maps(&open)).into());
    }
    match open.first() {
        Some(map) => map["number"]
            .as_u64()
            .ok_or_else(|| "missing map number".into()),
        None => Err(format!(
            "No open wayfinder:map issue; pass a map number. Recent maps:{}",
            list_maps(&maps.iter().collect::<Vec<_>>())
        )
        .into()),
    }
}
/// Open blocker numbers from paginated `dependencies/blocked_by` pages.
pub(crate) fn open_blockers(pages: &Value) -> Result<Vec<u64>> {
    let mut numbers = vec![];
    for page in pages.as_array().ok_or("expected paginated blockers")? {
        for blocker in page.as_array().ok_or("expected a blocker page")? {
            let number = blocker["number"].as_u64().ok_or("missing blocker number")?;
            if blocker["state"] == "open" {
                numbers.push(number);
            }
        }
    }
    Ok(numbers)
}
/// The login of each person in an `assignees` array.
pub(crate) fn logins(assignees: &Value) -> Result<Vec<&str>> {
    Ok(assignees
        .as_array()
        .ok_or("missing assignees")?
        .iter()
        .map(|person| person["login"].as_str().ok_or("missing assignee login"))
        .collect::<std::result::Result<_, _>>()?)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn startup_preserves_order_and_rejects_unknown_blockers() -> Result<()> {
        let free = json!({"number":17,"title":"Free","html_url":"https://example.com/17","state":"open","assignees":[],"issue_dependencies_summary":{"blocked_by":0}});
        let mut claimed = free.clone();
        claimed["number"] = json!(11);
        claimed["assignees"] = json!([{"login":"dev"}]);
        let mut blocked = free.clone();
        blocked["number"] = json!(12);
        blocked["issue_dependencies_summary"]["blocked_by"] = json!(2);
        let mut later = free.clone();
        later["number"] = json!(18);
        let rows = open_tickets(&json!([[{"state":"closed"},claimed,free],[blocked,later]]))?;
        assert_eq!(
            rows.iter()
                .map(|r| r["number"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            [11, 17, 12, 18]
        );
        assert_eq!(
            rows.iter()
                .filter(|r| r["frontier"] == true)
                .map(|r| r["number"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            [17, 18]
        );
        assert_eq!(rows[0]["assignees"], json!(["dev"]));
        assert!(open_tickets(&json!([[]]))?.is_empty());
        for unknown in [
            json!({}),
            json!({"blocked_by":null}),
            json!({"blocked_by":-1}),
            json!({"blocked_by":true}),
        ] {
            let mut ticket = free.clone();
            ticket["issue_dependencies_summary"] = unknown;
            assert!(open_tickets(&json!([[ticket]])).is_err());
        }
        Ok(())
    }
}
#[cfg(test)]
mod repo_tests {
    use super::*;
    use serde_json::json;
    fn issue(number: u64, blockers: Value, logins: Value) -> Value {
        let blockers = blockers.as_array().unwrap().clone();
        let logins = logins.as_array().unwrap().clone();
        json!({"number":number,"title":"T","url":"u","parent":{"number":19},
            "assignees":{"totalCount":logins.len(),"nodes":logins},
            "blockedBy":{"totalCount":blockers.len(),"nodes":blockers}})
    }
    fn page(issues: Vec<Value>) -> Value {
        json!({"data":{"repository":{"issues":{"nodes":issues}}}})
    }
    #[test]
    fn rows_sort_by_number_and_keep_open_blockers_only() {
        let blocked = issue(
            9,
            json!([{"number":3,"state":"CLOSED"},{"number":4,"state":"OPEN"}]),
            json!([]),
        );
        let free = issue(2, json!([{"number":3,"state":"CLOSED"}]), json!([]));
        let claimed = issue(5, json!([]), json!([{"login":"dev"}]));
        let rows = repo_tickets(&json!([page(vec![blocked, claimed]), page(vec![free])])).unwrap();
        let summary = rows
            .iter()
            .map(|r| {
                (
                    r["number"].clone(),
                    r["blocked_by"].clone(),
                    r["frontier"].clone(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            [
                (json!(2), json!([]), json!(true)),
                (json!(5), json!([]), json!(false)),
                (json!(9), json!([4]), json!(false))
            ]
        );
        assert_eq!(rows[0]["parent"], 19);
    }
    #[test]
    fn truncated_blockers_are_an_error() {
        let mut truncated = issue(7, json!([{"number":3,"state":"OPEN"}]), json!([]));
        truncated["blockedBy"]["totalCount"] = json!(60);
        let error = repo_tickets(&json!([page(vec![truncated])]))
            .unwrap_err()
            .to_string();
        assert!(error.contains("#7: blockedBy list truncated"), "{error}");
    }
}
#[cfg(test)]
mod map_tests {
    use super::*;
    use serde_json::json;
    fn maps() -> Value {
        json!([
            {"number":1,"state":"CLOSED","title":"Map: old"},
            {"number":30,"state":"OPEN","title":"Map: newer"},
            {"number":20,"state":"OPEN","title":"Map: older"}
        ])
    }
    #[test]
    fn default_map_is_newest_open_map() {
        assert_eq!(select_map(None, || Ok(maps())).unwrap(), 30);
    }
    #[test]
    fn no_open_map_is_an_error_listing_recent_maps() {
        let only_closed = json!([{"number":1,"state":"CLOSED","title":"Map: old"}]);
        let error = select_map(None, || Ok(only_closed))
            .unwrap_err()
            .to_string();
        assert!(error.contains("No open wayfinder:map"), "{error}");
        assert!(error.contains("#1 Map: old (closed)"), "{error}");
    }
    #[test]
    fn explicit_closed_map_is_an_error_naming_open_maps() {
        let closed = json!({"number":1,"state":"CLOSED","title":"Map: old"});
        let error = select_map(Some(&closed), || Ok(maps()))
            .unwrap_err()
            .to_string();
        assert!(error.contains("#1 is closed"), "{error}");
        assert!(error.contains("#30 Map: newer"), "{error}");
        assert!(error.contains("#20 Map: older"), "{error}");
        assert!(!error.contains("Map: old ("), "{error}");
    }
    #[test]
    fn explicit_open_issue_is_used_without_map_label() {
        let spec = json!({"number":19,"state":"OPEN","title":"Spec: x"});
        assert_eq!(
            select_map(Some(&spec), || panic!("fetched the map list")).unwrap(),
            19
        );
    }
    #[test]
    fn blockers_render_open_numbers_only() {
        let pages = json!([
            [{"number":46,"state":"open"},{"number":40,"state":"closed"}],
            [{"number":48,"state":"open"}]
        ]);
        assert_eq!(open_blockers(&pages).unwrap(), [46, 48]);
        assert!(open_blockers(&json!([[{"state":"open"}]])).is_err());
    }
}
