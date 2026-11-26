//! Discord V3 in-app message reporting menu parsing and submission types.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

/// Default path for submitting an in-app message report.
pub const DEFAULT_POSTBACK_PATH: &str = "/reporting/message";

/// Discord's in-app reporting endpoint timed out with an unknown result.
#[derive(Debug)]
pub struct ReportTimeout;

impl std::fmt::Display for ReportTimeout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Timed out while waiting for Discord to respond. The report may or may not have been received.")
    }
}

impl std::error::Error for ReportTimeout {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportButtonKind {
    Next,
    Submit,
    Done,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportButton {
    pub kind: ReportButtonKind,
    pub target: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportChild {
    pub label: String,
    pub target_node_id: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportElement {
    pub name: String,
    pub kind: String,
    pub should_submit_data: bool,
    pub options: Vec<ReportOption>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportNode {
    pub id: i64,
    pub key: String,
    pub report_type: Option<String>,
    pub header: String,
    pub subheader: Option<String>,
    pub info: Option<String>,
    pub children: Vec<ReportChild>,
    pub elements: Vec<ReportElement>,
    pub button: Option<ReportButton>,
    pub is_multi_select_required: bool,
    pub is_auto_submit: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportMenu {
    pub name: String,
    pub version: String,
    pub variant: String,
    pub postback_path: String,
    pub language: String,
    pub root_node_id: i64,
    pub success_node_id: Option<i64>,
    pub fail_node_id: Option<i64>,
    pub nodes: HashMap<i64, ReportNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportSubmission {
    pub version: String,
    pub variant: String,
    pub language: String,
    pub name: String,
    pub postback_path: String,
    pub breadcrumbs: Vec<i64>,
    pub elements: BTreeMap<String, Vec<String>>,
    pub channel_id: String,
    pub message_id: String,
}

impl ReportSubmission {
    pub fn body(&self) -> Value {
        json!({
            "version": self.version,
            "variant": self.variant,
            "language": self.language,
            "breadcrumbs": self.breadcrumbs,
            "elements": self.elements,
            "name": self.name,
            "channel_id": self.channel_id,
            "message_id": self.message_id,
        })
    }
}

/// Validates and normalizes `postback_url` to a relative `/reporting/...` API path.
pub fn validate_postback_path(raw: Option<&str>) -> String {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return DEFAULT_POSTBACK_PATH.to_owned();
    };
    let lower = raw.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || raw.starts_with("//") {
        return DEFAULT_POSTBACK_PATH.to_owned();
    }
    let path = raw.strip_prefix("/api/v9").unwrap_or(raw);
    if !path.starts_with("/reporting/")
        || path.contains("..")
        || path.contains('?')
        || path.contains('#')
        || path.contains('\\')
        || !path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'_' | b'-'))
    {
        return DEFAULT_POSTBACK_PATH.to_owned();
    }
    path.to_owned()
}

/// Parses Discord's `GET /reporting/menu/message` payload and validates its root node.
pub fn parse_menu(value: &Value) -> Result<ReportMenu> {
    let root_node_id = parse_i64(&value["root_node_id"])
        .context("Discord's report menu is missing a root node.")?;
    let nodes_obj = value["nodes"]
        .as_object()
        .context("Discord's report menu is empty.")?;
    if nodes_obj.is_empty() {
        bail!("Discord's report menu is empty.");
    }

    let mut nodes = HashMap::with_capacity(nodes_obj.len());
    for (key, node_val) in nodes_obj {
        let fallback_id = key.parse::<i64>().ok();
        let Some(node) = parse_node(node_val, fallback_id) else {
            continue;
        };
        nodes.insert(node.id, node);
    }

    // Drop child references that point to absent nodes so navigation cannot hit a dead end.
    let known_ids: std::collections::HashSet<i64> = nodes.keys().copied().collect();
    for node in nodes.values_mut() {
        node.children
            .retain(|child| known_ids.contains(&child.target_node_id));
        if let Some(button) = &mut node.button
            && button
                .target
                .is_some_and(|target| !known_ids.contains(&target))
        {
            button.target = None;
        }
    }

    let Some(root) = nodes.get(&root_node_id) else {
        bail!("Discord's report menu has no valid starting step.");
    };
    let root_usable = !root.children.is_empty()
        || root.is_auto_submit
        || matches!(
            root.button.as_ref().map(|b| b.kind),
            Some(ReportButtonKind::Submit | ReportButtonKind::Next)
        );
    if !root_usable {
        bail!("Discord returned an empty report menu.");
    }

    let name = non_empty_str(&value["name"]).unwrap_or_else(|| "message".to_owned());
    let version = non_empty_str(&value["version"]).unwrap_or_else(|| "1.0".to_owned());
    let variant = non_empty_str(&value["variant"]).unwrap_or_else(|| "1".to_owned());
    let language = non_empty_str(&value["language"]).unwrap_or_else(|| "en".to_owned());
    let postback_path = validate_postback_path(value["postback_url"].as_str());

    Ok(ReportMenu {
        name,
        version,
        variant,
        postback_path,
        language,
        root_node_id,
        success_node_id: parse_i64(&value["success_node_id"]),
        fail_node_id: parse_i64(&value["fail_node_id"]),
        nodes,
    })
}

fn parse_node(value: &Value, fallback_id: Option<i64>) -> Option<ReportNode> {
    let id = parse_i64(&value["id"]).or(fallback_id)?;
    let key = non_empty_str(&value["key"]).unwrap_or_else(|| id.to_string());
    let header = non_empty_str(&value["header"]).unwrap_or_else(|| "Report Message".to_owned());
    let children = value["children"]
        .as_array()
        .map(|items| items.iter().filter_map(parse_child).collect())
        .unwrap_or_default();
    let elements = value["elements"]
        .as_array()
        .map(|items| items.iter().filter_map(parse_element).collect())
        .unwrap_or_default();
    Some(ReportNode {
        id,
        key,
        report_type: non_empty_str(&value["report_type"]),
        header,
        subheader: non_empty_str(&value["subheader"]),
        info: non_empty_str(&value["info"]),
        children,
        elements,
        button: parse_button(&value["button"]),
        is_multi_select_required: value["is_multi_select_required"].as_bool().unwrap_or(false),
        is_auto_submit: value["is_auto_submit"].as_bool().unwrap_or(false),
    })
}

fn parse_child(value: &Value) -> Option<ReportChild> {
    if let Some(arr) = value.as_array() {
        let label = non_empty_str(arr.first()?)?;
        let target_node_id = parse_i64(arr.get(1)?)?;
        return Some(ReportChild {
            label,
            target_node_id,
        });
    }
    let label = non_empty_str(&value["name"])
        .or_else(|| non_empty_str(&value["label"]))
        .or_else(|| non_empty_str(&value["text"]))?;
    let target_node_id = parse_i64(&value["target_node_id"])
        .or_else(|| parse_i64(&value["child_node_id"]))
        .or_else(|| parse_i64(&value["id"]))?;
    Some(ReportChild {
        label,
        target_node_id,
    })
}

fn parse_element(value: &Value) -> Option<ReportElement> {
    let name = non_empty_str(&value["name"])?;
    let kind = non_empty_str(&value["type"]).unwrap_or_default();
    let should_submit_data = value["should_submit_data"].as_bool().unwrap_or(false);
    // Support interactive selection elements (checkbox / multi-select) cleanly; skip unknown non-interactive ones.
    let options = match kind.as_str() {
        "checkbox" | "multi_select" | "select" => parse_options(&value["data"]),
        _ => Vec::new(),
    };
    if options.is_empty() {
        return None;
    }
    Some(ReportElement {
        name,
        kind,
        should_submit_data,
        options,
    })
}

fn parse_options(data: &Value) -> Vec<ReportOption> {
    let items = if let Some(arr) = data.as_array() {
        arr.as_slice()
    } else if let Some(arr) = data["options"].as_array() {
        arr.as_slice()
    } else if let Some(arr) = data["items"].as_array() {
        arr.as_slice()
    } else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            if let Some(arr) = item.as_array() {
                let value = non_empty_str(arr.first()?)?;
                let label = arr.get(1).and_then(non_empty_str).unwrap_or_else(|| value.clone());
                return Some(ReportOption { value, label });
            }
            let value = non_empty_str(&item["name"])
                .or_else(|| non_empty_str(&item["value"]))
                .or_else(|| non_empty_str(&item["id"]))?;
            let label = non_empty_str(&item["label"])
                .or_else(|| non_empty_str(&item["text"]))
                .or_else(|| non_empty_str(&item["title"]))
                .unwrap_or_else(|| value.clone());
            Some(ReportOption { value, label })
        })
        .collect()
}

fn parse_button(value: &Value) -> Option<ReportButton> {
    if value.is_null() {
        return None;
    }
    let kind = match value["type"].as_str()?.trim() {
        "next" => ReportButtonKind::Next,
        "submit" => ReportButtonKind::Submit,
        "done" => ReportButtonKind::Done,
        "cancel" => ReportButtonKind::Cancel,
        _ => return None,
    };
    Some(ReportButton {
        kind,
        target: parse_i64(&value["target"]).or_else(|| parse_i64(&value["target_node_id"])),
    })
}

fn parse_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|v| i64::try_from(v).ok()))
        .or_else(|| value.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
}

