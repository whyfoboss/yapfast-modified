//! Game activity detection, media sessions, and Rich Presence management.

pub mod windows_smtc;

use crate::chat::Activity;
use std::collections::BTreeMap;

fn default_true() -> bool {
    true
}

/// An identified application or game.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AppIdentity {
    pub id: String,
    pub display_name: String,
    pub icon_url: Option<String>,
    pub running: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: u32,
    #[serde(default)]
    pub started_ms: Option<u64>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub party: Option<(u64, u64)>,
    #[serde(default)]
    pub large_image: Option<String>,
    #[serde(default)]
    pub large_text: Option<String>,
    #[serde(default)]
    pub small_image: Option<String>,
    #[serde(default)]
    pub small_text: Option<String>,
}

/// A media session track (from SMTC, Spotify, or compatible player).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MediaTrack {
    /// An ambiguous session can enrich an identified music RPC, but cannot win on its own.
    #[serde(default)]
    pub requires_music_rpc: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub position_ms: u64,
    #[serde(default)]
    pub position_updated_ms: Option<u64>,
    pub is_playing: bool,
    pub thumbnail_url: Option<String>,
    #[serde(skip)]
    pub thumbnail: Option<std::sync::Arc<crate::chat::Picture>>,
    pub url: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

/// User configuration rule for a specific game/application.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GameActivityRule {
    pub executable: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: u32,
    #[serde(default)]
    pub custom_name: Option<String>,
}

/// Deterministic test scenarios for Fake Discord and WSL2 manual GUI verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SimulatedScenario {
    /// 1. Single game running (Minecraft).
    SingleGame = 1,
    /// 2. Three simultaneous games (VALORANT, Minecraft, Subnautica).
    ThreeGames = 2,
    /// 3. Selective: VALORANT & Minecraft disabled, Subnautica active.
    SelectiveGames = 3,
    /// 4. All games disabled.
    AllGamesDisabled = 4,
    /// 5. Music playing with album artwork and progress bar.
    MusicPlaying = 5,
    /// 6. Music paused.
    MusicPaused = 6,
    /// 7. Game and music active simultaneously.
    GameAndMusic = 7,
    /// 8. Rich presence with party/lobby and timestamps.
    RichPresenceParty = 8,
    /// 9. Activity with missing/invalid image data.
    MissingArtwork = 9,
    /// 10. No activity available.
    NoActivity = 10,
}

impl SimulatedScenario {
    pub fn from_index(index: i32) -> Option<Self> {
        match index {
            1 => Some(Self::SingleGame),
            2 => Some(Self::ThreeGames),
            3 => Some(Self::SelectiveGames),
            4 => Some(Self::AllGamesDisabled),
            5 => Some(Self::MusicPlaying),
            6 => Some(Self::MusicPaused),
            7 => Some(Self::GameAndMusic),
            8 => Some(Self::RichPresenceParty),
            9 => Some(Self::MissingArtwork),
            10 => Some(Self::NoActivity),
            _ => None,
        }
    }
}

/// Trait for platform-specific process/application detection.
pub trait ProcessDetector: Send + Sync {
    fn detect(&mut self) -> Vec<AppIdentity>;
}

/// Trait for platform-specific media playback detection (e.g. SMTC, MPRIS).
pub trait MediaDetector: Send + Sync {
    fn detect(&mut self) -> Option<MediaTrack>;
}

/// Mock process detector for testing and simulation.
#[derive(Clone, Debug, Default)]
pub struct MockProcessDetector {
    pub apps: Vec<AppIdentity>,
}

impl ProcessDetector for MockProcessDetector {
    fn detect(&mut self) -> Vec<AppIdentity> {
        self.apps.clone()
    }
}

/// Mock media detector for testing and simulation.
#[derive(Clone, Debug, Default)]
pub struct MockMediaDetector {
    pub track: Option<MediaTrack>,
}

