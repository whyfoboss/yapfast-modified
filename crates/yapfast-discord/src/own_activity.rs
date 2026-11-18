//! The game this account shares: what a game says over Rich Presence, or
//! else the game detection finds. The account's `show_current_game` setting
//! turns both on and off; the custom status stays apart from them.

use crate::{api::Api, service::Snapshot};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::watch;

type Activity = Option<Arc<Value>>;

/// Correlates activity diagnostics within one process without logging track names.
pub fn trace_track(title: &str) -> u64 {
    use std::{hash::{BuildHasher, RandomState}, sync::OnceLock};
    static KEY: OnceLock<RandomState> = OnceLock::new();
    KEY.get_or_init(RandomState::new).hash_one(title)
}

/// Evicts a permanently failed Discord media-proxy cover from the RPC external-asset cache.
pub fn invalidate_external_asset(proxy_url: &str) {
    #[cfg(any(target_os = "linux", windows, test))]
    crate::rpc::proto::invalidate_external_asset(proxy_url);
    #[cfg(not(any(target_os = "linux", windows, test)))]
    let _ = proxy_url;
}

pub(crate) fn subscribe_late_soundcloud_artwork() -> watch::Receiver<Activity> {
    #[cfg(any(target_os = "linux", windows, test))]
    {
        crate::rpc::proto::subscribe_late_soundcloud_artwork()
    }
    #[cfg(not(any(target_os = "linux", windows, test)))]
    {
        watch::channel(None).1
    }
}

pub(crate) fn subscribe_soundcloud_rpc_clients() -> watch::Receiver<usize> {
    #[cfg(any(target_os = "linux", windows, test))]
    {
        crate::rpc::proto::subscribe_soundcloud_rpc_clients()
    }
    #[cfg(not(any(target_os = "linux", windows, test)))]
    {
        watch::channel(0).1
    }
}

pub(crate) fn eligible(activity: &Value) -> bool {
    crate::activity::activity(activity).is_some_and(|parsed| {
        parsed.eligible_for_own_presence()
            && !(matches!(
                parsed.kind,
                crate::activity::Kind::Playing | crate::activity::Kind::Listening
            ) && [
                "/state_url",
                "/details_url",
                "/metadata/button_urls/0",
                "/metadata/button_urls/1",
            ]
            .into_iter()
            .filter_map(|path| activity.pointer(path).and_then(Value::as_str))
            .any(crate::activity::video_url))
    })
}

/// The native timeline for a track that may also be shared over music RPC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MusicPlayback {
    pub soundcloud_source: bool,
    pub title: String,
    pub artist: String,
    pub duration_ms: u64,
    pub position_ms: u64,
    pub start_ms: Option<u64>,
}

impl MusicPlayback {
    /// A running clock needs a new anchor only after a meaningful correction.
    pub fn same_published_timeline(&self, previous: &Self) -> bool {
        match (self.start_ms, previous.start_ms) {
            (Some(start), Some(old_start)) => {
                self.soundcloud_source == previous.soundcloud_source
                    && self.title == previous.title
                    && self.artist == previous.artist
                    && self.duration_ms == previous.duration_ms
                    && start.abs_diff(old_start) < 750
            }
            _ => self == previous,
        }
    }

