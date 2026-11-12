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
    pub playback_type: PlaybackType,
    pub source_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub is_playing: bool,
    pub duration_ms: u64,
    pub position_ms: u64,
    pub position_updated_ms: Option<u64>,
    pub has_session: bool,
    pub thumbnail: Option<std::sync::Arc<crate::chat::Picture>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaybackType {
    #[default]
    Unknown,
    Music,
    Video,
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
        self.position_updated_ms = None;
        self.thumbnail = None;
        self.playback_type = PlaybackType::Unknown;
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
        self.position_ms = if duration_ms == 0 {
            position_ms
        } else {
            position_ms.min(duration_ms)
        };
    }

    /// Converts current state into an optional `MediaTrack`.
    ///
    /// Returns `None` if no session is active, no title is reported, or
    /// playback is completely stopped.
    pub fn to_media_track(&self) -> Option<MediaTrack> {
        if !self.has_session || self.title.is_empty() || self.playback_type == PlaybackType::Video {
            return None;
        }

        let source = clean_source_name(&self.source_id);
        #[cfg(feature = "discord")]
        let trusted_music_source = yapfast_discord::activity::music_integration(&source)
            && !yapfast_discord::activity::browser_media_source(&self.source_id);
        #[cfg(not(feature = "discord"))]
        let trusted_music_source = false;
        Some(MediaTrack {
            requires_music_rpc: !trusted_music_source || self.playback_type != PlaybackType::Music,
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            duration_ms: self.duration_ms,
            position_ms: self.position_ms,
            position_updated_ms: self.position_updated_ms,
            is_playing: self.is_playing,
            thumbnail_url: None,
            thumbnail: self.thumbnail.clone(),
            url: None,
            source: Some(source),
        })
    }
}

#[cfg(windows)]
pub mod winrt {
    use super::*;
    use windows::Media::MediaPlaybackType;

