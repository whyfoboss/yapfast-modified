//! In-memory message logger for Discord channels.
//!
//! Retains deleted messages and previous versions of edited messages with
//! strict deterministic bounds on memory, version counts and total bytes.
//! No logged message content is ever persisted to disk.

use std::collections::{HashMap, VecDeque};
use yapfast_discord::message::Message;

/// Maximum number of previous versions retained per message (in addition to current).
pub const MAX_PREVIOUS_VERSIONS: usize = 5;

/// Discord's character limit for standard messages; historical content beyond
/// this is capped and explicitly marked as truncated.
pub const MAX_VERSION_CHARS: usize = 2000;

/// Global hard limit on tracked messages across all channels.
pub const MAX_GLOBAL_MESSAGES: usize = 500;

/// Per-channel hard limit on tracked messages (edited and/or deleted).
pub const MAX_CHANNEL_MESSAGES: usize = 100;

/// Global memory ceiling (16 MB) covering all tracked entries and metadata.
pub const MAX_LOGGER_BYTES: usize = 16 * 1024 * 1024;

/// A single historical version of a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageVersion {
    pub content: String,
    pub timestamp: u64,
    pub edited_at: Option<u64>,
    pub is_observed_time: bool,
    pub truncated: bool,
}

impl MessageVersion {
    pub fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.content.capacity()
    }
}

/// A tracked message with its version history and deletion status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackedEntry {
    pub channel_id: String,
    pub message_id: String,
    pub versions: VecDeque<MessageVersion>,
    pub deleted: bool,
    pub original_known: bool,
}

impl TrackedEntry {
    pub fn new(channel_id: String, message_id: String, original_known: bool) -> Self {
        Self {
            channel_id,
            message_id,
            versions: VecDeque::new(),
            deleted: false,
            original_known,
        }
    }

    pub fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.channel_id.capacity()
            + self.message_id.capacity()
            + self
                .versions
                .iter()
                .map(MessageVersion::estimated_bytes)
                .sum::<usize>()
    }
}

/// In-memory engine managing edit histories and deleted messages.
#[derive(Clone, Debug, Default)]
pub struct MessageLogger {
    pub enabled: bool,
    pub entries: HashMap<String, TrackedEntry>,
    /// Oldest tracked message ID first.
    pub order: VecDeque<String>,
    pub channel_counts: HashMap<String, usize>,
    pub total_bytes: usize,
}

