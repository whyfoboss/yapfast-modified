//! Search filter dropdown suggestions and message search helpers.

use chrono::Local;
use slint::SharedString;
use std::collections::{HashMap, HashSet};
use yapfast_core::{
    application::AppState,
    chat::{SearchUserBinding, SearchUserFilter},
    discord,
    yapfast_discord::ready::PRIVATE,
};
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
    id: String,
    name: String,
    username: Option<String>,
    avatar: Option<String>,
    color: Option<u32>,
    external: bool,
    friend: bool,
}

enum SearchScope<'a> {
    Server(&'a str),
    GroupDm(&'a yapfast_core::chat::ChatRoom),
    OneToOneDm(&'a yapfast_core::chat::ChatRoom),
    Unknown,
}

fn active_scope(state: &AppState) -> SearchScope<'_> {
    let Some(room) = state.conversation_that(|can| can.search) else {
        return SearchScope::Unknown;
    };
    if !room.direct
        && let Some((guild, _)) = discord::parse_room(&room.id)
    {
        return SearchScope::Server(guild);
    }
    if room.direct {
        if room.participants.len() > 2 || (room.person.is_none() && room.members > 2) {
            SearchScope::GroupDm(room)
        } else {
            SearchScope::OneToOneDm(room)
        }
    } else {
        SearchScope::Unknown
    }
}

/// Resolves the active trailing `from:` or `mentions:` query prefix while typing.
pub(super) fn active_person_filter_query(text: &str) -> Option<(SearchUserFilter, &str)> {
    if text.is_empty() || text.ends_with(char::is_whitespace) {
        return None;
    }
    let last_token = text.split_whitespace().last()?;
    let (key, value) = last_token.split_once(':')?;
    let filter = match key.to_ascii_lowercase().as_str() {
        "from" => SearchUserFilter::From,
        "mentions" => SearchUserFilter::Mentions,
        _ => return None,
    };
    let clean = value.trim_start_matches('"').trim_start_matches('@').trim();
    Some((filter, clean))
}

/// Keeps only bound user IDs whose `from:` or `mentions:` token still exists verbatim
/// in `query`; editing or removing a token drops its stale ID binding immediately.
pub(super) fn prune_search_bindings(query: &str, bindings: &mut Vec<SearchUserBinding>) {
    if bindings.is_empty() {
        return;
    }
    if let Ok(parsed) = yapfast_core::yapfast_discord::search::Query::parse(query) {
        let has_term = |filter: SearchUserFilter, token: &str| {
            let needle = token.trim().trim_start_matches('@').to_lowercase();
            let list = match filter {
                SearchUserFilter::From => &parsed.from,
                SearchUserFilter::Mentions => &parsed.mentions,
            };
            list.iter()
                .any(|t| t.text.trim_start_matches('@').eq_ignore_ascii_case(&needle))
        };
        bindings.retain(|b| has_term(b.filter, &b.token));
        return;
    }
    let lower = query.to_lowercase();
    bindings.retain(|b| {
        let prefix = match b.filter {
            SearchUserFilter::From => "from:",
            SearchUserFilter::Mentions => "mentions:",
        };
        let norm = b.token.trim().trim_start_matches('@').to_lowercase();
        !norm.is_empty()
            && (lower.contains(&format!("{prefix}{norm}"))
                || lower.contains(&format!("{prefix}@{norm}"))
                || lower.contains(&format!("{prefix}\"{norm}\""))
                || lower.contains(&format!("{prefix}@\"{norm}\"")))
    });
}

/// Extracts the normalized value from a suggestion token such as `from:@alex.new ` -> `alex.new`.
pub(super) fn normalized_binding_token(token: &str, label: &str) -> String {
    let raw = token
        .split_once(':')
        .map_or(label, |(_, value)| value)
        .trim()
        .trim_matches('"')
        .trim_start_matches('@')
        .trim()
        .trim_matches('"')
        .trim();
    raw.to_lowercase()
}

fn resolve_discord_user(
    state: &AppState,
    guild: Option<&str>,
    uid: &str,
) -> (String, Option<String>, Option<String>, Option<u32>) {
    if state.discord_user.as_deref() == Some(uid) {
        let name = state
            .discord_names
            .as_ref()
            .and_then(|names| {
                let n = names.read();
                guild
                    .and_then(|g| n.name(g, uid).map(str::to_owned))
                    .or_else(|| n.user_name(uid).map(str::to_owned))
            })
            .or_else(|| state.account_name.clone())
            .unwrap_or_else(|| "You".to_owned());
        let username = state.account_username.clone().or_else(|| {
            state
                .discord_names
                .as_ref()
                .and_then(|names| names.read().username(uid).map(str::to_owned))
        });
        let avatar = state
            .discord_names
            .as_ref()
            .map(|names| names.read().avatar(guild, uid))
            .or_else(|| state.account_avatar.clone());
        let color = state
            .members
            .iter()
            .flatten()
            .flatten()
            .find(|m| m.id == uid)
            .and_then(|m| m.color);
        return (name, username, avatar, color);
    }

    if let Some(member) = state
        .members
        .iter()
        .flatten()
        .flatten()
        .chain(&state.found_members)
        .find(|m| m.id == uid)
    {
        return (
            member.name.clone(),
            member.username.clone(),
            member.avatar.clone(),
            member.color,
        );
    }

    if let Some(names_lock) = &state.discord_names {
        let names = names_lock.read();
        let display = guild
            .and_then(|g| names.name(g, uid))
            .or_else(|| names.user_name(uid))
            .map(str::to_owned);
        let username = names.username(uid).map(str::to_owned);
        if display.is_some() || username.is_some() {
            let name = display.unwrap_or_else(|| username.clone().unwrap_or_default());
            let avatar = Some(names.avatar(guild, uid));
            return (name, username, avatar, None);
        }
    }

    if let Some(friend) = state.friends.iter().find(|f| f.id == uid) {
        return (friend.name.clone(), None, friend.avatar.clone(), None);
    }

    (uid.to_owned(), None, None, None)
}

fn collect_person_candidates(state: &AppState, filter_value: &str) -> Vec<UserCandidate> {
    let scope = active_scope(state);
    let guild = match scope {
        SearchScope::Server(g) => Some(g),
        _ => None,
    };
    let val_lower = filter_value.to_lowercase();
    let friend_ids: HashSet<&str> = state
        .friends
        .iter()
        .map(|f| f.id.as_str())
        .chain(state.discord_friends.iter().map(String::as_str))
        .collect();

    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut local: Vec<UserCandidate> = Vec::new();

    let push_unique = |list: &mut Vec<UserCandidate>,
                       seen: &mut HashSet<String>,
                       id: String,
                       name: String,
                       username: Option<String>,
                       avatar: Option<String>,
                       color: Option<u32>,
                       external: bool| {
        if id.is_empty() || (name.is_empty() && username.is_none()) || !seen.insert(id.clone()) {
            return;
        }
        let friend = friend_ids.contains(id.as_str());
        list.push(UserCandidate {
            name: if name.is_empty() {
                username.clone().unwrap_or_default()
            } else {
                name
            },
            id,
            username,
            avatar,
            color,
            external,
            friend,
        });
    };

    match scope {
        SearchScope::GroupDm(room) => {
            if !room.participants.is_empty() {
                for uid in &room.participants {
                    let (name, username, avatar, color) =
                        resolve_discord_user(state, Some(PRIVATE), uid);
                    push_unique(
                        &mut local,
                        &mut seen_ids,
                        uid.clone(),
                        name,
                        username,
                        avatar,
                        color,
                        false,
                    );
                }
            } else {
                for uid in state.discord_user.iter().chain(room.person.iter()) {
                    let (name, username, avatar, color) =
                        resolve_discord_user(state, Some(PRIVATE), uid);
                    push_unique(
                        &mut local,
                        &mut seen_ids,
                        uid.clone(),
                        name,
                        username,
                        avatar,
                        color,
                        false,
                    );
                }
            }
        }
        SearchScope::OneToOneDm(room) => {
            for uid in room
                .participants
                .iter()
                .chain(state.discord_user.iter())
                .chain(room.person.iter())
            {
                let (name, username, avatar, color) =
                    resolve_discord_user(state, Some(PRIVATE), uid);
                push_unique(
                    &mut local,
                    &mut seen_ids,
                    uid.clone(),
                    name,
                    username,
                    avatar,
                    color,
                    false,
                );
            }
        }
        SearchScope::Server(guild_id) => {
            for m in state
                .members
                .iter()
                .flatten()
                .flatten()
                .chain(&state.found_members)
            {
                let mut username = m.username.clone();
                if username.is_none()
                    && let Some(names) = &state.discord_names
                {
                    username = names.read().username(&m.id).map(str::to_owned);
                }
                push_unique(
                    &mut local,
                    &mut seen_ids,
                    m.id.clone(),
                    m.name.clone(),
                    username,
                    m.avatar.clone(),
                    m.color,
                    false,
                );
            }
            if let Some(names_lock) = &state.discord_names {
                let ids = names_lock.read().guild_members(guild_id);
                for uid in ids {
                    let (name, username, avatar, color) =
                        resolve_discord_user(state, Some(guild_id), &uid);
                    push_unique(
                        &mut local,
                        &mut seen_ids,
                        uid,
                        name,
                        username,
                        avatar,
                        color,
                        false,
                    );
                }
            }
            if let Some(me) = state.discord_user.as_deref() {
                let (name, username, avatar, color) =
                    resolve_discord_user(state, Some(guild_id), me);
                push_unique(
                    &mut local,
                    &mut seen_ids,
                    me.to_owned(),
                    name,
                    username,
                    avatar,
                    color,
                    false,
                );
            }
        }
        SearchScope::Unknown => {
            for c in state.mention_candidates().into_iter().filter(|c| !c.group && !c.channel) {
                let id = c
                    .username
                    .clone()
                    .unwrap_or_else(|| c.name.clone());
                push_unique(
                    &mut local,
                    &mut seen_ids,
                    id,
                    c.name,
                    c.username,
                    c.avatar,
                    c.color,
                    false,
                );
            }
        }
    }

    let matches_query = |u: &UserCandidate| -> bool {
        if val_lower.is_empty() {
            return true;
        }
        u.name.to_lowercase().contains(&val_lower)
            || u.username
                .as_deref()
                .is_some_and(|un| un.to_lowercase().contains(&val_lower))
    };

    local.retain(matches_query);
    local.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.username.cmp(&b.username))
            .then_with(|| a.id.cmp(&b.id))
    });

    if val_lower.is_empty() {
        local.truncate(20);
        return local;
    }

    // Typed queries include clearly labeled external known users/friends below scope members.
    let mut external: Vec<UserCandidate> = Vec::new();
    for f in &state.friends {
        if seen_ids.contains(&f.id) {
            continue;
        }
        let (_, username, avatar, _) = resolve_discord_user(state, None, &f.id);
        push_unique(
            &mut external,
            &mut seen_ids,
            f.id.clone(),
            f.name.clone(),
            username,
            avatar.or_else(|| f.avatar.clone()),
            None,
            true,
        );
    }
    if let Some(names_lock) = &state.discord_names {
        let extra_ids = names_lock.read().search_people(None, filter_value);
        for uid in extra_ids {
            if seen_ids.contains(&uid) {
                continue;
            }
            let (name, username, avatar, _) = resolve_discord_user(state, None, &uid);
            push_unique(
                &mut external,
                &mut seen_ids,
                uid,
                name,
                username,
                avatar,
                None,
                true,
            );
        }
    }
    let _ = guild;
    external.retain(matches_query);
    external.sort_by(|a, b| {
        b.friend
            .cmp(&a.friend)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.username.cmp(&b.username))
            .then_with(|| a.id.cmp(&b.id))
    });

    local.truncate(15);
    let remaining = 20usize.saturating_sub(local.len()).max(5);
    external.truncate(remaining);
    local.extend(external);
    local
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

    let make_category_item = |idx: usize, (cat, desc): &(&str, &str)| SearchFilterItem {
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
        user_id: SharedString::default(),
        external: false,
    };

    if last_token.is_empty() {
        return CATEGORIES
            .iter()
            .filter(|(cat, _)| in_server || *cat != "in:")
            .enumerate()
            .map(|(idx, pair)| make_category_item(idx, pair))
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
                let external_tag = match active_scope(state) {
                    SearchScope::Server(_) => "Not in server",
                    SearchScope::GroupDm(_) => "Not in group",
                    SearchScope::OneToOneDm(_) => "Not in DM",
                    SearchScope::Unknown => "External",
                };
                let users = collect_person_candidates(state, val_filter);
                let mut seen_external_header = false;
                return users
                    .into_iter()
                    .enumerate()
                    .map(|(idx, u)| {
                        let token = if let Some(un) = &u.username {
                            if un.contains(' ') {
                                format!("{key_lower}:@\"{un}\" ")
                            } else {
                                format!("{key_lower}:@{un} ")
                            }
                        } else if u.name.contains(' ') {
                            format!("{key_lower}:\"{}\" ", u.name)
                        } else {
                            format!("{key_lower}:{} ", u.name)
                        };
                        let detail = match (u.username.as_deref(), u.external, u.friend) {
                            (Some(un), false, _) => format!("@{un}"),
                            (None, false, _) => String::new(),
                            (Some(un), true, true) => {
                                format!("@{un} · Friend · {external_tag}")
                            }
                            (Some(un), true, false) => format!("@{un} · {external_tag}"),
                            (None, true, true) => format!("Friend · {external_tag}"),
                            (None, true, false) => external_tag.to_owned(),
                        };
                        let heading = if idx == 0 {
                            if key_lower == "from" {
                                "FROM USERS".into()
                            } else {
                                "MENTIONS".into()
                            }
                        } else if u.external && !seen_external_header {
                            seen_external_header = true;
                            "OTHER KNOWN USERS".into()
                        } else {
                            SharedString::default()
                        };
                        SearchFilterItem {
                            kind: 1,
                            token: token.into(),
                            label: u.name.as_str().into(),
                            detail: detail.into(),
                            avatar: cached_picture(pictures, state, u.avatar.as_deref()),
                            initial: initial(&u.name),
                            tint: tint(&u.name),
                            color: if role_colors {
                                role_color(u.color)
                            } else {
                                Default::default()
                            },
                            has_color: role_colors && u.color.is_some(),
                            heading,
                            user_id: u.id.into(),
                            external: u.external,
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
                            user_id: SharedString::default(),
                            external: false,
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
                            user_id: SharedString::default(),
                            external: false,
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
                        user_id: SharedString::default(),
                        external: false,
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
                        user_id: SharedString::default(),
                        external: false,
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
        .map(|(idx, pair)| make_category_item(idx, pair))
        .collect();

    if !matching.is_empty() {
        matching
    } else {
        CATEGORIES
            .iter()
            .filter(|(cat, _)| in_server || *cat != "in:")
            .enumerate()
            .map(|(idx, pair)| make_category_item(idx, pair))
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
            let op = ["mentions:", "from:"]
                .into_iter()
                .filter_map(|op| current_query.rfind(op).map(|index| (index, op)))
                .max_by_key(|(index, _)| *index);
            if let Some((idx, op_str)) = op {
                if token.to_ascii_lowercase().starts_with(op_str) {
                    format!("{}{token}", &current_query[..idx])
                } else {
                    let val = if label.contains(' ') {
                        format!("\"{label}\" ")
                    } else {
                        format!("{label} ")
                    };
                    format!("{}{op_str}{val}", &current_query[..idx])
                }
            } else {
                format!("{current_query}{token}")
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
        let lower = result.to_lowercase();
        // Lowercasing can change both the byte length and the number of
        // characters (for example, U+0130). Translate whole character
        // boundaries back to offsets in the original text.
        let boundaries = (!result.is_ascii()).then(|| {
            let mut boundaries = Vec::with_capacity(result.len() + 1);
            let mut offset = 0;
            for (original, ch) in result.char_indices() {
                boundaries.push((offset, original));
                offset += ch.to_lowercase().map(char::len_utf8).sum::<usize>();
            }
            boundaries.push((offset, result.len()));
            boundaries
        });
        let original = |offset| {
            let Some(boundaries) = &boundaries else {
                return Some(offset);
            };
            boundaries
                .binary_search_by_key(&offset, |&(lower, _)| lower)
                .ok()
                .map(|index| boundaries[index].1)
        };
        let matches: Vec<_> = lower
            .match_indices(&term)
            .filter_map(|(start, matched)| {
                Some((original(start)?, original(start + matched.len())?))
            })
            .filter(|&(start, end)| {
                let before = &result[..start];
                let after = &result[end..];
                let in_link = before
                    .rfind("http://")
                    .or_else(|| before.rfind("https://"))
                    .is_some_and(|http_idx| !before[http_idx..].contains(' '));
                let already_bold = before.ends_with("**") && after.starts_with("**");
                !in_link && !already_bold
            })
            .collect();
        for (start, end) in matches.into_iter().rev() {
            result.insert_str(end, "**");
            result.insert_str(start, "**");
        }
    }
    result
}

#[cfg(all(test, feature = "discord"))]
#[test]
fn search_highlight_keeps_unicode_character_boundaries() {
    assert_eq!(highlight_search_terms("İЖ потом", "потом"), "İЖ **потом**");
    assert_eq!(highlight_search_terms("İЖ", "i\u{307}ж"), "**İЖ**");
    assert_eq!(
        highlight_search_terms("https://example.test/İЖ потом", "потом i\u{307}ж"),
        "https://example.test/İЖ **потом**"
    );
}
