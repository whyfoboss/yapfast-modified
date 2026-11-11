//! Real Windows System Media Transport Controls (SMTC) integration
//! via the Windows Runtime (WinRT) `Windows.Media.Control` APIs.
//!
//! Provides event-driven media session tracking for desktop music players
//! (Spotify, SoundCloud, Apple Music) and browsers (Chrome, Edge).

use super::MediaTrack;

/// Normalizes application user model ID or executable name into a clean, human-readable source.
pub fn clean_source_name(raw_id: &str) -> String {
    let lower = raw_id.to_lowercase();
    if lower.contains("spotify") {
        "Spotify".into()
    } else if lower.contains("soundcloud") {
        "SoundCloud".into()
    } else if lower.contains("apple") && lower.contains("music") {
        "Apple Music".into()
    } else if lower.contains("msedge") || lower.contains("edge") {
        "Edge".into()
    } else if lower.contains("chrome") {
        "Chrome".into()
    } else if lower.contains("firefox") {
        "Firefox".into()
    } else if lower.contains("brave") {
        "Brave".into()
    } else if lower.contains("tidal") {
        "TIDAL".into()
    } else if lower.contains("foobar") {
        "foobar2000".into()
    } else if lower.contains("vlc") {
        "VLC".into()
    } else {
        // Strip path and .exe suffix if present
        let trimmed = raw_id
            .split(['\\', '/'])
            .next_back()
            .unwrap_or(raw_id)
            .trim_end_matches(".exe")
            .trim_end_matches(".EXE");
        if trimmed.is_empty() {
            "Media".into()
        } else {
            trimmed.to_string()
        }
    }
}

/// State tracking for active media sessions, supporting deterministic event processing and testing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaSessionState {
    pub source_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub is_playing: bool,
    pub duration_ms: u64,
    pub position_ms: u64,
    pub has_session: bool,
    pub thumbnail_url: Option<String>,
}

impl MediaSessionState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Handles a new active media session or session switch.
    pub fn on_session_changed(&mut self, source_id: String) {
        self.source_id = source_id;
        self.has_session = true;
    }

    /// Handles session closing or termination.
    pub fn on_session_closed(&mut self) {
        self.has_session = false;
        self.is_playing = false;
        self.title.clear();
        self.artist.clear();
        self.album.clear();
        self.duration_ms = 0;
        self.position_ms = 0;
        self.thumbnail_url = None;
    }

    /// Updates track metadata (title, artist, album).
    pub fn on_media_properties_changed(&mut self, title: String, artist: String, album: String) {
        self.title = title.trim().to_string();
        self.artist = artist.trim().to_string();
        self.album = album.trim().to_string();
    }

    /// Updates playback state (Playing vs Paused/Stopped).
    pub fn on_playback_info_changed(&mut self, is_playing: bool) {
        self.is_playing = is_playing;
    }

    /// Updates track timeline (duration and playback position in milliseconds).
    pub fn on_timeline_changed(&mut self, duration_ms: u64, position_ms: u64) {
        self.duration_ms = duration_ms;
        self.position_ms = position_ms.min(duration_ms);
    }

    /// Converts current state into an optional `MediaTrack`.
    ///
    /// Returns `None` if no session is active, no title is reported, or
    /// playback is completely stopped.
    pub fn to_media_track(&self) -> Option<MediaTrack> {
        if !self.has_session || self.title.is_empty() {
            return None;
        }

        let source = clean_source_name(&self.source_id);
        Some(MediaTrack {
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            duration_ms: self.duration_ms,
            position_ms: self.position_ms,
            is_playing: self.is_playing,
            thumbnail_url: self.thumbnail_url.clone(),
            url: None,
            source: Some(source),
        })
    }
}

#[cfg(windows)]
pub mod winrt {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::watch;
    use windows::{
        Foundation::TypedEventHandler,
        Media::Control::{
            GlobalSystemMediaTransportControlsSession,
            GlobalSystemMediaTransportControlsSessionManager,
            GlobalSystemMediaTransportControlsSessionPlaybackStatus,
        },
    };

    /// Active session subscription tracking RAII cleanup tokens.
    struct SessionSubscription {
        session: GlobalSystemMediaTransportControlsSession,
        media_token: i64,
        playback_token: i64,
        timeline_token: i64,
    }

    impl Drop for SessionSubscription {
        fn drop(&mut self) {
            let _ = self.session.RemoveMediaPropertiesChanged(self.media_token);
            let _ = self.session.RemovePlaybackInfoChanged(self.playback_token);
            let _ = self
                .session
                .RemoveTimelinePropertiesChanged(self.timeline_token);
        }
    }