    pub fn matches(&self, rpc: &crate::activity::Activity) -> bool {
        use crate::activity::{Kind, music_integration};

        if !matches!(rpc.kind, Kind::Playing | Kind::Listening) || !music_integration(&rpc.name) {
            return false;
        }
        let normalize = |text: &str| {
            text.trim_matches('\u{2800}')
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        };
        let Some(title) = rpc.details.as_deref().map(normalize) else {
            return false;
        };
        let track_title = normalize(&self.title);
        let duration_matches = rpc.start.zip(rpc.end).is_some_and(|(start, end)| {
            end > start && self.duration_ms > 0 && (end - start).abs_diff(self.duration_ms) <= 2_000
        });
        let allow_prefix = duration_matches || (rpc.start.is_none() && rpc.end.is_none());
        let matches_candidate = |candidate: &str| {
            !title.is_empty()
                && (title == candidate
                    || (allow_prefix
                        && title
                            .strip_suffix('…')
                            .or_else(|| title.strip_suffix("..."))
                            .is_some_and(|prefix| {
                                let prefix = prefix.trim_end();
                                prefix.chars().count() >= 12 && candidate.starts_with(prefix)
                            })))
        };
        let is_soundcloud = rpc.name.eq_ignore_ascii_case("SoundCloud");
        let split_match = split_artist_track(&track_title).is_some_and(|(parsed_artist, parsed_track)| {
            matches_candidate(parsed_track)
                && ((self.soundcloud_source && is_soundcloud)
                    || rpc
                        .state
                        .as_deref()
                        .is_some_and(|state| artists_match(parsed_artist, &normalize(state))))
        });
        if !matches_candidate(&track_title) && !split_match {
            return false;
        }
        if self.soundcloud_source && is_soundcloud {
            return true;
        }
        if split_match {
            return true;
        }
        let artist = normalize(&self.artist);
        if is_soundcloud && is_paused_state(rpc.state.as_deref()) {
            return !artist.is_empty() || duration_matches;
        }
        if artist.is_empty() {
            return is_soundcloud && duration_matches;
        }
        rpc.state.as_deref().is_some_and(|state| {
            let rpc_artist = normalize(state);
            artists_match(&artist, &rpc_artist)
                || (is_soundcloud
                    && !rpc_artist.is_empty()
                    && title_credits_artist(&track_title, &artist))
        })
    }
}

fn artist_tokens(normalized: &str) -> Vec<&str> {
    let mut parts = vec![normalized];
    for sep in [
        ",",
        ";",
        " & ",
        " + ",
        " × ",
        " x ",
        " feat. ",
        " feat ",
        " ft. ",
        " ft ",
        " w/ ",
        " / ",
        " · ",
        " • ",
    ] {
        parts = parts
            .into_iter()
            .flat_map(|part| part.split(sep))
            .collect();
    }
    parts
        .into_iter()
        .map(|part| {
            part.trim_matches(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']'))
        })
        .filter(|part| !part.is_empty())
        .collect()
}

fn artists_match(left: &str, right: &str) -> bool {
    if left.is_empty() || right.is_empty() {
        return false;
    }
    if left == right {
        return true;
    }
    let left_tokens = artist_tokens(left);
    let right_tokens = artist_tokens(right);
    left_tokens
        .iter()
        .any(|l| right_tokens.iter().any(|r| l == r))
}

fn title_credits_artist(normalized_title: &str, normalized_artist: &str) -> bool {
    if normalized_title.is_empty() || normalized_artist.is_empty() {
        return false;
    }
    let artist_parts = artist_tokens(normalized_artist);
    for sep in [
        " + ",
        " & ",
        " feat. ",
        " feat ",
        " ft. ",
        " ft ",
        " w/ ",
        " prod. ",
        " prod ",
    ] {
        if normalized_title.contains(sep) {
            for segment in normalized_title.split(sep).skip(1) {
                let seg_tokens = artist_tokens(segment);
                if seg_tokens
                    .iter()
                    .any(|s| artist_parts.iter().any(|a| s == a))
                {
                    return true;
                }
            }
        }
    }
    false
}

fn split_artist_track(title: &str) -> Option<(&str, &str)> {
    for sep in [" - ", " – ", " — ", " ― ", " -- "] {
        if let Some((artist, track)) = title.split_once(sep) {
            let (artist, track) = (artist.trim(), track.trim());
            if !artist.is_empty() && !track.is_empty() {
                return Some((artist, track));
            }
        }
    }
    None
}