impl MediaDetector for MockMediaDetector {
    fn detect(&mut self) -> Option<MediaTrack> {
        self.track.clone()
    }
}

/// The outcome of the activity selection policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectedActivity {
    /// Sharing is disabled globally.
    SharingDisabled,
    /// No running or permitted activity available.
    None,
    /// A selected game/application.
    Game(AppIdentity),
    /// A media track.
    Media(MediaTrack),
}

/// Coordinates registered applications, media tracks, user preferences, and activity selection.
#[derive(Clone, Debug)]
pub struct ActivityManager {
    pub registered_apps: BTreeMap<String, AppIdentity>,
    pub rules: BTreeMap<String, GameActivityRule>,
    pub active_media: Option<MediaTrack>,
    simulated_media: Option<MediaTrack>,
    pub preferred_app_id: Option<String>,
    pub global_sharing_enabled: bool,
    pub active_scenario: Option<SimulatedScenario>,
    pub live_rpc_app_id: Option<String>,
}

impl Default for ActivityManager {
    fn default() -> Self {
        Self {
            registered_apps: BTreeMap::new(),
            rules: BTreeMap::new(),
            active_media: None,
            simulated_media: None,
            preferred_app_id: None,
            global_sharing_enabled: true,
            active_scenario: None,
            live_rpc_app_id: None,
        }
    }
}