    /// Native Windows SMTC event-driven listener.
    pub struct WindowsSmtcListener {
        manager: GlobalSystemMediaTransportControlsSessionManager,
        current_changed_token: i64,
        sessions_changed_token: i64,
        _subscription: Option<SessionSubscription>,
        _running: Arc<AtomicBool>,
    }

    impl Drop for WindowsSmtcListener {
        fn drop(&mut self) {
            self._subscription = None;
            let _ = self
                .manager
                .RemoveCurrentSessionChanged(self.current_changed_token);
            let _ = self
                .manager
                .RemoveSessionsChanged(self.sessions_changed_token);
        }
    }

    impl WindowsSmtcListener {
        /// Starts the event-driven SMTC listener and sends media updates over the watch channel.
        pub fn start(sender: watch::Sender<Option<MediaTrack>>) -> windows::core::Result<Self> {
            let manager =
                GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.join()?;
            let running = Arc::new(AtomicBool::new(true));

            // Helper to inspect active session and update watch sender
            let update_active_session = {
                let sender = sender.clone();
                let running = running.clone();
                Arc::new(
                    move |session_opt: Option<&GlobalSystemMediaTransportControlsSession>| {
                        if !running.load(Ordering::Relaxed) {
                            return;
                        }
                        let track = session_opt.and_then(|session| Self::read_session(session));
                        sender.send_replace(track);
                    },
                )
            };

            // Initial read
            let current = manager.GetCurrentSession().ok();
            update_active_session(current.as_ref());

            // Subscribe to CurrentSessionChanged on manager
            let current_changed_token = {
                let mgr = manager.clone();
                let update_active = update_active_session.clone();
                manager.CurrentSessionChanged(&TypedEventHandler::new(move |_mgr, _args| {
                    let session = mgr.GetCurrentSession().ok();
                    update_active(session.as_ref());
                    Ok(())
                }))?
            };

            // Subscribe to SessionsChanged on manager
            let sessions_changed_token = {
                let mgr = manager.clone();
                let update_active = update_active_session.clone();
                manager.SessionsChanged(&TypedEventHandler::new(move |_mgr, _args| {
                    let session = mgr.GetCurrentSession().ok();
                    update_active(session.as_ref());
                    Ok(())
                }))?
            };

            // Subscribe to session-level events if an active session exists
            let subscription = current.and_then(|session| {
                let media_update = update_active_session.clone();
                let s_clone = session.clone();
                let media_token = session
                    .MediaPropertiesChanged(&TypedEventHandler::new(move |_s, _args| {
                        media_update(Some(&s_clone));
                        Ok(())
                    }))
                    .ok()?;

                let playback_update = update_active_session.clone();
                let s_clone = session.clone();
                let playback_token = session
                    .PlaybackInfoChanged(&TypedEventHandler::new(move |_s, _args| {
                        playback_update(Some(&s_clone));
                        Ok(())
                    }))
                    .ok()?;

                let timeline_update = update_active_session.clone();
                let s_clone = session.clone();
                let timeline_token = session
                    .TimelinePropertiesChanged(&TypedEventHandler::new(move |_s, _args| {
                        timeline_update(Some(&s_clone));
                        Ok(())
                    }))
                    .ok()?;

                Some(SessionSubscription {
                    session,
                    media_token,
                    playback_token,
                    timeline_token,
                })
            });

            Ok(Self {
                manager,
                current_changed_token,
                sessions_changed_token,
                _subscription: subscription,
                _running: running,
            })
        }

