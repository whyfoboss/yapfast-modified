//! Search filter dropdown suggestions and message search helpers.

use chrono::Local;
use slint::SharedString;
use std::collections::HashMap;
use yapfast_core::{application::AppState, discord};
use yapfast_ui::SearchFilterItem;

use super::{cached_picture, initial, role_color, tint};

const CATEGORIES: &[(&str, &str)] = &[
    ("from:", "user"),
    ("mentions:", "user"),
    ("has:", "link, embed, or file"),
    ("before:", "date (YYYY-MM-DD)"),
    ("during:", "date (YYYY-MM-DD)"),
    ("after:", "date (YYYY-MM-DD)"),
    ("in:", "channel"),
    ("pinned:", "true or false"),
];

const HAS_OPTIONS: &[(&str, &str)] = &[
    ("image", "Messages with images"),
    ("file", "Messages with file attachments"),
    ("link", "Messages with links"),
    ("video", "Messages with video"),
    ("sound", "Messages with audio files"),
    ("embed", "Messages with embeds"),
    ("poll", "Messages with polls"),
    ("sticker", "Messages with stickers"),
    ("forward", "Forwarded messages"),
];

struct UserCandidate {
    name: String,
    username: Option<String>,
    avatar: Option<String>,
    color: Option<u32>,
}