fn non_empty_str(value: &Value) -> Option<String> {
    let s = value.as_str()?.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

pub fn is_snowflake(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tuple_and_object_children_and_checkbox_elements() {
        let menu = parse_menu(&json!({
            "name": "message",
            "version": "1.0",
            "variant": "2",
            "postback_url": "/api/v9/reporting/message",
            "language": "en",
            "root_node_id": 1,
            "success_node_id": 900,
            "fail_node_id": 901,
            "nodes": {
                "1": {
                    "id": 1,
                    "key": "root",
                    "header": "What's wrong?",
                    "children": [
                        ["Spam", 10],
                        {"name": "Harassment", "target_node_id": 20},
                        {"label": "Dangling", "child_node_id": 9999}
                    ]
                },
                "10": {
                    "id": 10,
                    "key": "spam",
                    "header": "Spam details",
                    "is_auto_submit": true,
                    "button": {"type": "submit"}
                },
                "20": {
                    "id": 20,
                    "key": "harassment",
                    "header": "Who?",
                    "elements": [
                        {
                            "name": "extra",
                            "type": "checkbox",
                            "should_submit_data": true,
                            "data": [
                                ["repeated", "Repeated messages"],
                                {"value": "dm", "label": "Unwanted DM"}
                            ]
                        },
                        {
                            "name": "unknown_widget",
                            "type": "custom_banner",
                            "should_submit_data": false,
                            "data": []
                        }
                    ],
                    "button": {"type": "submit"},
                    "is_multi_select_required": true
                }
            }
        }))
        .unwrap();

        assert_eq!(menu.postback_path, "/reporting/message");
        assert_eq!(menu.root_node_id, 1);
        let root = &menu.nodes[&1];
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].label, "Spam");
        assert_eq!(root.children[0].target_node_id, 10);
        assert_eq!(root.children[1].label, "Harassment");
        assert_eq!(root.children[1].target_node_id, 20);

        let harassment = &menu.nodes[&20];
        assert!(harassment.is_multi_select_required);
        assert_eq!(harassment.elements.len(), 1);
        assert_eq!(harassment.elements[0].name, "extra");
        assert_eq!(harassment.elements[0].options.len(), 2);
        assert_eq!(harassment.elements[0].options[1].value, "dm");
    }

    #[test]
    fn rejects_external_or_traversing_postback_urls() {
        assert_eq!(validate_postback_path(None), "/reporting/message");
        assert_eq!(
            validate_postback_path(Some("https://evil.example/reporting/message")),
            "/reporting/message"
        );
        assert_eq!(
            validate_postback_path(Some("//evil.example/reporting/message")),
            "/reporting/message"
        );
        assert_eq!(
            validate_postback_path(Some("/users/@me")),
            "/reporting/message"
        );
        assert_eq!(
            validate_postback_path(Some("/reporting/../users/@me")),
            "/reporting/message"
        );
        assert_eq!(
            validate_postback_path(Some("/reporting/message?foo=bar")),
            "/reporting/message"
        );
        assert_eq!(
            validate_postback_path(Some("/api/v9/reporting/v2/message")),
            "/reporting/v2/message"
        );
    }

    #[test]
    fn rejects_empty_or_missing_root_menu() {
        assert!(parse_menu(&json!({})).is_err());
        assert!(parse_menu(&json!({"root_node_id": 1, "nodes": {}})).is_err());
        assert!(
            parse_menu(&json!({
                "root_node_id": 1,
                "nodes": {
                    "1": {"id": 1, "key": "root", "header": "Empty", "children": []}
                }
            }))
            .is_err()
        );
    }
}