impl ActivityManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates running processes reported by detector.
    pub fn update_detected_processes(&mut self, detected: Vec<AppIdentity>, now_ms: u64) {
        // Mark existing apps not in detected as not running
        for app in self.registered_apps.values_mut() {
            app.running = false;
        }
        for mut app in detected {
            let key = app.id.to_lowercase();
            if let Some(rule) = self.rules.get(&key) {
                app.enabled = rule.enabled;
                app.priority = rule.priority;
                if let Some(custom) = &rule.custom_name {
                    app.display_name = custom.clone();
                }
            }
            if app.started_ms.is_none() {
                app.started_ms = Some(now_ms);
            }
            self.registered_apps.insert(key, app);
        }
    }

    /// Sets or updates a user rule for an application.
    pub fn set_rule(
        &mut self,
        executable: &str,
        enabled: bool,
        priority: u32,
        custom_name: Option<String>,
    ) {
        let key = executable.to_lowercase();
        let rule = GameActivityRule {
            executable: executable.to_string(),
            name: custom_name
                .clone()
                .unwrap_or_else(|| executable.to_string()),
            enabled,
            priority,
            custom_name: custom_name.clone(),
        };
        self.rules.insert(key.clone(), rule);
        if let Some(app) = self.registered_apps.get_mut(&key) {
            app.enabled = enabled;
            app.priority = priority;
            if let Some(custom) = custom_name {
                app.display_name = custom;
            }
        }
    }

    /// Finds a rule by its configured identity or a known executable alias.
    pub fn rule_for(&self, name: &str) -> Option<&GameActivityRule> {
        let key = activity_key(name);
        if key != "wallpaper engine" {
            return self.rules.get(&key);
        }
        self.rules
            .values()
            .filter(|rule| activity_key(&rule.executable) == key)
            .min_by_key(|rule| (rule.enabled, rule.executable.as_str()))
    }

    pub fn activity_enabled(&self, name: &str) -> bool {
        self.rule_for(name).is_none_or(|rule| rule.enabled)
    }

    /// Removes an application rule and its registered entry.
    pub fn remove_app(&mut self, id: &str) {
        let key = id.to_lowercase();
        self.rules.remove(&key);
        self.registered_apps.remove(&key);
        if self.preferred_app_id.as_deref() == Some(&key) {
            self.preferred_app_id = None;
        }
    }

    /// Sets manual priority preference for a specific application.
    pub fn set_preferred_app(&mut self, id: Option<String>) {
        self.preferred_app_id = id.map(|s| s.to_lowercase());
    }

    /// Enables or disables global activity sharing.
    pub fn set_global_sharing(&mut self, enabled: bool) {
        self.global_sharing_enabled = enabled;
    }

    /// Updates the active media track.
    pub fn set_media_track(&mut self, track: Option<MediaTrack>) {
        self.active_media = track;
    }

    /// Sets a simulated scenario for Fake Discord or manual testing.
    pub fn set_simulated_scenario(&mut self, scenario: Option<SimulatedScenario>) {
        if self.active_scenario.is_some() && scenario.is_none() {
            self.registered_apps.clear();
            self.simulated_media = None;
        }
        self.active_scenario = scenario;
        if let Some(s) = scenario {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            self.load_scenario_state(s, now);
        }
    }

    /// Populates scenario apps and media state into the manager.
    pub fn load_scenario_state(&mut self, scenario: SimulatedScenario, now_ms: u64) {
        self.registered_apps.clear();
        self.simulated_media = None;

        match scenario {
            SimulatedScenario::SingleGame => {
                let mut app = AppIdentity {
                    id: "minecraft.exe".into(),
                    display_name: "Minecraft".into(),
                    icon_url: Some("icons/play.svg".into()),
                    running: true,
                    enabled: true,
                    priority: 0,
                    started_ms: Some(now_ms.saturating_sub(1_240_000)),
                    details: Some("Exploring Caves".into()),
                    state: Some("Survival Mode".into()),
                    party: None,
                    large_image: Some("icons/play.svg".into()),
                    large_text: Some("Minecraft 1.20".into()),
                    small_image: None,
                    small_text: None,
                };
                if let Some(rule) = self.rules.get("minecraft.exe") {
                    app.enabled = rule.enabled;
                    app.priority = rule.priority;
                }
                self.registered_apps.insert("minecraft.exe".into(), app);
            }
            SimulatedScenario::ThreeGames => {
                let games = vec![
                    AppIdentity {
                        id: "valorant.exe".into(),
                        display_name: "VALORANT".into(),
                        icon_url: Some("icons/play.svg".into()),
                        running: true,
                        enabled: true,
                        priority: 10,
                        started_ms: Some(now_ms.saturating_sub(3_600_000)),
                        details: Some("Competitive".into()),
                        state: Some("In Match (Score 11-9)".into()),
                        party: Some((5, 5)),
                        large_image: Some("icons/play.svg".into()),
                        large_text: Some("Ascent".into()),
                        small_image: Some("icons/play.svg".into()),
                        small_text: Some("Ranked".into()),
                    },
                    AppIdentity {
                        id: "minecraft.exe".into(),
                        display_name: "Minecraft".into(),
                        icon_url: Some("icons/play.svg".into()),
                        running: true,
                        enabled: true,
                        priority: 5,
                        started_ms: Some(now_ms.saturating_sub(2_400_000)),
                        details: Some("Exploring Caves".into()),
                        state: Some("Survival Mode".into()),
                        party: None,
                        large_image: Some("icons/play.svg".into()),
                        large_text: Some("Minecraft 1.20".into()),
                        small_image: None,
                        small_text: None,
                    },
                    AppIdentity {
                        id: "subnautica.exe".into(),
                        display_name: "Subnautica".into(),
                        icon_url: Some("icons/play.svg".into()),
                        running: true,
                        enabled: true,
                        priority: 0,
                        started_ms: Some(now_ms.saturating_sub(1_800_000)),
                        details: Some("Grand Reef".into()),
                        state: Some("Piloting Cyclops".into()),
                        party: None,
                        large_image: Some("icons/play.svg".into()),
                        large_text: Some("Subnautica".into()),
                        small_image: None,
                        small_text: None,
                    },
                ];
                for mut app in games {
                    let key = app.id.to_lowercase();
                    if let Some(rule) = self.rules.get(&key) {
                        app.enabled = rule.enabled;
                        app.priority = rule.priority;
                    }
                    self.registered_apps.insert(key, app);
                }
            }
            SimulatedScenario::SelectiveGames => {
                // Preset where VALORANT and Minecraft are disabled
                let games = vec![
                    AppIdentity {
                        id: "valorant.exe".into(),
                        display_name: "VALORANT".into(),
                        icon_url: Some("icons/play.svg".into()),
                        running: true,
                        enabled: false,
                        priority: 10,
                        started_ms: Some(now_ms.saturating_sub(3_600_000)),
                        details: Some("Competitive".into()),
                        state: Some("In Match (Score 11-9)".into()),
                        party: Some((5, 5)),
                        large_image: Some("icons/play.svg".into()),
                        large_text: Some("Ascent".into()),
                        small_image: Some("icons/play.svg".into()),
                        small_text: Some("Ranked".into()),
                    },
                    AppIdentity {
                        id: "minecraft.exe".into(),
                        display_name: "Minecraft".into(),
                        icon_url: Some("icons/play.svg".into()),
                        running: true,
                        enabled: false,
                        priority: 5,
                        started_ms: Some(now_ms.saturating_sub(2_400_000)),
                        details: Some("Exploring Caves".into()),
                        state: Some("Survival Mode".into()),
                        party: None,
                        large_image: Some("icons/play.svg".into()),
                        large_text: Some("Minecraft 1.20".into()),
                        small_image: None,
                        small_text: None,
                    },
                    AppIdentity {
                        id: "subnautica.exe".into(),
                        display_name: "Subnautica".into(),
                        icon_url: Some("icons/play.svg".into()),
                        running: true,
                        enabled: true,
                        priority: 0,
                        started_ms: Some(now_ms.saturating_sub(1_800_000)),
                        details: Some("Grand Reef".into()),
                        state: Some("Piloting Cyclops".into()),
                        party: None,
                        large_image: Some("icons/play.svg".into()),
                        large_text: Some("Subnautica".into()),
                        small_image: None,
                        small_text: None,
                    },
                ];
                for app in games {
                    self.registered_apps.insert(app.id.clone(), app);
                }
            }
            SimulatedScenario::AllGamesDisabled => {
                let games = vec![
                    AppIdentity {
                        id: "valorant.exe".into(),
                        display_name: "VALORANT".into(),
                        icon_url: Some("icons/play.svg".into()),
                        running: true,
                        enabled: false,
                        priority: 10,
                        started_ms: Some(now_ms.saturating_sub(3_600_000)),
                        details: None,
                        state: None,
                        party: None,
                        large_image: None,
                        large_text: None,
                        small_image: None,
                        small_text: None,
                    },
                    AppIdentity {
                        id: "minecraft.exe".into(),
                        display_name: "Minecraft".into(),
                        icon_url: Some("icons/play.svg".into()),
                        running: true,
                        enabled: false,
                        priority: 5,
                        started_ms: Some(now_ms.saturating_sub(2_400_000)),
                        details: None,
                        state: None,
                        party: None,
                        large_image: None,
                        large_text: None,
                        small_image: None,
                        small_text: None,
                    },
                ];
                for app in games {
                    self.registered_apps.insert(app.id.clone(), app);
                }
            }
            SimulatedScenario::MusicPlaying => {
                self.simulated_media = Some(MediaTrack {
                    requires_music_rpc: false,
                    title: "Starboy".into(),
                    artist: "The Weeknd".into(),
                    album: "Starboy".into(),
                    duration_ms: 230_000,
                    position_ms: 68_000,
                    position_updated_ms: Some(now_ms),
                    is_playing: true,
                    thumbnail_url: Some("icons/volume-2.svg".into()),
                    thumbnail: None,
                    url: Some("https://open.spotify.com".into()),
                    source: Some("Spotify".into()),
                });
            }
            SimulatedScenario::MusicPaused => {
                self.simulated_media = Some(MediaTrack {
                    requires_music_rpc: false,
                    title: "Blinding Lights".into(),
                    artist: "The Weeknd".into(),
                    album: "After Hours".into(),
                    duration_ms: 200_000,
                    position_ms: 104_000,
                    position_updated_ms: Some(now_ms),
                    is_playing: false,
                    thumbnail_url: Some("icons/volume-2.svg".into()),
                    thumbnail: None,
                    url: Some("https://open.spotify.com".into()),
                    source: Some("Spotify".into()),
                });
            }
            SimulatedScenario::GameAndMusic => {
                let app = AppIdentity {
                    id: "cyberpunk2077.exe".into(),
                    display_name: "Cyberpunk 2077".into(),
                    icon_url: Some("icons/play.svg".into()),
                    running: true,
                    enabled: true,
                    priority: 5,
                    started_ms: Some(now_ms.saturating_sub(4_500_000)),
                    details: Some("Night City".into()),
                    state: Some("Roaming Westbrook".into()),
                    party: None,
                    large_image: Some("icons/play.svg".into()),
                    large_text: Some("Cyberpunk 2077".into()),
                    small_image: None,
                    small_text: None,
                };
                self.registered_apps.insert(app.id.clone(), app);
                self.simulated_media = Some(MediaTrack {
                    requires_music_rpc: false,
                    title: "Starboy".into(),
                    artist: "The Weeknd".into(),
                    album: "Starboy".into(),
                    duration_ms: 230_000,
                    position_ms: 68_000,
                    position_updated_ms: Some(now_ms),
                    is_playing: true,
                    thumbnail_url: Some("icons/volume-2.svg".into()),
                    thumbnail: None,
                    url: Some("https://open.spotify.com".into()),
                    source: Some("Spotify".into()),
                });
            }
            SimulatedScenario::RichPresenceParty => {
                let app = AppIdentity {
                    id: "rocketleague.exe".into(),
                    display_name: "Rocket League".into(),
                    icon_url: Some("icons/play.svg".into()),
                    running: true,
                    enabled: true,
                    priority: 0,
                    started_ms: Some(now_ms.saturating_sub(180_000)),
                    details: Some("Ranked Doubles 2v2".into()),
                    state: Some("In Lobby".into()),
                    party: Some((2, 5)),
                    large_image: Some("icons/play.svg".into()),
                    large_text: Some("Champion II".into()),
                    small_image: Some("icons/play.svg".into()),
                    small_text: Some("In Party".into()),
                };
                self.registered_apps.insert(app.id.clone(), app);
            }
            SimulatedScenario::MissingArtwork => {
                let app = AppIdentity {
                    id: "custom_indie.exe".into(),
                    display_name: "My Indie Game".into(),
                    icon_url: None,
                    running: true,
                    enabled: true,
                    priority: 0,
                    started_ms: Some(now_ms.saturating_sub(600_000)),
                    details: Some("Custom Prototype".into()),
                    state: Some("Testing Build".into()),
                    party: None,
                    large_image: None,
                    large_text: None,
                    small_image: None,
                    small_text: None,
                };
                self.registered_apps.insert(app.id.clone(), app);
            }
            SimulatedScenario::NoActivity => {}
        }
    }

    /// Resolves the currently active activity deterministically.
    pub fn resolve(&self, now_ms: u64) -> SelectedActivity {
        // 1. Global switch check
        if !self.global_sharing_enabled {
            return SelectedActivity::SharingDisabled;
        }

        // 2. Collect candidates: running and enabled
        let mut candidates: Vec<&AppIdentity> = self
            .registered_apps
            .values()
            .filter(|app| {
                app.running
                    && self
                        .rule_for(&app.id)
                        .map_or(app.enabled, |rule| rule.enabled)
            })
            .collect();

        // 3. If a preferred application is running and enabled, it takes precedence
        if let Some(preferred) = &self.preferred_app_id
            && let Some(app) = candidates
                .iter()
                .find(|app| app.id.to_lowercase() == *preferred)
        {
            let mut app = (*app).clone();
            if app.started_ms.is_none() {
                app.started_ms = Some(now_ms);
            }
            return SelectedActivity::Game(app);
        }

        // 4. Sort candidates: highest priority first, then earliest started_ms, then display name
        candidates.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.started_ms.cmp(&b.started_ms))
                .then_with(|| a.display_name.cmp(&b.display_name))
        });

        if let Some(top) = candidates.first() {
            let mut app = (*top).clone();
            if app.started_ms.is_none() {
                app.started_ms = Some(now_ms);
            }
            return SelectedActivity::Game(app);
        }

        if let Some(media) = self
            .enabled_media()
            .filter(|track| !track.requires_music_rpc)
        {
            return SelectedActivity::Media(media.clone());
        }

        SelectedActivity::None
    }

    pub fn enabled_media(&self) -> Option<&MediaTrack> {
        let media = if self.active_scenario.is_some() {
            &self.simulated_media
        } else {
            &self.active_media
        };
        media.as_ref().filter(|track| {
            track
                .source
                .as_ref()
                .is_none_or(|source| self.activity_enabled(source))
        })
    }

    #[cfg(feature = "discord")]
    pub fn to_discord_activity(&self, now_ms: u64) -> Option<Activity> {
        self.resolve(now_ms).into_discord_activity(now_ms)
    }
}

