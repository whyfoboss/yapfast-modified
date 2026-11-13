//! The game this account shares: what a game says over Rich Presence, or
//! else the game detection finds. The account's `show_current_game` setting
//! turns both on and off; the custom status stays apart from them.

use crate::{api::Api, service::Snapshot};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::watch;

type Activity = Option<Arc<Value>>;

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
            text.split_whitespace()
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
        let title_matches = !title.is_empty()
            && (title == track_title
                || (duration_matches
                    && title
                        .strip_suffix('…')
                        .or_else(|| title.strip_suffix("..."))
                        .is_some_and(|prefix| {
                            let prefix = prefix.trim_end();
                            prefix.chars().count() >= 12 && track_title.starts_with(prefix)
                        })));
        if !title_matches {
            return false;
        }
        if self.soundcloud_source && rpc.name.eq_ignore_ascii_case("SoundCloud") {
            return true;
        }
        let artist = normalize(&self.artist);
        !artist.is_empty() && rpc.state.as_deref().is_some_and(|state| normalize(state) == artist)
    }
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

pub(crate) fn merge(presence: &mut Value, activity: Option<&Value>, music: Option<&MusicPlayback>) {
    if let Some(activities) = presence["activities"].as_array_mut() {
        activities.retain(|value| value["type"] == 4);
        if let Some(activity) = activity.filter(|activity| eligible(activity)) {
            let mut activity = activity.clone();
            if let Some(music) = music {
                let matched =
                    crate::activity::activity(&activity).is_some_and(|rpc| music.matches(&rpc));
                tracing::debug!(target: "yapfast_media", matched,
                    paused = music.start_ms.is_none(),
                    has_artwork = activity["assets"]["large_image"].is_string(),
                    "gateway music candidate");
                let replace_stale_track = !matched
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
        assert_eq!(paused["state"], "Paused at 1:49 · Artist, Collaborator");

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
}