/// Treats minor running-clock anchor jitter (< 2s) on an otherwise identical
/// Listening activity as unchanged so loop-detector or SMTC/RPC rounding ticks
/// do not exhaust Discord's 5-per-20s presence rate limit.
pub(crate) fn same_effective_presence(previous: &Value, next: &Value) -> bool {
    if previous == next {
        return true;
    }
    if previous["status"] != next["status"]
        || previous["afk"] != next["afk"]
        || previous["since"] != next["since"]
    {
        return false;
    }
    let (Some(old_list), Some(new_list)) = (
        previous["activities"].as_array(),
        next["activities"].as_array(),
    ) else {
        return false;
    };
    old_list.len() == new_list.len()
        && old_list.iter().zip(new_list).all(|(old, new)| {
            if old == new {
                return true;
            }
            if old["type"] != 2 || new["type"] != 2 {
                return false;
            }
            let same_fields = [
                "name",
                "application_id",
                "details",
                "state",
                "assets",
                "buttons",
                "metadata",
                "_artwork_pending",
            ]
            .into_iter()
            .all(|field| old.get(field) == new.get(field));
            if !same_fields {
                return false;
            }
            let (Some(old_start), Some(new_start)) = (
                old["timestamps"]["start"].as_u64(),
                new["timestamps"]["start"].as_u64(),
            ) else {
                return false;
            };
            let end_same = match (
                old["timestamps"]["end"].as_u64(),
                new["timestamps"]["end"].as_u64(),
            ) {
                (Some(a), Some(b)) => a.abs_diff(b) < 2_500,
                (None, None) => true,
                _ => false,
            };
            old_start.abs_diff(new_start) < 2_000 && end_same
        })
}

/// The game the account shows it plays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Playing {
    pub name: String,
    /// When it started, in Unix milliseconds, when the game says.
    pub started_ms: Option<u64>,
    pub activity: Option<Arc<crate::activity::Activity>>,
}

impl Playing {
    /// The game an activity names, if it names one.
    pub(crate) fn of(activity: &Value) -> Option<Self> {
        if !eligible(activity) {
            return None;
        }
        let name = activity["name"].as_str()?.trim();
        (!name.is_empty()).then(|| Self {
            name: name.to_owned(),
            started_ms: activity
                .pointer("/timestamps/start")
                .and_then(Value::as_u64),
            activity: crate::activity::activity(activity).map(Arc::new),
        })
    }
}

/// Runs detection and Rich Presence while `shown` holds, and gives the
/// activity they make, until the receiver goes.
pub(crate) fn start(
    api: Arc<Api>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
    mut shown: watch::Receiver<bool>,
) -> watch::Receiver<Activity> {
    #[cfg(not(any(target_os = "linux", windows)))]
    let _ = snapshots;
    let (activity, receiver) = watch::channel(None);
    tokio::spawn(async move {
        loop {
            if *shown.borrow_and_update() {
                let detected = crate::game_detection::start(api.clone());
                #[cfg(any(target_os = "linux", windows))]
                let rpc = crate::rpc::start(api.clone(), snapshots.clone());
                #[cfg(not(any(target_os = "linux", windows)))]
                let rpc = watch::channel(None).1;
                tokio::select! {
                    () = combine(rpc, detected, crate::game_detection::custom_games::subscribe_disabled(), activity.clone()) => return,
                    () = hidden(&mut shown) => {}
                }
                activity.send_replace(None);
            } else {
                tokio::select! {
                    () = activity.closed() => return,
                    result = shown.changed() => if result.is_err() { return; },
                }
            }
        }
    });
    receiver
}

/// Waits for sharing to stop. Once the session is gone, the activity's
/// receiver goes with it, which ends [`combine`] instead.
async fn hidden(shown: &mut watch::Receiver<bool>) {
    while shown.changed().await.is_ok() {
        if !*shown.borrow_and_update() {
            return;
        }
    }
    std::future::pending().await
}

async fn combine(
    mut rpc: watch::Receiver<Activity>,
    mut detected: watch::Receiver<Activity>,
    mut disabled: watch::Receiver<Vec<String>>,
    shown: watch::Sender<Activity>,
) {
    let mut rpc_open = true;
    let mut detection_open = true;
    loop {
        let enabled = |activity: &Arc<Value>| {
            activity["name"].as_str().is_some_and(|name| {
                !crate::game_detection::custom_games::is_disabled(&disabled.borrow(), name)
            })
        };
        let rpc_activity = rpc_open
            .then(|| rpc.borrow().clone())
            .flatten()
            .filter(|activity| eligible(activity) && enabled(activity));
        let game = detection_open
            .then(|| detected.borrow().clone())
            .flatten()
            .filter(enabled);
        let next = rpc_activity
            .as_ref()
            .filter(|activity| {
                activity["name"]
                    .as_str()
                    .is_some_and(|name| !crate::activity::music_integration(name))
            })
            .cloned()
            .or(game)
            .or(rpc_activity);
        shown.send_if_modified(|old| {
            if *old == next {
                false
            } else {
                *old = next;
                true
            }
        });
        tokio::select! {
            () = shown.closed() => return,
            result = rpc.changed(), if rpc_open => rpc_open = result.is_ok(),
            result = detected.changed(), if detection_open => detection_open = result.is_ok(),
            _ = disabled.changed() => {}
        }
    }
}