/// Computes the search filter suggestions based on the current search text.
pub(super) fn compute_search_suggestions(
    state: &AppState,
    text: &str,
    pictures: &mut HashMap<String, slint::Image>,
    role_colors: bool,
) -> Vec<SearchFilterItem> {
    let in_server = match &state.navigation {
        yapfast_core::application::Navigation::Server(id) => {
            id.starts_with("discord:") || (id != "friends" && id != "dms")
        }
        _ => false,
    } || state
        .search_place()
        .as_deref()
        .is_some_and(|p| discord::parse_server(p).is_some());

    let last_token = if text.ends_with(' ') || text.is_empty() {
        ""
    } else {
        text.split_whitespace().last().unwrap_or("")
    };

    if last_token.is_empty() {
        return CATEGORIES
            .iter()
            .filter(|(cat, _)| in_server || *cat != "in:")
            .enumerate()
            .map(|(idx, (cat, desc))| SearchFilterItem {
                kind: 0,
                token: (*cat).into(),
                label: (*cat).into(),
                detail: (*desc).into(),
                avatar: Default::default(),
                initial: Default::default(),
                tint: Default::default(),
                color: Default::default(),
                has_color: false,
                heading: if idx == 0 {
                    "SEARCH OPTIONS".into()
                } else {
                    SharedString::default()
                },
            })
            .collect();
    }

    if let Some((key, value)) = last_token.split_once(':') {
        let key_lower = key.to_ascii_lowercase();
        let val_filter = value
            .trim_matches('"')
            .trim_start_matches('@')
            .trim_start_matches('#');
        let val_lower = val_filter.to_lowercase();

        match key_lower.as_str() {
            "from" | "mentions" => {
                let candidates = state.mention_candidates();
                let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
                let mut users: Vec<UserCandidate> = Vec::new();
                for c in candidates.into_iter().filter(|c| !c.group && !c.channel) {
                    if seen.insert(c.name.clone()) {
                        users.push(UserCandidate {
                            name: c.name,
                            username: c.username,
                            avatar: c.avatar,
                            color: c.color,
                        });
                    }
                }
                for f in &state.friends {
                    if seen.insert(f.name.clone()) {
                        users.push(UserCandidate {
                            name: f.name.clone(),
                            username: None,
                            avatar: f.avatar.clone(),
                            color: None,
                        });
                    }
                }
                return users
                    .into_iter()
                    .filter(|u| {
                        val_lower.is_empty()
                            || u.name.to_lowercase().contains(&val_lower)
                            || u.username
                                .as_deref()
                                .unwrap_or("")
                                .to_lowercase()
                                .contains(&val_lower)
                    })
                    .take(20)
                    .enumerate()
                    .map(|(idx, u)| {
                        let token = if u.name.contains(' ') {
                            format!("{key_lower}:\"{}\" ", u.name)
                        } else {
                            format!("{key_lower}:{} ", u.name)
                        };
                        SearchFilterItem {
                            kind: 1,
                            token: token.into(),
                            label: u.name.as_str().into(),
                            detail: u
                                .username
                                .as_deref()
                                .map(|un| format!("@{un}"))
                                .unwrap_or_default()
                                .into(),
                            avatar: cached_picture(pictures, state, u.avatar.as_deref()),
                            initial: initial(&u.name),
                            tint: tint(&u.name),
                            color: if role_colors {
                                role_color(u.color)
                            } else {
                                Default::default()
                            },
                            has_color: role_colors && u.color.is_some(),
                            heading: if idx == 0 {
                                if key_lower == "from" {
                                    "FROM USERS".into()
                                } else {
                                    "MENTIONS".into()
                                }
                            } else {
                                SharedString::default()
                            },
                        }
                    })
                    .collect();
            }
            "in" => {
                if !in_server {
                    return Vec::new();
                }
                let channels = state.mention_candidates();
                return channels
                    .into_iter()
                    .filter(|c| c.channel && !c.voice)
                    .filter(|c| val_lower.is_empty() || c.name.to_lowercase().contains(&val_lower))
                    .take(20)
                    .enumerate()
                    .map(|(idx, c)| {
                        let token = format!("in:{} ", c.name);
                        SearchFilterItem {
                            kind: 2,
                            token: token.into(),
                            label: c.name.as_str().into(),
                            detail: "channel".into(),
                            avatar: Default::default(),
                            initial: Default::default(),
                            tint: Default::default(),
                            color: Default::default(),
                            has_color: false,
                            heading: if idx == 0 {
                                "CHANNELS".into()
                            } else {
                                SharedString::default()
                            },
                        }
                    })
                    .collect();
            }
            "has" => {
                return HAS_OPTIONS
                    .iter()
                    .filter(|(name, _)| {
                        val_lower.is_empty() || name.to_lowercase().contains(&val_lower)
                    })
                    .enumerate()
                    .map(|(idx, (name, desc))| {
                        let token = format!("has:{name} ");
                        SearchFilterItem {
                            kind: 3,
                            token: token.into(),
                            label: (*name).into(),
                            detail: (*desc).into(),
                            avatar: Default::default(),
                            initial: Default::default(),
                            tint: Default::default(),
                            color: Default::default(),
                            has_color: false,
                            heading: if idx == 0 {
                                "CONTENT TYPES".into()
                            } else {
                                SharedString::default()
                            },
                        }
                    })
                    .collect();
            }
            "before" | "after" | "during" => {
                let today = Local::now().format("%Y-%m-%d").to_string();
                let yesterday = (Local::now().date_naive() - chrono::Duration::days(1))
                    .format("%Y-%m-%d")
                    .to_string();
                let options = [
                    (format!("{key_lower}:{today} "), "Today", today.as_str()),
                    (
                        format!("{key_lower}:{yesterday} "),
                        "Yesterday",
                        yesterday.as_str(),
                    ),
                    (format!("{key_lower}:YYYY-MM-DD"), "Format", "YYYY-MM-DD"),
                ];
                return options
                    .into_iter()
                    .enumerate()
                    .map(|(idx, (tok, lbl, det))| SearchFilterItem {
                        kind: 4,
                        token: tok.into(),
                        label: lbl.into(),
                        detail: det.into(),
                        avatar: Default::default(),
                        initial: Default::default(),
                        tint: Default::default(),
                        color: Default::default(),
                        has_color: false,
                        heading: if idx == 0 {
                            "DATES".into()
                        } else {
                            SharedString::default()
                        },
                    })
                    .collect();
            }
            "pinned" => {
                let options = [
                    ("pinned:true ", "true", "Only pinned messages"),
                    ("pinned:false ", "false", "Exclude pinned messages"),
                ];
                return options
                    .into_iter()
                    .filter(|(_, lbl, _)| val_lower.is_empty() || lbl.starts_with(&val_lower))
                    .enumerate()
                    .map(|(idx, (tok, lbl, det))| SearchFilterItem {
                        kind: 4,
                        token: tok.into(),
                        label: lbl.into(),
                        detail: det.into(),
                        avatar: Default::default(),
                        initial: Default::default(),
                        tint: Default::default(),
                        color: Default::default(),
                        has_color: false,
                        heading: if idx == 0 {
                            "PINNED".into()
                        } else {
                            SharedString::default()
                        },
                    })
                    .collect();
            }
            _ => {}
        }
    }

    // Partial word without colon: match categories or return all
    let token_lower = last_token.to_lowercase();
    let matching: Vec<_> = CATEGORIES
        .iter()
        .filter(|(cat, _)| in_server || *cat != "in:")
        .filter(|(cat, _)| cat.starts_with(&token_lower) || cat.contains(&token_lower))
        .enumerate()
        .map(|(idx, (cat, desc))| SearchFilterItem {
            kind: 0,
            token: (*cat).into(),
            label: (*cat).into(),
            detail: (*desc).into(),
            avatar: Default::default(),
            initial: Default::default(),
            tint: Default::default(),
            color: Default::default(),
            has_color: false,
            heading: if idx == 0 {
                "SEARCH OPTIONS".into()
            } else {
                SharedString::default()
            },
        })
        .collect();

    if !matching.is_empty() {
        matching
    } else {
        CATEGORIES
            .iter()
            .filter(|(cat, _)| in_server || *cat != "in:")
            .enumerate()
            .map(|(idx, (cat, desc))| SearchFilterItem {
                kind: 0,
                token: (*cat).into(),
                label: (*cat).into(),
                detail: (*desc).into(),
                avatar: Default::default(),
                initial: Default::default(),
                tint: Default::default(),
                color: Default::default(),
                has_color: false,
                heading: if idx == 0 {
                    "SEARCH OPTIONS".into()
                } else {
                    SharedString::default()
                },
            })
            .collect()
    }
}