impl MessageLogger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggles the logger. When disabled, purges all in-memory entries and
    /// returns the IDs of any retained deleted messages so the channel can
    /// purge them from view.
    pub fn set_enabled(&mut self, enabled: bool) -> Vec<String> {
        self.enabled = enabled;
        if !enabled { self.clear() } else { Vec::new() }
    }

    /// Wipes all retained logs from memory immediately and returns the IDs of
    /// deleted messages that should now be removed from active channels.
    pub fn clear(&mut self) -> Vec<String> {
        let deleted: Vec<String> = self
            .entries
            .values()
            .filter(|entry| entry.deleted)
            .map(|entry| entry.message_id.clone())
            .collect();
        self.entries.clear();
        self.order.clear();
        self.channel_counts.clear();
        self.total_bytes = 0;
        deleted
    }

    /// Hides a deleted message from the logger and returns its ID if it was retained.
    pub fn hide_deleted(&mut self, message_id: &str) -> Option<String> {
        if let Some(entry) = self.entries.get(message_id)
            && entry.deleted
        {
            let res = self.evict_message(message_id);
            self.recalculate_total_bytes();
            return res;
        }
        None
    }

    /// Removes all currently retained deleted messages from the logger and
    /// returns their IDs so they can be removed from open channels.
    /// Preserves entries that are only edited and not deleted.
    pub fn hide_all_deleted(&mut self) -> Vec<String> {
        let deleted_ids: Vec<String> = self
            .entries
            .values()
            .filter(|entry| entry.deleted)
            .map(|entry| entry.message_id.clone())
            .collect();
        for id in &deleted_ids {
            self.evict_message(id);
        }
        self.recalculate_total_bytes();
        deleted_ids
    }

    pub fn is_deleted(&self, message_id: &str) -> bool {
        self.enabled
            && self
                .entries
                .get(message_id)
                .is_some_and(|entry| entry.deleted)
    }

    pub fn has_history(&self, message_id: &str) -> bool {
        self.enabled
            && self
                .entries
                .get(message_id)
                .is_some_and(|entry| !entry.versions.is_empty())
    }

    pub fn get(&self, message_id: &str) -> Option<&TrackedEntry> {
        if self.enabled {
            self.entries.get(message_id)
        } else {
            None
        }
    }

    /// Records an edit from `old_message` before it is updated to `new_message`.
    /// Returns `(recorded, evicted_deleted_ids)`.
    pub fn record_edit(
        &mut self,
        channel_id: &str,
        old_message: &Message,
        new_message: &Message,
        now_sec: u64,
    ) -> (bool, Vec<String>) {
        if !self.enabled {
            return (false, Vec::new());
        }

        // Ignore updates without content change or with empty old content.
        if new_message.content == old_message.content || old_message.content.trim().is_empty() {
            return (false, Vec::new());
        }

        let message_id = old_message.id.clone();
        let (content, truncated) = if old_message.content.chars().count() > MAX_VERSION_CHARS {
            let capped: String = old_message
                .content
                .chars()
                .take(MAX_VERSION_CHARS)
                .collect();
            (capped, true)
        } else {
            (old_message.content.clone(), false)
        };

        let timestamp = old_message.timestamp();
        let (edited_at, is_observed_time) = if let Some(server_edit) = old_message.edited_timestamp
        {
            (Some(server_edit), false)
        } else if old_message.edited {
            (Some(now_sec), true)
        } else {
            (None, false)
        };

        let version = MessageVersion {
            content,
            timestamp,
            edited_at,
            is_observed_time,
            truncated,
        };

        let is_new = !self.entries.contains_key(&message_id);
        if is_new {
            let entry = TrackedEntry::new(
                channel_id.to_owned(),
                message_id.clone(),
                !old_message.edited,
            );
            self.order.push_back(message_id.clone());
            *self
                .channel_counts
                .entry(channel_id.to_owned())
                .or_insert(0) += 1;
            self.entries.insert(message_id.clone(), entry);
        }

        let entry = self.entries.get_mut(&message_id).expect("entry exists");
        if entry.versions.len() >= MAX_PREVIOUS_VERSIONS {
            entry.versions.pop_front();
        }
        entry.versions.push_back(version);

        self.recalculate_total_bytes();
        let evicted = self.enforce_limits(Some(channel_id));
        (true, evicted)
    }

    /// Records a deletion for `message_id`. Returns `(marked, evicted_deleted_ids)`.
    pub fn record_delete(&mut self, channel_id: &str, message_id: &str) -> (bool, Vec<String>) {
        if !self.enabled {
            return (false, Vec::new());
        }

        if let Some(entry) = self.entries.get_mut(message_id) {
            entry.deleted = true;
            (true, Vec::new())
        } else {
            let mut entry = TrackedEntry::new(channel_id.to_owned(), message_id.to_owned(), true);
            entry.deleted = true;
            self.order.push_back(message_id.to_owned());
            *self
                .channel_counts
                .entry(channel_id.to_owned())
                .or_insert(0) += 1;
            self.entries.insert(message_id.to_owned(), entry);

            self.recalculate_total_bytes();
            let evicted = self.enforce_limits(Some(channel_id));
            (true, evicted)
        }
    }

    fn recalculate_total_bytes(&mut self) {
        self.total_bytes = self
            .entries
            .values()
            .map(TrackedEntry::estimated_bytes)
            .sum::<usize>()
            + (self.order.len() * std::mem::size_of::<String>())
            + (self.channel_counts.len()
                * (std::mem::size_of::<String>() + std::mem::size_of::<usize>()));
    }

    /// Deterministically evicts the oldest message entries until all limits are met.
    /// Returns any evicted message IDs that were marked deleted.
    fn enforce_limits(&mut self, target_channel: Option<&str>) -> Vec<String> {
        let mut evicted_deleted = Vec::new();

        // 1. Per-channel limit check
        if let Some(channel) = target_channel {
            while self.channel_counts.get(channel).copied().unwrap_or(0) > MAX_CHANNEL_MESSAGES {
                if let Some(id) = self.find_oldest_in_channel(channel) {
                    if let Some(deleted_id) = self.evict_message(&id) {
                        evicted_deleted.push(deleted_id);
                    }
                } else {
                    break;
                }
            }
        }

        // 2. Global message count and byte limit check
        while self.entries.len() > MAX_GLOBAL_MESSAGES || self.total_bytes > MAX_LOGGER_BYTES {
            if let Some(id) = self.order.pop_front() {
                if let Some(deleted_id) = self.evict_entry(&id) {
                    evicted_deleted.push(deleted_id);
                }
            } else {
                break;
            }
        }

        self.recalculate_total_bytes();
        evicted_deleted
    }

    fn find_oldest_in_channel(&self, channel_id: &str) -> Option<String> {
        self.order
            .iter()
            .find(|id| {
                self.entries
                    .get(*id)
                    .is_some_and(|entry| entry.channel_id == channel_id)
            })
            .cloned()
    }

    fn evict_message(&mut self, message_id: &str) -> Option<String> {
        self.order.retain(|id| id != message_id);
        self.evict_entry(message_id)
    }

    fn evict_entry(&mut self, message_id: &str) -> Option<String> {
        if let Some(entry) = self.entries.remove(message_id) {
            if let Some(count) = self.channel_counts.get_mut(&entry.channel_id) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.channel_counts.remove(&entry.channel_id);
                }
            }
            if entry.deleted {
                return Some(entry.message_id);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(id: &str, content: &str) -> Message {
        Message {
            id: id.to_owned(),
            channel_id: "chan1".to_owned(),
            author_id: "user1".to_owned(),
            author: "User".to_owned(),
            content: content.to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn record_edit_up_to_five_previous_versions() {
        let mut logger = MessageLogger::new();
        logger.set_enabled(true);

        let mut current = make_msg("1", "v1 original");
        for i in 2..=8 {
            let next = make_msg("1", &format!("v{i} edit"));
            let (recorded, evicted) = logger.record_edit("chan1", &current, &next, 1000 + i);
            assert!(recorded);
            assert!(evicted.is_empty());
            current = next;
        }

        let entry = logger.get("1").expect("tracked");
        // Only 5 previous versions retained
        assert_eq!(entry.versions.len(), MAX_PREVIOUS_VERSIONS);
        // The oldest surviving previous version should be v3 (v1 and v2 were rolled out)
        assert_eq!(entry.versions[0].content, "v3 edit");
        assert_eq!(entry.versions[4].content, "v7 edit");
    }

    #[test]
    fn ignore_duplicate_or_empty_edits() {
        let mut logger = MessageLogger::new();
        logger.set_enabled(true);

        let msg1 = make_msg("1", "hello");
        let msg2 = make_msg("1", "hello");
        let (recorded, _) = logger.record_edit("chan1", &msg1, &msg2, 1000);
        assert!(!recorded);
        assert_eq!(logger.entries.len(), 0);

        let empty = make_msg("2", "   ");
        let next = make_msg("2", "something");
        let (recorded, _) = logger.record_edit("chan1", &empty, &next, 1000);
        assert!(!recorded);
    }

    #[test]
    fn truncation_caps_at_max_version_chars() {
        let mut logger = MessageLogger::new();
        logger.set_enabled(true);

        let long_text = "a".repeat(2500);
        let msg1 = make_msg("1", &long_text);
        let msg2 = make_msg("1", "short edit");

        let (recorded, _) = logger.record_edit("chan1", &msg1, &msg2, 1000);
        assert!(recorded);

        let entry = logger.get("1").unwrap();
        assert_eq!(entry.versions.len(), 1);
        assert!(entry.versions[0].truncated);
        assert_eq!(entry.versions[0].content.chars().count(), MAX_VERSION_CHARS);
    }

    #[test]
    fn channel_limit_evicts_oldest_in_channel() {
        let mut logger = MessageLogger::new();
        logger.set_enabled(true);

        for i in 1..=(MAX_CHANNEL_MESSAGES + 5) {
            let id = format!("{i}");
            let msg1 = make_msg(&id, "initial");
            let msg2 = make_msg(&id, "edited");
            logger.record_edit("chan1", &msg1, &msg2, 1000 + i as u64);
        }

        assert_eq!(logger.entries.len(), MAX_CHANNEL_MESSAGES);
        assert_eq!(
            *logger.channel_counts.get("chan1").unwrap(),
            MAX_CHANNEL_MESSAGES
        );
        // The first 5 messages should have been evicted
        for i in 1..=5 {
            assert!(logger.get(&format!("{i}")).is_none());
        }
        // Messages 6 to 105 should remain
        assert!(logger.get("6").is_some());
        assert!(
            logger
                .get(&format!("{}", MAX_CHANNEL_MESSAGES + 5))
                .is_some()
        );
    }

    #[test]
    fn global_limit_evicts_oldest_globally() {
        let mut logger = MessageLogger::new();
        logger.set_enabled(true);

        // Fill across multiple channels
        for i in 1..=(MAX_GLOBAL_MESSAGES + 10) {
            let chan = format!("chan_{}", i % 20);
            let id = format!("{i}");
            let msg1 = Message {
                id: id.clone(),
                channel_id: chan.clone(),
                content: "v1".to_owned(),
                ..Default::default()
            };
            let msg2 = Message {
                id: id.clone(),
                channel_id: chan.clone(),
                content: "v2".to_owned(),
                ..Default::default()
            };
            logger.record_edit(&chan, &msg1, &msg2, 1000);
        }

        assert_eq!(logger.entries.len(), MAX_GLOBAL_MESSAGES);
        for i in 1..=10 {
            assert!(logger.get(&format!("{i}")).is_none());
        }
        assert!(logger.get("11").is_some());
    }

    #[test]
    fn record_delete_and_clear() {
        let mut logger = MessageLogger::new();
        logger.set_enabled(true);

        // Edit then delete
        let msg1 = make_msg("1", "first");
        let msg2 = make_msg("1", "second");
        logger.record_edit("chan1", &msg1, &msg2, 1000);
        logger.record_delete("chan1", "1");

        assert!(logger.is_deleted("1"));
        assert!(logger.has_history("1"));

        // Delete without edit
        logger.record_delete("chan1", "2");
        assert!(logger.is_deleted("2"));
        assert!(!logger.has_history("2"));

        let cleared = logger.clear();
        assert_eq!(cleared.len(), 2);
        assert!(cleared.contains(&"1".to_string()));
        assert!(cleared.contains(&"2".to_string()));
        assert_eq!(logger.entries.len(), 0);
        assert_eq!(logger.total_bytes, 0);
    }
}