/// Formats the visible SoundCloud pause status (`Paused on m:ss` or `Paused on h:mm:ss`,
/// or `Paused` when no track timeline/position is available).
pub fn format_pause_status(position_ms: u64, duration_ms: u64) -> String {
    if position_ms == 0 && duration_ms == 0 {
        return "Paused".into();
    }
    let clamped = if duration_ms > 0 {
        position_ms.min(duration_ms)
    } else {
        position_ms
    };
    let secs = clamped / 1000;
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if hours > 0 {
        format!("Paused on {hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("Paused on {minutes}:{seconds:02}")
    }
}

pub(crate) fn is_paused_state(state: Option<&str>) -> bool {
    state.is_some_and(|s| s == "Paused" || s.starts_with("Paused on ") || s.starts_with("Paused at "))
}

/// Holds the track through SoundCloud's RPC clear or idle activity on pause.
pub(crate) fn soundcloud_on_pause(
    current: Activity,
    previous: &mut Activity,
    music: Option<&MusicPlayback>,
) -> Activity {
    if current.as_ref().is_some_and(|activity| {
        activity["name"] == "SoundCloud" && activity["type"] == 2
    }) {
        *previous = current.clone();
        return current;
    }
    if current.as_ref().is_none_or(|activity| activity["name"] == "SoundCloud")
        && let Some(music) = music
        && let Some(track) = previous.as_ref()
        && crate::activity::activity(track).is_some_and(|rpc| music.matches(&rpc))
    {
        // The RPC clear may arrive before the Windows playback event; keep `previous`
        // intact while `music` still matches so the upcoming SMTC pause retains title & cover.
        return if music.start_ms.is_none() {
            previous.clone()
        } else {
            current
        };
    }
    *previous = None;
    current
}

pub(crate) fn merge(presence: &mut Value, activity: Option<&Value>, music: Option<&MusicPlayback>) {
    merge_with_fallback(presence, activity, music, true);
}

pub(crate) fn merge_with_fallback(
    presence: &mut Value,
    activity: Option<&Value>,
    music: Option<&MusicPlayback>,
    allow_stale_replace: bool,
) {
    if let Some(activities) = presence["activities"].as_array_mut() {
        activities.retain(|value| value["type"] == 4);
        if let Some(activity) = activity.filter(|activity| eligible(activity)) {
            let mut activity = activity.clone();
            if let Some(music) = music {
                let matched =
                    crate::activity::activity(&activity).is_some_and(|rpc| music.matches(&rpc));
                let replace_stale_track = allow_stale_replace
                    && !matched
                    && music.soundcloud_source
                    && !music.title.trim().is_empty()
                    && activity["name"]
                        .as_str()
                        .is_some_and(|name| name.eq_ignore_ascii_case("SoundCloud"));
                if replace_stale_track {
                    // Keep the integration identity, but discard assets and links for the old track.
                    activity = serde_json::json!({
                        "type": 2,
                        "name": activity["name"],
                        "application_id": activity["application_id"],
                        "details": music.title.chars().take(128).collect::<String>(),
                        "state": music.artist.chars().take(128).collect::<String>(),
                    });
                }
                if matched || replace_stale_track {
                    if let Some(start) = music.start_ms {
                        if replace_stale_track || music.duration_ms > 0 {
                            activity["timestamps"] = serde_json::json!({
                                "start": start, "end": start.saturating_add(music.duration_ms)
                            });
                            if music.duration_ms == 0 {
                                activity["timestamps"] = serde_json::json!({"start": start});
                            }
                        }
                    } else if let Some(activity) = activity.as_object_mut() {
                        // Discord timestamps run against the viewer's clock.
                        activity.remove("timestamps");
                        if activity
                            .get("name")
                            .and_then(Value::as_str)
                            .is_some_and(|name| name.eq_ignore_ascii_case("SoundCloud"))
                        {
                            activity.insert(
                                "state".into(),
                                Value::String(format_pause_status(
                                    music.position_ms,
                                    music.duration_ms,
                                )),
                            );
                        } else {
                            let position = if music.duration_ms > 0 {
                                music.position_ms.min(music.duration_ms)
                            } else {
                                music.position_ms
                            } / 1000;
                            let artist = activity
                                .get("state")
                                .and_then(Value::as_str)
                                .unwrap_or(&music.artist);
                            activity.insert(
                                "state".into(),
                                Value::String(format!(
                                    "Paused at {}:{:02} · {}",
                                    position / 60,
                                    position % 60,
                                    artist
                                )),
                            );
                        }
                    }
                }
            }
            activities.push(activity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn running_clock_jitter_does_not_hide_seek_pause_or_track_change() {
        let original = MusicPlayback {
            soundcloud_source: true,
            title: "Track".into(),
            artist: "Artist".into(),
            duration_ms: 72_000,
            position_ms: 0,
            start_ms: Some(10_000),
        };
        let mut next = original.clone();
        next.start_ms = Some(10_200);
        assert!(next.same_published_timeline(&original));
        next.start_ms = Some(12_000);
        assert!(!next.same_published_timeline(&original));
        next.start_ms = None;
        assert!(!next.same_published_timeline(&original));
        next.start_ms = original.start_ms;
        next.title = "Next track".into();
        assert!(!next.same_published_timeline(&original));
    }

    #[test]
    fn soundcloud_shortened_title_keeps_cover_only_with_matching_duration() {
        let track = MusicPlayback {
            soundcloud_source: true,
            title: "A long track title with a suffix".into(),
            artist: "Artist".into(),
            duration_ms: 72_000,
            position_ms: 0,
            start_ms: Some(1000),
        };
        let mut rpc = json!({
            "type": 2, "name": "SoundCloud", "application_id": "42",
            "details": "A long track title…", "state": "Artist, Collaborator",
            "assets": {"large_image": "123"},
            "timestamps": {"start":1000,"end":73000}
        });
        let mut presence = json!({"activities":[]});
        merge(&mut presence, Some(&rpc), Some(&track));
        assert_eq!(presence["activities"][0]["assets"]["large_image"], "123");

        rpc["timestamps"]["end"] = json!(80_000);
        merge(&mut presence, Some(&rpc), Some(&track));
        assert!(presence["activities"][0].get("assets").is_none());
    }

    #[test]
    fn paused_music_publishes_fixed_position_and_resume_restores_timeline() {
        let activity = json!({
            "type": 2, "name": "SoundCloud", "details": "Track", "state": "Artist, Collaborator",
            "timestamps": {"start": 1000, "end": 124000}
        });
        let mut playback = MusicPlayback {
            soundcloud_source: true,
            title: "Track".into(),
            artist: "Artist".into(),
            duration_ms: 123_000,
            position_ms: 109_000,
            start_ms: None,
        };
        let mut presence = json!({"activities":[]});
        merge(&mut presence, Some(&activity), Some(&playback));
        let paused = &presence["activities"][0];
        assert!(paused.get("timestamps").is_none());
        assert_eq!(paused["details"], "Track");
        assert_eq!(paused["state"], "Paused on 1:49");
        assert_eq!(format_pause_status(48_000, 72_000), "Paused on 0:48");
        assert_eq!(format_pause_status(3_665_000, 4_000_000), "Paused on 1:01:05");
        assert_eq!(format_pause_status(0, 0), "Paused");

        playback.start_ms = Some(2000);
        merge(&mut presence, Some(&activity), Some(&playback));
        let resumed = &presence["activities"][0];
        assert_eq!(resumed["timestamps"], json!({"start":2000,"end":125000}));
        assert_eq!(resumed["state"], "Artist, Collaborator");

        playback.title = "Next track".into();
        playback.artist = "Next artist".into();
        let mut stale = activity;
        stale["assets"] = json!({"large_image": "123"});
        merge(&mut presence, Some(&stale), Some(&playback));
        let next = &presence["activities"][0];
        assert_eq!(next["details"], "Next track");
        assert_eq!(next["state"], "Next artist");
        assert!(next.get("assets").is_none());
        assert_eq!(next["timestamps"], json!({"start":2000,"end":125000}));
    }

    #[test]
    fn own_presence_excludes_video_and_ambiguous_media() {
        for activity in [
            json!({"type":3,"name":"Video","details":"Title"}),
            json!({"type":0,"name":"YouTube","details":"Title","state":"Channel"}),
            json!({"type":0,"name":"Third-party RPC","details_url":"https://youtu.be/fixture"}),
            json!({"type":2,"name":"Browser Media","details":"Title"}),
            json!({"type":0,"name":"Chrome","assets":{"large_image":"123"}}),
            json!({"type":0,"name":"Chrome"}),
        ] {
            assert!(!eligible(&activity));
            assert!(Playing::of(&activity).is_none());
            let mut presence = json!({"activities":[]});
            merge(&mut presence, Some(&activity), None);
            assert_eq!(presence["activities"], json!([]));
        }
        for activity in [
            json!({"type":0,"name":"ARC Raiders"}),
            json!({"type":0,"name":"Game","details":"In match"}),
            json!({"type":2,"name":"SoundCloud","details":"Track"}),
        ] {
            assert!(eligible(&activity));
        }
    }

    #[tokio::test]
    async fn video_and_music_rpc_do_not_mask_a_detected_game() {
        let (rpc, rpc_rx) = watch::channel(None);
        let (detected, detected_rx) = watch::channel(None);
        let (shown, mut receiver) = watch::channel(None);
        let (_disabled, rules) = watch::channel(Vec::new());
        let worker = tokio::spawn(combine(rpc_rx, detected_rx, rules, shown));
        let soundcloud = Arc::new(json!({"type":2,"name":"SoundCloud","details":"Track"}));
        rpc.send_replace(Some(soundcloud.clone()));
        receiver.changed().await.unwrap();
        assert_eq!(receiver.borrow_and_update().as_ref(), Some(&soundcloud));
        let subnautica = Arc::new(json!({"type":0,"name":"Subnautica"}));
        detected.send_replace(Some(subnautica.clone()));
        receiver.changed().await.unwrap();
        assert_eq!(receiver.borrow_and_update().as_ref(), Some(&subnautica));
        rpc.send_replace(Some(Arc::new(
            json!({"type":0,"name":"YouTube","details":"Title"}),
        )));
        detected.send_replace(None);
        receiver.changed().await.unwrap();
        assert!(receiver.borrow_and_update().is_none());
        drop(receiver);
        worker.await.unwrap();
    }

    #[tokio::test]
    async fn rpc_overrides_detection_and_disconnect_restores_it() {
        let (rpc, rpc_rx) = watch::channel(None);
        let (detected, detected_rx) = watch::channel(None);
        let (shown, mut receiver) = watch::channel(None);
        let (disabled, rules) = watch::channel(Vec::new());
        let worker = tokio::spawn(combine(rpc_rx, detected_rx, rules, shown));
        let game = Arc::new(json!({"type":0,"name":"Game"}));
        detected.send_replace(Some(game.clone()));
        receiver.changed().await.unwrap();
        assert_eq!(receiver.borrow_and_update().as_ref(), Some(&game));
        let rich = Arc::new(json!({"type":0,"name":"Game","details":"In match"}));
        rpc.send_replace(Some(rich.clone()));
        receiver.changed().await.unwrap();
        assert_eq!(receiver.borrow_and_update().as_ref(), Some(&rich));
        disabled.send_replace(vec!["game".into()]);
        receiver.changed().await.unwrap();
        assert!(receiver.borrow_and_update().is_none());
        disabled.send_replace(Vec::new());
        receiver.changed().await.unwrap();
        assert_eq!(receiver.borrow_and_update().as_ref(), Some(&rich));
        // A source that dies without clearing must not leave a stale activity.
        drop(rpc);
        receiver.changed().await.unwrap();
        assert_eq!(receiver.borrow_and_update().as_ref(), Some(&game));
        detected.send_replace(None);
        receiver.changed().await.unwrap();
        assert!(receiver.borrow_and_update().is_none());
        drop(receiver);
        worker.await.unwrap();
        assert!(detected.is_closed());
    }

    #[test]
    fn names_the_game_an_activity_plays() {
        let playing = Playing::of(&json!({"type":0,"name":" Game ","timestamps":{"start":5},
            "application_id":"42", "assets":{"large_image":"99"}}))
        .unwrap();
        assert_eq!(playing.name, "Game");
        assert_eq!(playing.started_ms, Some(5));
        let activity = playing.activity.unwrap();
        let art = activity.large_image.as_deref().unwrap();
        assert!(art.ends_with("/app-assets/42/99.png?size=160"));
        assert_eq!(Playing::of(&json!({"type":0,"name":" "})), None);
    }

    #[test]
    fn soundcloud_split_artist_dash_title_and_braille_padding_keep_rpc_cover() {
        let smtc = MusicPlayback {
            soundcloud_source: true,
            title: "Burial - Archangel".into(),
            artist: "Hyperdub Records".into(),
            duration_ms: 238_000,
            position_ms: 0,
            start_ms: Some(1_800_000_000_000),
        };
        let rpc = json!({
            "type": 2,
            "name": "SoundCloud",
            "application_id": "1090770350251458592",
            "details": "Archangel",
            "state": "Burial",
            "assets": {"large_image": "mp:external/sig/https/i1.sndcdn.com/archangel.jpg"},
            "buttons": ["▶️ Listen on SoundCloud"],
            "metadata": {"button_urls": ["https://soundcloud.com/burial/archangel"]}
        });
        let mut presence = json!({"activities": []});
        merge(&mut presence, Some(&rpc), Some(&smtc));
        let card = &presence["activities"][0];
        assert_eq!(card["details"], "Archangel");
        assert_eq!(card["state"], "Burial");
        assert_eq!(
            card["assets"]["large_image"],
            "mp:external/sig/https/i1.sndcdn.com/archangel.jpg"
        );
        assert_eq!(
            card["timestamps"],
            json!({"start": 1_800_000_000_000_u64, "end": 1_800_000_238_000_u64})
        );
        assert_eq!(card["buttons"], json!(["▶️ Listen on SoundCloud"]));

        // Short 1-char title/artist padded with Braille blanks (`\u{2800}`) in presenceService.ts:
        let short_smtc = MusicPlayback {
            soundcloud_source: true,
            title: "X".into(),
            artist: "Y".into(),
            duration_ms: 90_000,
            position_ms: 0,
            start_ms: Some(1_800_000_000_000),
        };
        let short_rpc = json!({
            "type": 2,
            "name": "SoundCloud",
            "application_id": "1090770350251458592",
            "details": "X\u{2800}\u{2800}",
            "state": "Y\u{2800}\u{2800}",
            "assets": {"large_image": "mp:external/sig/https/i1.sndcdn.com/xy.jpg"}
        });
        let mut short_presence = json!({"activities": []});
        merge(&mut short_presence, Some(&short_rpc), Some(&short_smtc));
        assert_eq!(
            short_presence["activities"][0]["assets"]["large_image"],
            "mp:external/sig/https/i1.sndcdn.com/xy.jpg"
        );
    }

    #[test]
    fn effective_presence_deduplicates_loop_clock_jitter_but_keeps_seek_pause_and_cover() {
        let base = json!({
            "status": "online",
            "activities": [{
                "type": 2,
                "name": "SoundCloud",
                "application_id": "1090770350251458592",
                "details": "Archangel",
                "state": "Burial",
                "timestamps": {"start": 1_800_000_000_000_u64, "end": 1_800_000_238_000_u64}
            }]
        });
        let mut jittered = base.clone();
        jittered["activities"][0]["timestamps"] =
            json!({"start": 1_800_000_000_850_u64, "end": 1_800_000_238_850_u64});
        assert!(same_effective_presence(&base, &jittered));

        let mut sought = base.clone();
        sought["activities"][0]["timestamps"] =
            json!({"start": 1_800_000_003_000_u64, "end": 1_800_000_241_000_u64});
        assert!(!same_effective_presence(&base, &sought));

        let mut with_cover = jittered.clone();
        with_cover["activities"][0]["assets"] =
            json!({"large_image": "mp:external/sig/https/i1.sndcdn.com/archangel.jpg"});
        assert!(!same_effective_presence(&base, &with_cover));

        let mut paused = with_cover.clone();
        paused["activities"][0]["state"] = json!("Paused");
        paused["activities"][0]
            .as_object_mut()
            .unwrap()
            .remove("timestamps");
        assert!(!same_effective_presence(&with_cover, &paused));
    }

    #[test]
    fn webview2_multi_artist_plus_title_matches_pause_at_008_and_blocks_stale_revival() {
        let rpc_value = Arc::new(json!({
            "type": 2,
            "name": "SoundCloud",
            "application_id": "1090770350251458592",
            "details": "We feel tired  + jinatsu",
            "state": "повреждённый, jinatsu",
            "timestamps": {"start": 1_800_000_000_000_u64, "end": 1_800_000_134_000_u64},
            "assets": {"large_image": "mp:external/sig/https/i1.sndcdn.com/we-feel-tired.jpg"}
        }));
        let parsed = crate::activity::activity(&rpc_value).unwrap();

        // Observed live WebView2 SMTC session (`soundcloud_source: false`):
        // RPC carries multi-artist badge `"повреждённый, jinatsu"`, SMTC carries uploader `"повреждённый"`.
        let mut smtc = MusicPlayback {
            soundcloud_source: false,
            title: "We feel tired  + jinatsu".into(),
            artist: "повреждённый".into(),
            duration_ms: 134_000,
            position_ms: 0,
            start_ms: Some(1_800_000_000_000),
        };
        assert!(smtc.matches(&parsed));

        // Co-artist only or empty SMTC artist with matching duration also match, while an
        // unrelated session with the same title and a different artist must never match.
        let mut co_artist = smtc.clone();
        co_artist.artist = "jinatsu".into();
        assert!(co_artist.matches(&parsed));

        let mut foreign_same_title = smtc.clone();
        foreign_same_title.artist = "Another artist".into();
        assert!(!foreign_same_title.matches(&parsed));

        // Trace observed pause sequence, including SMTC pause arriving AFTER the 250ms window:
        let mut previous = None;
        assert_eq!(
            soundcloud_on_pause(Some(rpc_value.clone()), &mut previous, Some(&smtc)),
            Some(rpc_value.clone())
        );

        // 1. RPC clears first while SMTC still reports playing -> after 250ms window expires,
        // active presence clears (`None`), while `previous` stays retained for the matching track.
        assert_eq!(soundcloud_on_pause(None, &mut previous, Some(&smtc)), None);
        assert_eq!(previous, Some(rpc_value.clone()));

        // 2. Confirmed SMTC pause at 0:08 arrives after the reconciliation window -> restores card!
        smtc.start_ms = None;
        smtc.position_ms = 8_000;
        let restored = soundcloud_on_pause(None, &mut previous, Some(&smtc));
        assert_eq!(restored, Some(rpc_value.clone()));
        let mut presence = json!({"activities": []});
        merge(&mut presence, restored.as_deref(), Some(&smtc));
        assert_eq!(
            presence["activities"][0]["details"],
            "We feel tired  + jinatsu"
        );
        assert_eq!(presence["activities"][0]["state"], "Paused on 0:08");
        assert!(presence["activities"][0].get("timestamps").is_none());
        assert_eq!(
            presence["activities"][0]["assets"]["large_image"],
            "mp:external/sig/https/i1.sndcdn.com/we-feel-tired.jpg"
        );

        // 3. Confirmed pause with unavailable position (`0/0`) shows bare `Paused`:
        let mut no_pos = smtc.clone();
        no_pos.position_ms = 0;
        no_pos.duration_ms = 0;
        let mut bare_presence = json!({"activities": []});
        merge(&mut bare_presence, restored.as_deref(), Some(&no_pos));
        assert_eq!(bare_presence["activities"][0]["state"], "Paused");
        assert!(bare_presence["activities"][0].get("timestamps").is_none());

        // 4. Genuine stop (`None`) or foreign session (`!matches`) wipes `previous` permanently,
        // so a late event from the old track can never revive its card.
        assert_eq!(soundcloud_on_pause(None, &mut previous, None), None);
        assert!(previous.is_none());
        assert_eq!(soundcloud_on_pause(None, &mut previous, Some(&smtc)), None);
    }
}