    fn classify(kind: Option<MediaPlaybackType>) -> PlaybackType {
        match kind {
            Some(MediaPlaybackType::Music) => PlaybackType::Music,
            Some(MediaPlaybackType::Video) => PlaybackType::Video,
            _ => PlaybackType::Unknown,
        }
    }
    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll, Waker},
        time::Instant,
    };
    use std::{
        sync::{
            Arc,
            atomic::{AtomicU8, Ordering},
        },
        thread::{self, Thread},
    };
    use tokio::sync::watch;
    use windows::{
        Foundation::TypedEventHandler,
        Media::Control::{
            GlobalSystemMediaTransportControlsSession as Session,
            GlobalSystemMediaTransportControlsSessionManager as Manager,
            GlobalSystemMediaTransportControlsSessionMediaProperties as Properties,
            GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
        },
        Storage::Streams::DataReader,
        Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize},
    };

    const SESSION: u8 = 1;
    const METADATA: u8 = 2;
    const PLAYBACK: u8 = 4;
    const STOP: u8 = 8;
    const ASYNC: u8 = 16;

    #[derive(Clone)]
    struct Wake {
        pending: Arc<AtomicU8>,
        thread: Thread,
    }

    impl Wake {
        fn notify(&self, event: u8) {
            self.pending.fetch_or(event, Ordering::Release);
            self.thread.unpark();
        }

        fn handler<
            S: windows::core::RuntimeType + 'static,
            A: windows::core::RuntimeType + 'static,
        >(
            &self,
            event: u8,
        ) -> TypedEventHandler<S, A> {
            let wake = self.clone();
            TypedEventHandler::new(move |_, _| {
                wake.notify(event);
                Ok(())
            })
        }
    }

    impl std::task::Wake for Wake {
        fn wake(self: Arc<Self>) {
            self.notify(ASYNC);
        }
    }

    type Metadata = Pin<Box<dyn Future<Output = windows::core::Result<Properties>>>>;
    type Artwork = Pin<Box<dyn Future<Output = Option<crate::chat::Picture>>>>;

    struct Subscription {
        session: Session,
        media: Option<i64>,
        playback: Option<i64>,
        timeline: Option<i64>,
    }

    impl Subscription {
        fn new(session: Session, wake: &Wake) -> windows::core::Result<Self> {
            let mut sub = Self {
                session,
                media: None,
                playback: None,
                timeline: None,
            };
            sub.media = Some(
                sub.session
                    .MediaPropertiesChanged(&wake.handler(METADATA))?,
            );
            sub.playback = Some(sub.session.PlaybackInfoChanged(&wake.handler(PLAYBACK))?);
            sub.timeline = Some(
                sub.session
                    .TimelinePropertiesChanged(&wake.handler(PLAYBACK))?,
            );
            Ok(sub)
        }
    }

    impl Drop for Subscription {
        fn drop(&mut self) {
            if let Some(token) = self.media {
                let _ = self.session.RemoveMediaPropertiesChanged(token);
            }
            if let Some(token) = self.playback {
                let _ = self.session.RemovePlaybackInfoChanged(token);
            }
            if let Some(token) = self.timeline {
                let _ = self.session.RemoveTimelinePropertiesChanged(token);
            }
        }
    }

    pub struct WindowsSmtcListener(Wake);

    impl Drop for WindowsSmtcListener {
        fn drop(&mut self) {
            self.0.notify(STOP);
        }
    }

    impl WindowsSmtcListener {
        pub fn start(sender: watch::Sender<Option<MediaTrack>>) -> std::io::Result<Self> {
            let pending = Arc::new(AtomicU8::new(SESSION));
            let updates = pending.clone();
            let worker = thread::Builder::new()
                .name("windows-smtc".into())
                .spawn(move || {
                    // SAFETY: the worker owns the apartment and releases it after the WinRT objects.
                    let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
                        .ok()
                        .and_then(|()| {
                            let result = listen(
                                &sender,
                                Wake {
                                    pending: updates,
                                    thread: thread::current(),
                                },
                            );
                            unsafe { CoUninitialize() };
                            result
                        });
                    if let Err(error) = result {
                        tracing::warn!(%error, "Windows SMTC media detection unavailable");
                        sender.send_if_modified(|track| track.take().is_some());
                    }
                })?;
            Ok(Self(Wake {
                pending,
                thread: worker.thread().clone(),
            }))
        }
    }

    fn listen(sender: &watch::Sender<Option<MediaTrack>>, wake: Wake) -> windows::core::Result<()> {
        let manager = Manager::RequestAsync()?.join()?;
        let current_token = manager.CurrentSessionChanged(&wake.handler(SESSION))?;
        let sessions_token = manager.SessionsChanged(&wake.handler(SESSION));
        let result = sessions_token.as_ref().map(|_| ()).map_err(Clone::clone);
        if result.is_ok() {
            let mut subscription: Option<Subscription> = None;
            let mut state = MediaSessionState::new();
            let mut metadata_dirty = true;
            let mut timeline_revision = None;
            let mut was_paused = false;
            let mut metadata: Option<Metadata> = None;
            let mut artwork: Option<Artwork> = None;
            let async_waker = Waker::from(Arc::new(wake.clone()));
            loop {
                let events = wake.pending.swap(0, Ordering::AcqRel);
                if events & STOP != 0 {
                    break;
                }
                if events == 0 {
                    thread::park();
                    continue;
                }

                let received = Instant::now();
                if events & SESSION != 0 {
                    let reported = manager.GetCurrentSession().ok();
                    let manager_has_current = reported.is_some();
                    let current = reported.or_else(|| {
                        let previous = &subscription.as_ref()?.session;
                        manager
                            .GetSessions()
                            .ok()?
                            .into_iter()
                            .find(|session| session == previous)
                    });
                    if !manager_has_current && current.is_some() {
                        tracing::debug!(target: "yapfast_media", "SMTC previous session still visible");
                    }
                    if current.as_ref() != subscription.as_ref().map(|sub| &sub.session) {
                        subscription = None;
                        state.on_session_closed();
                        metadata = None;
                        artwork = None;
                        tracing::debug!(target: "yapfast_media", present = current.is_some(), "SMTC session changed");
                        timeline_revision = None;
                        was_paused = false;
                        sender.send_if_modified(|track| track.take().is_some());
                        if let Some(session) = current {
                            let id = session
                                .SourceAppUserModelId()
                                .map(|id| id.to_string_lossy())
                                .unwrap_or_else(|_| "Media".into());
                            match Subscription::new(session, &wake) {
                                Ok(sub) => {
                                    tracing::debug!(target: "yapfast_media", "SMTC handlers attached");
                                    subscription = Some(sub);
                                    state.on_session_changed(id);
                                }
                                Err(error) => {
                                    tracing::debug!(%error, "could not subscribe to the Windows media session")
                                }
                            }
                        }
                        metadata_dirty = true;
                    }
                }
                let Some(sub) = &subscription else { continue };
                metadata_dirty |= events & METADATA != 0;
                let session = &sub.session;
                let info = session.GetPlaybackInfo();
                let playback_type = classify(
                    info.as_ref()
                        .ok()
                        .and_then(|info| info.PlaybackType().ok()?.Value().ok()),
                );
                let status = info.and_then(|info| info.PlaybackStatus());
                if !matches!(status, Ok(Status::Playing | Status::Paused)) {
                    tracing::debug!(target: "yapfast_media", "SMTC playback inactive");
                    metadata_dirty = true;
                    metadata = None;
                    artwork = None;
                    was_paused = false;
                    timeline_revision = None;
                    let source = std::mem::take(&mut state.source_id);
                    state.on_session_closed();
                    state.on_session_changed(source);
                    sender.send_if_modified(|track| track.take().is_some());
                    continue;
                }
                if playback_type == PlaybackType::Video {
                    state.playback_type = PlaybackType::Video;
                    artwork = None;
                    metadata = None;
                    metadata_dirty = true;
                    sender.send_if_modified(|track| track.take().is_some());
                    tracing::debug!(target: "yapfast_media", "SMTC video excluded");
                    continue;
                }
                state.on_playback_info_changed(matches!(status, Ok(Status::Playing)));
                if metadata_dirty {
                    artwork = None;
                    state.title.clear();
                    state.thumbnail = None;
                    metadata = session
                        .TryGetMediaPropertiesAsync()
                        .ok()
                        .map(|operation| Box::pin(async move { operation.await }) as Metadata);
                    metadata_dirty = false;
                }
                if let Some(pending) = metadata.as_mut()
                    && let Poll::Ready(result) = pending
                        .as_mut()
                        .poll(&mut Context::from_waker(&async_waker))
                {
                    metadata = None;
                    if let Ok(props) = result {
                        state.playback_type = match classify(
                            props.PlaybackType().ok().and_then(|kind| kind.Value().ok()),
                        ) {
                            PlaybackType::Unknown => playback_type,
                            kind => kind,
                        };
                        let previous = (&state.title, &state.artist, &state.album);
                        let updated = (
                            props
                                .Title()
                                .map(|s| s.to_string_lossy())
                                .unwrap_or_default(),
                            props
                                .Artist()
                                .map(|s| s.to_string_lossy())
                                .unwrap_or_default(),
                            props
                                .AlbumTitle()
                                .map(|s| s.to_string_lossy())
                                .unwrap_or_default(),
                        );
                        if (&updated.0, &updated.1, &updated.2) != previous {
                            state.thumbnail = None;
                        }
                        state.on_media_properties_changed(updated.0, updated.1, updated.2);
                        if state.playback_type != PlaybackType::Video {
                            artwork = Some(Box::pin(thumbnail(props)));
                        }
                    } else {
                        tracing::debug!(target: "yapfast_media", "SMTC metadata unavailable");
                    }
                }
                let previous_revision = timeline_revision;
                if let Ok(timeline) = session.GetTimelineProperties() {
                    let millis = |time: windows::Foundation::TimeSpan| {
                        (time.Duration / 10_000).max(0) as u64
                    };
                    state.on_timeline_changed(
                        timeline.EndTime().map(millis).unwrap_or(0),
                        timeline.Position().map(millis).unwrap_or(state.position_ms),
                    );
                    let revision = timeline.LastUpdatedTime().ok().and_then(|time| {
                        u64::try_from(time.UniversalTime / 10_000)
                            .ok()?
                            .checked_sub(11_644_473_600_000)
                    });
                    if revision != timeline_revision {
                        state.position_updated_ms = revision;
                        timeline_revision = revision;
                    }
                }
                if was_paused && state.is_playing && previous_revision == timeline_revision {
                    // A producer can resume without updating the timeline timestamp.
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default();
                    state.position_updated_ms = Some(now.as_millis() as u64);
                }
                was_paused = !state.is_playing;
                let next = state.to_media_track();
                if sender.send_if_modified(|track| {
                    if *track == next {
                        return false;
                    }
                    *track = next;
                    true
                }) {
                    let published_at_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    tracing::debug!(target: "yapfast_media", events, paused = was_paused,
                        position_ms = state.position_ms, elapsed_ms = received.elapsed().as_millis() as u64,
                        published_at_ms, "SMTC state published");
                }
                if let Some(pending) = artwork.as_mut()
                    && let Poll::Ready(picture) = pending
                        .as_mut()
                        .poll(&mut Context::from_waker(&async_waker))
                {
                    artwork = None;
                    if let Some(picture) = picture {
                        state.thumbnail = Some(Arc::new(picture));
                    }
                    // Process any pending track change before publishing this result.
                    wake.notify(ASYNC);
                }
            }
        }
        let _ = manager.RemoveCurrentSessionChanged(current_token);
        if let Ok(token) = sessions_token {
            let _ = manager.RemoveSessionsChanged(token);
        }
        result
    }

    async fn thumbnail(props: Properties) -> Option<crate::chat::Picture> {
        let stream = props.Thumbnail().ok()?.OpenReadAsync().ok()?.await.ok()?;
        let size = u32::try_from(stream.Size().ok()?).ok()?;
        if size == 0 || size > 2 * 1024 * 1024 {
            return None;
        }
        let input = stream.GetInputStreamAt(0).ok()?;
        let reader = DataReader::CreateDataReader(&input).ok()?;
        if reader.LoadAsync(size).ok()?.await.ok()? != size {
            return None;
        }
        let mut bytes = vec![0; size as usize];
        reader.ReadBytes(&mut bytes).ok()?;
        crate::chat::decode_picture(&bytes, 128, 128)
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

    #[cfg(feature = "discord")]
    #[test]
    fn opaque_browser_source_reporting_music_cannot_publish_a_card() {
        let mut state = MediaSessionState::new();
        state.on_session_changed("A12BC34DE56F7890".into());
        state.on_media_properties_changed("Video title".into(), "Channel".into(), "".into());
        state.on_playback_info_changed(true);
        state.playback_type = PlaybackType::Music;
        let mut manager = super::super::ActivityManager::new();
        manager.set_media_track(state.to_media_track());
        assert!(manager.to_discord_activity(1000).is_none());

        state.on_session_changed("com.richardhbtz.soundcloud-rpc".into());
        state.on_media_properties_changed("Song".into(), "Artist".into(), "".into());
        manager.set_media_track(state.to_media_track());
        assert!(manager.to_discord_activity(1000).is_some());
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