        /// Reads media metadata, playback status, and timeline properties from a session.
        fn read_session(session: &GlobalSystemMediaTransportControlsSession) -> Option<MediaTrack> {
            let mut state = MediaSessionState::new();

            // 1. Source App ID
            let source_id = session
                .SourceAppUserModelId()
                .map(|s| s.to_string_lossy())
                .unwrap_or_else(|_| "Media".into());
            state.on_session_changed(source_id);

            // 2. Playback Info
            let playback_info = session.GetPlaybackInfo().ok()?;
            let status = playback_info.PlaybackStatus().ok()?;
            let is_playing =
                status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;
            state.on_playback_info_changed(is_playing);

            // If stopped or closed, do not report as active track
            if status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Stopped
                || status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Closed
            {
                return None;
            }

            // 3. Media Properties (async in WinRT, synchronous join on worker thread)
            let props_op = session.TryGetMediaPropertiesAsync().ok()?;
            let props = props_op.join().ok()?;
            let title = props
                .Title()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default();
            let artist = props
                .Artist()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default();
            let album = props
                .AlbumTitle()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default();
            state.on_media_properties_changed(title, artist, album);

            // 4. Timeline
            if let Ok(timeline) = session.GetTimelineProperties() {
                let duration_ms = timeline
                    .EndTime()
                    .map(|t| (t.Duration / 10_000).max(0) as u64)
                    .unwrap_or(0);
                let position_ms = timeline
                    .Position()
                    .map(|t| (t.Duration / 10_000).max(0) as u64)
                    .unwrap_or(0);
                state.on_timeline_changed(duration_ms, position_ms);
            }

            state.to_media_track()
        }
    }
}

/// Detector implementing `super::MediaDetector`.
#[derive(Clone, Debug)]
pub struct WindowsSmtcDetector {
    receiver: tokio::sync::watch::Receiver<Option<MediaTrack>>,
}

impl WindowsSmtcDetector {
    pub fn new(receiver: tokio::sync::watch::Receiver<Option<MediaTrack>>) -> Self {
        Self { receiver }
    }
}

impl super::MediaDetector for WindowsSmtcDetector {
    fn detect(&mut self) -> Option<MediaTrack> {
        self.receiver.borrow().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_source_name_recognition() {
        assert_eq!(clean_source_name("Spotify.exe"), "Spotify");
        assert_eq!(
            clean_source_name("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"),
            "Spotify"
        );
        assert_eq!(clean_source_name("SoundCloud.exe"), "SoundCloud");
        assert_eq!(
            clean_source_name("C:\\Program Files\\SoundCloud\\SoundCloud.exe"),
            "SoundCloud"
        );
        assert_eq!(clean_source_name("AppleMusic.exe"), "Apple Music");
        assert_eq!(clean_source_name("msedge.exe"), "Edge");
        assert_eq!(clean_source_name("chrome.exe"), "Chrome");
        assert_eq!(clean_source_name("vlc.exe"), "VLC");
        assert_eq!(clean_source_name("unknown_player.exe"), "unknown_player");
        assert_eq!(clean_source_name(""), "Media");
    }

    #[test]
    fn state_machine_lifecycle() {
        let mut state = MediaSessionState::new();
        assert_eq!(state.to_media_track(), None);

        // 1. Session created for SoundCloud
        state.on_session_changed("SoundCloud.exe".into());
        // Title still empty -> None
        assert_eq!(state.to_media_track(), None);

        // 2. Track properties arrive
        state.on_media_properties_changed("Starboy".into(), "The Weeknd".into(), "Starboy".into());
        state.on_playback_info_changed(true);
        state.on_timeline_changed(230_000, 45_000);

        let track = state.to_media_track().expect("track should be active");
        assert_eq!(track.title, "Starboy");
        assert_eq!(track.artist, "The Weeknd");
        assert_eq!(track.album, "Starboy");
        assert!(track.is_playing);
        assert_eq!(track.duration_ms, 230_000);
        assert_eq!(track.position_ms, 45_000);
        assert_eq!(track.source.as_deref(), Some("SoundCloud"));

        // 3. User pauses playback
        state.on_playback_info_changed(false);
        let track = state
            .to_media_track()
            .expect("track still exists when paused");
        assert!(!track.is_playing);

        // 4. Track changes
        state.on_media_properties_changed(
            "Blinding Lights".into(),
            "The Weeknd".into(),
            "After Hours".into(),
        );
        state.on_playback_info_changed(true);
        let track = state.to_media_track().unwrap();
        assert_eq!(track.title, "Blinding Lights");
        assert_eq!(track.album, "After Hours");
        assert!(track.is_playing);

        // 5. Session closed
        state.on_session_closed();
        assert_eq!(state.to_media_track(), None);
    }

    #[test]
    fn timeline_position_clamped_to_duration() {
        let mut state = MediaSessionState::new();
        state.on_session_changed("Spotify.exe".into());
        state.on_media_properties_changed("Song".into(), "Artist".into(), "Album".into());
        state.on_playback_info_changed(true);
        state.on_timeline_changed(100_000, 150_000); // position exceeds duration
        let track = state.to_media_track().unwrap();
        assert_eq!(track.duration_ms, 100_000);
        assert_eq!(track.position_ms, 100_000);
    }
}