/// Windows detection and Discord can name the same app differently.
#[cfg(feature = "discord")]
pub use yapfast_discord::game_detection::custom_games::activity_key;

#[cfg(not(feature = "discord"))]
pub fn activity_key(name: &str) -> String {
    let basename = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_lowercase();
    match basename.strip_suffix(".exe").unwrap_or(&basename) {
        "wallpaper32" | "wallpaper64" | "wallpaperui" | "webwallpaper32" | "wallpaper engine" => {
            "wallpaper engine".into()
        }
        _ => name.to_lowercase(),
    }
}

#[cfg(feature = "discord")]
impl SelectedActivity {
    pub fn into_discord_activity(self, now_ms: u64) -> Option<Activity> {
        match self {
            SelectedActivity::SharingDisabled | SelectedActivity::None => None,
            SelectedActivity::Game(app) => Some(Activity {
                kind: yapfast_discord::activity::Kind::Playing,
                name: app.display_name.into_boxed_str(),
                details: app.details.map(String::into_boxed_str),
                state: app.state.map(String::into_boxed_str),
                party: app.party,
                application_id: None,
                start: app.started_ms,
                end: None,
                large_image: app.large_image.or(app.icon_url).map(String::into_boxed_str),
                large_text: app.large_text.map(String::into_boxed_str),
                small_image: app.small_image.map(String::into_boxed_str),
                small_text: app.small_text.map(String::into_boxed_str),
                url: None,
                link: None,
                emoji: None,
            }),
            SelectedActivity::Media(track) => {
                let start_ms = track
                    .position_updated_ms
                    .unwrap_or(now_ms)
                    .saturating_sub(track.position_ms);
                let end_ms = start_ms.saturating_add(track.duration_ms);
                let player_name = track.source.as_deref().unwrap_or("Spotify");
                let is_spotify = player_name.eq_ignore_ascii_case("spotify");

                let (details, state, large_text) = if is_spotify {
                    (
                        Some(track.title.clone().into_boxed_str()),
                        Some(track.artist.clone().into_boxed_str()),
                        Some(track.album.clone().into_boxed_str()),
                    )
                } else {
                    (
                        if track.artist.is_empty() {
                            None
                        } else {
                            Some(format!("by {}", track.artist).into_boxed_str())
                        },
                        if track.album.is_empty() {
                            None
                        } else {
                            Some(format!("on {}", track.album).into_boxed_str())
                        },
                        Some(track.album.clone().into_boxed_str()),
                    )
                };

                Some(Activity {
                    kind: yapfast_discord::activity::Kind::Listening,
                    name: if is_spotify {
                        "Spotify".into()
                    } else {
                        track.title.into_boxed_str()
                    },
                    details,
                    state,
                    party: None,
                    application_id: None,
                    start: Some(start_ms),
                    end: Some(end_ms),
                    large_image: track.thumbnail_url.map(String::into_boxed_str),
                    large_text,
                    small_image: Some("icons/volume-2.svg".into()),
                    small_text: if track.is_playing {
                        Some("Playing".into())
                    } else {
                        Some("Paused".into())
                    },
                    url: None,
                    link: track.url.map(String::into_boxed_str),
                    emoji: None,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_game_selection() {
        let mut manager = ActivityManager::new();
        let app = AppIdentity {
            id: "minecraft.exe".into(),
            display_name: "Minecraft".into(),
            icon_url: None,
            running: true,
            enabled: true,
            priority: 0,
            started_ms: Some(1000),
            details: None,
            state: None,
            party: None,
            large_image: None,
            large_text: None,
            small_image: None,
            small_text: None,
        };
        manager.update_detected_processes(vec![app], 1000);
        let selected = manager.resolve(1000);
        match selected {
            SelectedActivity::Game(app) => assert_eq!(app.display_name, "Minecraft"),
            _ => panic!("Expected Minecraft to be selected"),
        }
    }

    #[test]
    fn three_games_simultaneous_and_selective_disable() {
        let mut manager = ActivityManager::new();
        let now = 5000;
        let v = AppIdentity {
            id: "valorant.exe".into(),
            display_name: "VALORANT".into(),
            icon_url: None,
            running: true,
            enabled: true,
            priority: 10,
            started_ms: Some(now - 1000),
            details: None,
            state: None,
            party: None,
            large_image: None,
            large_text: None,
            small_image: None,
            small_text: None,
        };
        let m = AppIdentity {
            id: "minecraft.exe".into(),
            display_name: "Minecraft".into(),
            icon_url: None,
            running: true,
            enabled: true,
            priority: 5,
            started_ms: Some(now - 2000),
            details: None,
            state: None,
            party: None,
            large_image: None,
            large_text: None,
            small_image: None,
            small_text: None,
        };
        let s = AppIdentity {
            id: "subnautica.exe".into(),
            display_name: "Subnautica".into(),
            icon_url: None,
            running: true,
            enabled: true,
            priority: 0,
            started_ms: Some(now - 3000),
            details: None,
            state: None,
            party: None,
            large_image: None,
            large_text: None,
            small_image: None,
            small_text: None,
        };
        manager.update_detected_processes(vec![v, m, s], now);

        // Initially, VALORANT is top priority
        match manager.resolve(now) {
            SelectedActivity::Game(app) => assert_eq!(app.display_name, "VALORANT"),
            _ => panic!("Expected VALORANT"),
        }

        // User disables VALORANT and Minecraft
        manager.set_rule("valorant.exe", false, 10, None);
        manager.set_rule("minecraft.exe", false, 5, None);

        // Subnautica must remain the deterministic selection
        match manager.resolve(now) {
            SelectedActivity::Game(app) => assert_eq!(app.display_name, "Subnautica"),
            _ => panic!("Expected Subnautica to be selected"),
        }

        // Subnautica closes -> fallback to None
        manager.update_detected_processes(vec![], now);
        assert_eq!(manager.resolve(now), SelectedActivity::None);
    }

    #[test]
    fn global_sharing_suppression() {
        let mut manager = ActivityManager::new();
        let app = AppIdentity {
            id: "game.exe".into(),
            display_name: "Game".into(),
            icon_url: None,
            running: true,
            enabled: true,
            priority: 0,
            started_ms: Some(100),
            details: None,
            state: None,
            party: None,
            large_image: None,
            large_text: None,
            small_image: None,
            small_text: None,
        };
        manager.update_detected_processes(vec![app], 100);
        manager.set_global_sharing(false);
        assert_eq!(manager.resolve(100), SelectedActivity::SharingDisabled);
    }

    #[test]
    fn deterministic_scenarios() {
        let mut manager = ActivityManager::new();
        for i in 1..=10 {
            let scenario = SimulatedScenario::from_index(i).unwrap();
            manager.set_simulated_scenario(Some(scenario));
            let res = manager.resolve(10_000);
            match scenario {
                SimulatedScenario::NoActivity | SimulatedScenario::AllGamesDisabled => {
                    assert_eq!(res, SelectedActivity::None)
                }
                SimulatedScenario::MusicPlaying | SimulatedScenario::MusicPaused => {
                    assert!(matches!(res, SelectedActivity::Media(_)))
                }
                _ => assert!(matches!(res, SelectedActivity::Game(_))),
            }
        }
    }

    #[test]
    fn live_media_priority_and_formatting() {
        let mut manager = ActivityManager::new();
        let now = 500_000;

        // 1. Live music from SoundCloud
        let soundcloud_track = MediaTrack {
            requires_music_rpc: false,
            title: "Bass Cannon".into(),
            artist: "Excision".into(),
            album: "X Rated".into(),
            duration_ms: 210_000,
            position_ms: 30_000,
            position_updated_ms: None,
            is_playing: true,
            thumbnail_url: None,
            thumbnail: None,
            url: None,
            source: Some("SoundCloud".into()),
        };
        manager.set_media_track(Some(soundcloud_track));

        // When no game runs, SoundCloud media is selected
        match manager.resolve(now) {
            SelectedActivity::Media(track) => {
                assert_eq!(track.title, "Bass Cannon");
                assert_eq!(track.source.as_deref(), Some("SoundCloud"));
            }
            _ => panic!("Expected Media activity"),
        }

        #[cfg(feature = "discord")]
        {
            let act = manager.to_discord_activity(now).unwrap();
            // Distinct title and formatting without falsely claiming Spotify
            assert_eq!(&*act.name, "Bass Cannon");
            assert_eq!(act.details.as_deref(), Some("by Excision"));
            assert_eq!(act.state.as_deref(), Some("on X Rated"));
            assert_eq!(act.heading(), "Listening to Bass Cannon");
        }

        // 2. Game starts -> Game takes precedence over music
        let game = AppIdentity {
            id: "factorio.exe".into(),
            display_name: "Factorio".into(),
            icon_url: None,
            running: true,
            enabled: true,
            priority: 0,
            started_ms: Some(now),
            details: None,
            state: None,
            party: None,
            large_image: None,
            large_text: None,
            small_image: None,
            small_text: None,
        };
        manager.update_detected_processes(vec![game], now);
        match manager.resolve(now) {
            SelectedActivity::Game(app) => assert_eq!(app.display_name, "Factorio"),
            _ => panic!("Game must take precedence over media"),
        }

        // 3. Game closes -> Music restored
        manager.update_detected_processes(vec![], now);
        match manager.resolve(now) {
            SelectedActivity::Media(track) => assert_eq!(track.title, "Bass Cannon"),
            _ => panic!("Media must be restored once game closes"),
        }

        // Paused media stays selected until the source clears it.
        let mut paused_track = manager.active_media.clone().unwrap();
        paused_track.is_playing = false;
        manager.set_media_track(Some(paused_track));
        assert!(
            matches!(manager.resolve(now), SelectedActivity::Media(track) if !track.is_playing && track.position_ms == 30_000)
        );

        // 5. Global sharing turned off -> SharingDisabled
        let mut playing_track = manager.active_media.clone().unwrap();
        playing_track.is_playing = true;
        playing_track.position_ms = 32_000;
        manager.set_media_track(Some(playing_track));
        assert!(
            matches!(manager.resolve(now), SelectedActivity::Media(track) if track.is_playing && track.position_ms == 32_000)
        );
        manager.set_media_track(None);
        assert_eq!(manager.resolve(now), SelectedActivity::None);
        manager.set_global_sharing(false);
        assert_eq!(manager.resolve(now), SelectedActivity::SharingDisabled);
    }
}