/// Applies a selected search suggestion to the current query string.
pub(super) fn apply_search_suggestion(
    current_query: &str,
    kind: i32,
    token: &str,
    label: &str,
) -> String {
    match kind {
        0 => {
            // Category: e.g. "from:"
            if current_query.trim().is_empty() {
                token.to_string()
            } else if current_query.ends_with(' ') {
                format!("{current_query}{token}")
            } else if let Some(idx) = current_query.rfind(' ') {
                format!("{}{token}", &current_query[..=idx])
            } else {
                token.to_string()
            }
        }
        1 => {
            // Member for from: or mentions:
            let val = if label.contains(' ') {
                format!("\"{label}\" ")
            } else {
                format!("{label} ")
            };
            let op = current_query
                .rfind("mentions:")
                .map(|idx| (idx, "mentions:"))
                .or_else(|| current_query.rfind("from:").map(|idx| (idx, "from:")));
            if let Some((idx, op_str)) = op {
                format!("{}{op_str}{val}", &current_query[..idx])
            } else {
                format!("{current_query}{val}")
            }
        }
        2 => {
            // Channel for in:
            let val = format!("{label} ");
            if let Some(idx) = current_query.rfind("in:") {
                format!("{}in:{val}", &current_query[..idx])
            } else {
                format!("{current_query}in:{val}")
            }
        }
        3 => {
            // Content type for has:
            let val = format!("{label} ");
            if let Some(idx) = current_query.rfind("has:") {
                format!("{}has:{val}", &current_query[..idx])
            } else {
                format!("{current_query}has:{val}")
            }
        }
        _ => {
            // Date / pinned / other
            let last_op = ["before:", "after:", "during:", "pinned:"]
                .iter()
                .filter_map(|op| current_query.rfind(op).map(|idx| (idx, *op)))
                .max_by_key(|(idx, _)| *idx);
            if let Some((idx, _)) = last_op {
                format!("{}{token}", &current_query[..idx])
            } else if current_query.ends_with(' ') || current_query.is_empty() {
                format!("{current_query}{token}")
            } else if let Some(idx) = current_query.rfind(' ') {
                format!("{}{token}", &current_query[..=idx])
            } else {
                token.to_string()
            }
        }
    }
}

/// Highlights search keywords in the message body using bold markdown styling.
pub(super) fn highlight_search_terms(text: &str, query: &str) -> String {
    let Ok(parsed) = yapfast_core::yapfast_discord::search::Query::parse(query) else {
        return text.to_string();
    };
    if parsed.content.trim().is_empty() {
        return text.to_string();
    }
    let mut terms: Vec<String> = Vec::new();
    let mut in_quotes = false;
    let mut current = String::new();
    for ch in parsed.content.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                if !in_quotes && !current.trim().is_empty() {
                    terms.push(std::mem::take(&mut current));
                }
            }
            ch if ch.is_whitespace() && !in_quotes => {
                if !current.trim().is_empty() {
                    terms.push(std::mem::take(&mut current));
                }
            }
            ch => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        terms.push(current);
    }
    let terms: Vec<String> = terms
        .into_iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| t.len() >= 2)
        .collect();
    if terms.is_empty() {
        return text.to_string();
    }

    let mut result = text.to_string();
    for term in terms {
        let mut i = 0;
        while i < result.len() {
            let lower = result[i..].to_lowercase();
            if let Some(pos) = lower.find(&term) {
                let match_start = i + pos;
                let match_end = match_start + term.len();
                let before = &result[..match_start];
                let after = &result[match_end..];
                let in_link = before
                    .rfind("http://")
                    .or_else(|| before.rfind("https://"))
                    .is_some_and(|http_idx| !before[http_idx..].contains(' '));
                let already_bold = before.ends_with("**") && after.starts_with("**");
                if !in_link && !already_bold {
                    let matched_slice = &result[match_start..match_end];
                    let replacement = format!("**{matched_slice}**");
                    result = format!("{}{replacement}{}", before, after);
                    i = match_start + replacement.len();
                } else {
                    i = match_end;
                }
            } else {
                break;
            }
        }
    }
    result
}
