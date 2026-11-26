//! Shared V3 in-app Discord message reporting models, target eligibility, and
//! spoiler-safe preview helpers.

use std::collections::{BTreeMap, HashMap};

use crate::{
    chat::{Attachment, ChatMessage, Delivery, plain_snippet},
    discord,
    network::Network,
    spoiler,
};

/// Discord message flag for ephemeral application responses (not stored in history).
pub const EPHEMERAL: u64 = 1 << 6;
/// Discord message flag for deferred / "thinking" application responses.
pub const LOADING: u64 = 1 << 7;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportErrorKind {
    DeletedOrMissing,
    Forbidden,
    RateLimited,
    SessionRevoked,
    TimeoutUnknown,
    Network,
    InvalidMenu,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportError {
    pub kind: ReportErrorKind,
    pub message: String,
}

pub fn is_snowflake(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())
}

pub fn discord_channel_id(room: &str) -> Option<&str> {
    let (_, channel) = discord::parse_room(room)?;
    is_snowflake(channel).then_some(channel)
}

pub fn discord_message_id(item: &str) -> Option<&str> {
    let id = item.strip_prefix("d:")?;
    is_snowflake(id).then_some(id)
}

/// Resolves `(channel_id, message_id, message_link)` for an in-app Discord message report.
///
/// For `THREAD_STARTER_MESSAGE` (`type == 21`), `message.report_target` holds the
/// original parent `(channel_id, message_id)` and must both be valid snowflakes;
/// otherwise reporting is refused instead of falling back to the stub IDs.
pub fn resolve_report_ids<'a>(
    room: &'a str,
    message: &'a ChatMessage,
) -> Option<(&'a str, &'a str, String)> {
    if Network::of(room) != Network::Discord {
        return None;
    }
    let (guild, room_channel) = discord::parse_room(room)?;
    if !is_snowflake(room_channel) {
        return None;
    }
    if let Some(target) = message.report_target.as_deref() {
        if !is_snowflake(&target.0) || !is_snowflake(&target.1) {
            return None;
        }
        return Some((
            target.0.as_str(),
            target.1.as_str(),
            format!("https://discord.com/channels/{guild}/{}/{}", target.0, target.1),
        ));
    }
    let message_id = discord_message_id(&message.item)?;
    Some((room_channel, message_id, message_link(room, message_id)))
}

/// Returns whether a timeline item is eligible for Discord in-app reporting:
/// an already-sent, non-deleted Discord message written by another user with a
/// valid target channel and message snowflake id (excluding notices, ephemeral,
/// and loading states).
pub fn can_report_message(room: &str, message: &ChatMessage) -> bool {
    resolve_report_ids(room, message).is_some()
        && !message.own
        && !message.deleted
        && message.delivery == Delivery::Sent
        && message.notice.is_none()
        && (message.flags & (EPHEMERAL | LOADING)) == 0
}

/// Builds a Discord message permalink from a room id and raw message snowflake.
pub fn message_link(room: &str, message_id: &str) -> String {
    if discord_channel_id(room).is_none() || !is_snowflake(message_id) {
        return String::new();
    }
    let Some(discord_path) = room.strip_prefix("discord:") else {
        return String::new();
    };
    format!(
        "https://discord.com/channels/{}/{message_id}",
        discord_path.replacen(':', "/", 1)
    )
}

/// Returns a bounded, spoiler-redacted plain-text summary of `body` for the report dialog.
pub fn redacted_report_snippet(body: &str, max_chars: usize) -> String {
    let hidden = spoiler::hide(body);
    let snippet = plain_snippet(&hidden);
    let collapsed = snippet.split_whitespace().collect::<Vec<_>>().join(" ");
    match collapsed.char_indices().nth(max_chars) {
        Some((cut, _)) => format!("{}…", collapsed[..cut].trim_end()),
        None => collapsed,
    }
}

/// Summarizes attachments without touching previews or downloading remote media.
pub fn attachment_summary(attachments: &[Attachment]) -> String {
    match attachments.len() {
        0 => String::new(),
        1 => "1 attachment".to_owned(),
        n => format!("{n} attachments"),
    }
}
