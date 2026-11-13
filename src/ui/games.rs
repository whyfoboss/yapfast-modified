//! The game the Discord account shares: the switch for sharing it, the
//! games this device names itself, and how long the game shown has run.

use super::*;
#[cfg(feature = "discord")]
use yapfast_core::yapfast_discord;

pub(super) fn bind(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_show_game_toggled(move |shown| {
        if let Some(window) = weak.upgrade() {
            let mut ad = state.borrow_mut();
            ad.activity_manager.set_global_sharing(shown);
            window.set_show_game(shown);
            if ad.app.state.discord_user.is_some() {
                ad.action(&window, Action::ShowGame(shown));
            }
            refresh_game_activity(&mut ad, &window);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_game_duration_due(move || {
        if let Some(window) = weak.upgrade() {
            let started = state
                .borrow()
                .app
                .state
                .discord_playing
                .as_ref()
                .and_then(|playing| playing.started_ms);
            window.set_game_activity_duration(
                started
                    .map(|started| played_for(now_ms().saturating_sub(started)))
                    .unwrap_or_default()
                    .into(),
            );
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_save_game_executable(move |index, executable, name| {
        let (executable, name) = (executable.trim(), name.trim());
        if executable.is_empty()
            || name.is_empty()
            || executable.len() > 4096
            || name.chars().count() > 128
            || executable.chars().any(char::is_control)
            || name.chars().any(char::is_control)
        {
            return false;
        }
        let mut adapter = state.borrow_mut();
        let games = &mut adapter.layout.game_executables;
        let editing = usize::try_from(index).ok().filter(|&i| i < games.len());
        // Another entry for the same executable gives way to this one.
        let same = games
            .iter()
            .position(|(other, _)| other == executable)
            .filter(|&same| Some(same) != editing);
        let entry = (executable.to_owned(), name.to_owned());
        match (editing, same) {
            (Some(index), same) => {
                games[index] = entry;
                if let Some(same) = same {
                    games.remove(same);
                }
            }
            (None, Some(same)) => games[same] = entry,
            (None, None) => games.push(entry),
        }
        // Sync with rules and ActivityManager
        let key = executable.to_lowercase();
        adapter
            .activity_manager
            .set_rule(executable, true, 0, Some(name.to_owned()));
        if let Some(rule) = adapter
            .layout
            .game_activity_rules
            .iter_mut()
            .find(|r| r.executable.to_lowercase() == key)
        {
            rule.name = name.to_owned();
            rule.custom_name = Some(name.to_owned());
        } else {
            adapter
                .layout
                .game_activity_rules
                .push(yapfast_core::activity::GameActivityRule {
                    executable: executable.to_owned(),
                    name: name.to_owned(),
                    enabled: true,
                    priority: 0,
                    custom_name: Some(name.to_owned()),
                });
        }
        changed(&mut adapter, weak.upgrade());
        true
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_remove_game_executable(move |index| {
        let mut adapter = state.borrow_mut();
        if let Ok(index) = usize::try_from(index)
            && index < adapter.layout.game_executables.len()
        {
            let (exe, _) = adapter.layout.game_executables.remove(index);
            let key = exe.to_lowercase();
            adapter.activity_manager.remove_app(&key);
            adapter
                .layout
                .game_activity_rules
                .retain(|r| r.executable.to_lowercase() != key);
            if adapter.layout.preferred_activity_id.as_deref() == Some(&key) {
                adapter.layout.preferred_activity_id = None;
            }
            changed(&mut adapter, weak.upgrade());
        }
    });

    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_toggle_app_activity(move |id, enabled| {
        if let Some(window) = weak.upgrade() {
            let mut adapter = state.borrow_mut();
            let identity = adapter
                .activity_manager
                .rule_for(&id)
                .map_or_else(|| id.to_string(), |rule| rule.executable.clone());
            let key = identity.to_lowercase();
            let priority = adapter
                .activity_manager
                .rules
                .get(&key)
                .map(|r| r.priority)
                .unwrap_or(0);
            let custom = adapter
                .activity_manager
                .rules
                .get(&key)
                .and_then(|r| r.custom_name.clone());
            adapter
                .activity_manager
                .set_rule(&identity, enabled, priority, custom.clone());
            if let Some(rule) = adapter
                .layout
                .game_activity_rules
                .iter_mut()
                .find(|r| r.executable.to_lowercase() == key)
            {
                rule.enabled = enabled;
            } else {
                adapter
                    .layout
                    .game_activity_rules
                    .push(yapfast_core::activity::GameActivityRule {
                        executable: identity,
                        name: custom.unwrap_or_else(|| id.to_string()),
                        enabled,
                        priority,
                        custom_name: None,
                    });
            }
            tracing::debug!(target: "yapfast_media", enabled, "per-app activity rule changed");
            changed(&mut adapter, Some(window));
        }
    });

    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_set_primary_app_activity(move |id| {
        if let Some(window) = weak.upgrade() {
            let mut adapter = state.borrow_mut();
            let key = id.to_lowercase();
            let next = if adapter.layout.preferred_activity_id.as_deref() == Some(&key) {
                None
            } else {
                Some(key)
            };
            adapter.layout.preferred_activity_id = next.clone();
            adapter.activity_manager.set_preferred_app(next);
            adapter.layout.write(&adapter.root);
            refresh_game_activity(&mut adapter, &window);
        }
    });

    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_remove_app_activity(move |id| {
        if let Some(window) = weak.upgrade() {
            let mut adapter = state.borrow_mut();
            let key = id.to_lowercase();
            adapter.activity_manager.remove_app(&key);
            adapter
                .layout
                .game_activity_rules
                .retain(|r| r.executable.to_lowercase() != key);
            adapter
                .layout
                .game_executables
                .retain(|(exe, _)| exe.to_lowercase() != key);
            if adapter.layout.preferred_activity_id.as_deref() == Some(&key) {
                adapter.layout.preferred_activity_id = None;
            }
            changed(&mut adapter, Some(window));
        }
    });

    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_select_simulated_scenario(move |index| {
        if let Some(window) = weak.upgrade() {
            let mut adapter = state.borrow_mut();
            let scenario = if index <= 0 {
                None
            } else {
                yapfast_core::activity::SimulatedScenario::from_index(index)
            };
            adapter.layout.active_simulated_scenario = scenario.map(|s| s as i32);
            adapter.activity_manager.set_simulated_scenario(scenario);
            adapter.layout.write(&adapter.root);
            refresh_game_activity(&mut adapter, &window);
        }
    });

    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_refresh_game_activity(move || {
        if let Some(window) = weak.upgrade() {
            refresh_game_activity(&mut state.borrow_mut(), &window);
        }
    });

    #[cfg(windows)]
    {
        let (tx, mut rx) = tokio::sync::watch::channel(None);
        match yapfast_core::activity::windows_smtc::winrt::WindowsSmtcListener::start(tx) {
            Ok(listener) => {
                let weak_adapter = Rc::downgrade(adapter);
                let weak_window = window.as_weak();
                let _ = slint::spawn_local(async move {
                    let _keep_alive = listener;
                    while rx.changed().await.is_ok() {
                        let track = rx.borrow().clone();
                        if let (Some(window), Some(adapter)) =
                            (weak_window.upgrade(), weak_adapter.upgrade())
                        {
                            apply_media_update(&mut adapter.borrow_mut(), &window, track);
                        }
                    }
                });
            }
            Err(error) => {
                tracing::warn!(%error, "Windows SMTC media detection unavailable");
            }
        }
    }

    refresh_game_activity(&mut adapter.borrow_mut(), window);
}

/// Refreshes detected applications list, active scenario, and Rich Presence preview.
#[cfg(any(windows, test))]
pub(super) fn apply_media_update(
    adapter: &mut Adapter,
    window: &YapfastWindow,
    mut track: Option<yapfast_core::activity::MediaTrack>,
) {
    if let Some(track) = track.as_mut() {
        track.position_updated_ms.get_or_insert_with(now_ms);
    }
    adapter.activity_manager.set_media_track(track);
    refresh_game_activity(adapter, window);
    adapter.refresh_own_activity(window);
    tracing::debug!(target: "yapfast_media", source = "smtc", ui_at_ms = now_ms(), "activity UI updated");
}

pub(super) fn refresh_game_activity(adapter: &mut Adapter, window: &YapfastWindow) {
    // If a Discord user is logged in, sync global sharing state from Discord cloud settings
    if adapter.app.state.discord_user.is_some() {
        adapter
            .activity_manager
            .set_global_sharing(adapter.app.state.discord_show_game);
        window.set_show_game(adapter.app.state.discord_show_game);
    } else {
        window.set_show_game(adapter.activity_manager.global_sharing_enabled);
    }

    // Ensure configured rules exist in registered_apps
    for rule in &adapter.layout.game_activity_rules {
        let key = rule.executable.to_lowercase();
        adapter
            .activity_manager
            .registered_apps
            .entry(key)
            .or_insert_with(|| yapfast_core::activity::AppIdentity {
                id: rule.executable.clone(),
                display_name: rule
                    .custom_name
                    .clone()
                    .unwrap_or_else(|| rule.name.clone()),
                icon_url: Some("icons/play.svg".into()),
                running: false,
                enabled: rule.enabled,
                priority: rule.priority,
                started_ms: None,
                details: None,
                state: None,
                party: None,
                large_image: None,
                large_text: None,
                small_image: None,
                small_text: None,
            });
    }

    sync_live_game(adapter);
    #[cfg(feature = "discord")]
    publish_music_playback(adapter);

    // Populate detected app rows for Slint
    let preferred = adapter.activity_manager.preferred_app_id.as_deref();
    let app_rows: Vec<DetectedAppRow> = adapter
        .activity_manager
        .registered_apps
        .values()
        .filter(|app| {
            adapter
                .activity_manager
                .rule_for(&app.id)
                .is_none_or(|rule| app.running || app.id.eq_ignore_ascii_case(&rule.executable))
        })
        .map(|app| DetectedAppRow {
            id: app.id.as_str().into(),
            name: app.display_name.as_str().into(),
            executable: app.id.as_str().into(),
            running: app.running,
            enabled: adapter
                .activity_manager
                .rule_for(&app.id)
                .map_or(app.enabled, |rule| rule.enabled),
            is_primary: preferred == Some(app.id.to_lowercase().as_str()),
            priority: app.priority as i32,
        })
        .collect();
    if !window
        .get_detected_apps()
        .iter()
        .eq(app_rows.iter().cloned())
    {
        window.set_detected_apps(ModelRc::new(VecModel::from(app_rows)));
    }
    window.set_active_simulated_scenario(adapter.layout.active_simulated_scenario.unwrap_or(0));

    // Resolve current activity
    #[cfg(feature = "discord")]
    if let Some(row) = local_activity_row(
        &adapter.activity_manager,
        adapter.app.state.discord_playing.as_ref(),
        now_ms(),
    ) {
        if !window.get_has_game_activity_preview() || window.get_game_activity_preview() != row {
            window.set_game_activity_preview(row);
            window.set_has_game_activity_preview(true);
            tracing::debug!(target: "yapfast_media", ui_at_ms = now_ms(), "Game Activity card updated");
        }
        let status = if adapter.activity_manager.active_scenario.is_some() {
            "SIMULATED SCENARIO (TEST/DEMO)".into()
        } else if adapter.app.state.discord_show_game {
            "ACTIVE (DISCORD SHARING ON)".into()
        } else {
            "LOCAL PREVIEW (SHARING DISABLED)".into()
        };
        window.set_game_activity_preview_status(status);
    } else if window.get_has_game_activity_preview() {
        window.set_game_activity_preview(Default::default());
        window.set_has_game_activity_preview(false);
        tracing::debug!(target: "yapfast_media", ui_at_ms = now_ms(), "Game Activity card cleared");
    }
}

pub(super) fn sync_live_game(adapter: &mut Adapter) {
    // Sync live playing application (e.g. from Discord RPC or local game detection)
    if let Some(playing) = adapter
        .app
        .state
        .discord_playing
        .as_ref()
        .filter(|playing| {
            playing
                .activity
                .as_deref()
                .is_none_or(|activity| activity.eligible_for_own_presence())
        })
    {
        let rule = adapter.activity_manager.rule_for(&playing.name);
        let key = rule.map_or_else(
            || playing.name.to_lowercase(),
            |rule| rule.executable.to_lowercase(),
        );
        let rule_enabled = rule.is_none_or(|rule| rule.enabled);
        let rule_priority = rule.map_or(0, |rule| rule.priority);
        let display_name = rule
            .and_then(|rule| rule.custom_name.as_ref())
            .unwrap_or(&playing.name)
            .clone();

        if let Some(old_key) = &adapter.activity_manager.live_rpc_app_id
            && old_key != &key
            && let Some(old_app) = adapter.activity_manager.registered_apps.get_mut(old_key)
        {
            old_app.running = false;
        }
        adapter.activity_manager.live_rpc_app_id = Some(key.clone());

        adapter
            .activity_manager
            .registered_apps
            .entry(key.clone())
            .and_modify(|app| {
                app.running = true;
                app.enabled = rule_enabled;
                app.priority = rule_priority;
                app.display_name.clone_from(&display_name);
                app.started_ms = playing.started_ms.or(app.started_ms);
            })
            .or_insert_with(|| yapfast_core::activity::AppIdentity {
                id: key,
                display_name,
                icon_url: Some("icons/play.svg".into()),
                running: true,
                enabled: rule_enabled,
                priority: rule_priority,
                started_ms: playing.started_ms,
                details: None,
                state: None,
                party: None,
                large_image: None,
                large_text: None,
                small_image: None,
                small_text: None,
            });
    } else if let Some(old_key) = adapter.activity_manager.live_rpc_app_id.take()
        && let Some(old_app) = adapter.activity_manager.registered_apps.get_mut(&old_key)
    {
        old_app.running = false;
    }
}

#[cfg(feature = "discord")]
fn music_playback(
    track: &yapfast_core::activity::MediaTrack,
    now: u64,
) -> yapfast_discord::own_activity::MusicPlayback {
    yapfast_discord::own_activity::MusicPlayback {
        title: track.title.clone(),
        artist: track.artist.clone(),
        duration_ms: track.duration_ms,
        start_ms: track.is_playing.then(|| {
            track
                .position_updated_ms
                .unwrap_or(now)
                .saturating_sub(track.position_ms)
        }),
    }
}

#[cfg(feature = "discord")]
pub(super) fn publish_music_playback(adapter: &mut Adapter) {
    if adapter.app.state.discord_online
        && let Some(backend) = &adapter.backend
    {
        let playback = adapter
            .activity_manager
            .active_media
            .as_ref()
            .filter(|track| {
                track
                    .source
                    .as_deref()
                    .is_none_or(|source| adapter.activity_manager.activity_enabled(source))
            })
            .map(|track| music_playback(track, now_ms()));
        if playback != adapter.published_music
            && backend
                .send(Command::Discord(
                    yapfast_core::chat::DiscordCommand::MusicPlayback(playback.clone()),
                ))
                .is_ok()
        {
            adapter.published_music = playback;
        }
    }
}

#[cfg(feature = "discord")]
pub(super) fn own_activity_rows(
    manager: &yapfast_core::activity::ActivityManager,
    playing: Option<&yapfast_core::chat::Playing>,
    fallback: &yapfast_core::chat::Activities,
) -> ModelRc<ActivityRow> {
    use yapfast_core::activity::activity_key;

    if !manager.global_sharing_enabled {
        return ModelRc::default();
    }
    let now = now_ms();
    let mut rows: Vec<_> = local_activity_row(manager, playing, now)
        .into_iter()
        .collect();
    let live = playing.map(|playing| activity_key(&playing.name));
    rows.extend(
        fallback
            .iter()
            .filter(|activity| {
                manager.activity_enabled(&activity.name)
                    && activity.eligible_for_own_presence()
                    && live
                        .as_ref()
                        .is_none_or(|live| activity_key(&activity.name) != *live)
            })
            .filter_map(|activity| {
                if let Some(track) = matching_track(manager, activity, now) {
                    Some(media_row(track, Some(activity), now))
                } else if activity.application_id.is_some()
                    && yapfast_discord::activity::music_integration(&activity.name)
                {
                    // An old profile snapshot cannot keep a local music RPC alive.
                    None
                } else {
                    Some(profile::activity_row(activity))
                }
            }),
    );
    ModelRc::new(VecModel::from(rows))
}

#[cfg(feature = "discord")]
pub(super) fn local_activity_row(
    manager: &yapfast_core::activity::ActivityManager,
    playing: Option<&yapfast_core::chat::Playing>,
    now: u64,
) -> Option<crate::ui::ActivityRow> {
    use yapfast_core::activity::SelectedActivity;

    let selected = manager.resolve(now);
    if let SelectedActivity::Game(app) = &selected
        && manager.active_scenario.is_none()
        && manager
            .live_rpc_app_id
            .as_deref()
            .is_some_and(|id| id.eq_ignore_ascii_case(&app.id))
        && let Some(rpc) = playing
            .and_then(|playing| playing.activity.as_deref())
            .filter(|activity| activity.eligible_for_own_presence())
    {
        let mut row = matching_track(manager, rpc, now).map_or_else(
            || profile::activity_row(rpc),
            |track| media_row(track, Some(rpc), now),
        );
        if playing.is_some_and(|playing| app.display_name != playing.name)
            && !rpc.name.eq_ignore_ascii_case("SoundCloud")
        {
            row.name = app.display_name.as_str().into();
        }
        return Some(row);
    }
    if let SelectedActivity::Media(track) = &selected {
        return Some(media_row(track, None, now));
    }
    let activity = selected.into_discord_activity(now)?;
    let mut row = profile::activity_row(&activity);
    clear_builtin_art(&mut row);
    Some(row)
}

#[cfg(feature = "discord")]
fn matching_track<'a>(
    manager: &'a yapfast_core::activity::ActivityManager,
    rpc: &yapfast_core::chat::Activity,
    now: u64,
) -> Option<&'a yapfast_core::activity::MediaTrack> {
    let track = manager
        .enabled_media()
        .filter(|track| music_playback(track, now).matches(rpc));
    tracing::debug!(target: "yapfast_media", source = "rpc", smtc_available = manager.enabled_media().is_some(), matched = track.is_some(), paused = ?track.map(|track| !track.is_playing), position_ms = ?track.map(|track| track.position_ms), "media activity selection");
    track
}

#[cfg(feature = "discord")]
fn media_row(
    track: &yapfast_core::activity::MediaTrack,
    rpc: Option<&yapfast_core::chat::Activity>,
    now: u64,
) -> ActivityRow {
    use yapfast_core::activity::SelectedActivity;
    let media = SelectedActivity::Media(track.clone())
        .into_discord_activity(now)
        .unwrap();
    let mut row = if let Some(rpc) = rpc {
        let mut rpc = rpc.clone();
        rpc.kind = yapfast_core::chat::ActivityKind::Listening;
        rpc.start = media.start;
        rpc.end = media.end;
        if track.thumbnail_url.is_some() {
            rpc.large_image = media.large_image;
        }
        profile::activity_row(&rpc)
    } else {
        profile::activity_row(&media)
    };
    clear_builtin_art(&mut row);
    if track.source.as_deref().is_some_and(|source| source.eq_ignore_ascii_case("SoundCloud")) {
        row.heading = "LISTENING TO SOUNDCLOUD".into();
    }
    if let Some(art) = track.thumbnail.as_deref() {
        row.artwork = super::pictures::picture(art);
    }
    if !track.is_playing {
        row.heading = "PAUSED".into();
        row.paused = true;
        row.paused_position = (track.position_ms / 1000).min(i32::MAX as u64) as i32;
    }
    tracing::debug!(target: "yapfast_media", source = "smtc", paused = row.paused, position_ms = track.position_ms, "media card position");
    row
}

#[cfg(feature = "discord")]
fn clear_builtin_art(row: &mut ActivityRow) {
    if row.large_url.starts_with("icons/") {
        row.large_url = Default::default();
    }
    if row.small_url.starts_with("icons/") {
        row.small_url = Default::default();
    }
}

#[cfg(all(test, feature = "discord"))]
#[test]
fn soundcloud_matches_browser_metadata_and_holds_the_smtc_position() {
    use yapfast_core::activity::{ActivityManager, AppIdentity, MediaTrack};
    use yapfast_core::chat::{ActivityKind, Playing};
    let now = 1_800_000_000_000;
    let rpc = yapfast_discord::activity::activities(&serde_json::json!([{
        "type": 2, "name": "SoundCloud", "details": "Unusually long track…", "state": "Artist",
        "timestamps": {"start": now - 116_000, "end": now + 3_000}, "application_id": "42"
    }]))[0]
        .clone();
    assert_eq!(rpc.kind, ActivityKind::Listening);
    let mut manager = ActivityManager::new();
    manager.live_rpc_app_id = Some("soundcloud".into());
    manager.update_detected_processes(
        vec![AppIdentity {
            id: "soundcloud".into(),
            display_name: "SoundCloud".into(),
            icon_url: None,
            running: true,
            enabled: true,
            priority: 0,
            started_ms: rpc.start,
            details: None,
            state: None,
            party: None,
            large_image: None,
            large_text: None,
            small_image: None,
            small_text: None,
        }],
        now,
    );
    let playing = Playing {
        name: "SoundCloud".into(),
        started_ms: rpc.start,
        activity: Some(Arc::new(rpc)),
    };
    let mut track = MediaTrack {
        requires_music_rpc: false,
        title: "Unusually long track title".into(),
        artist: "  Artist ".into(),
        album: String::new(),
        duration_ms: 119_000,
        position_ms: 48_000,
        position_updated_ms: Some(now),
        is_playing: false,
        thumbnail_url: None,
        thumbnail: None,
        url: None,
        source: Some("Chrome".into()),
    };
    manager.set_media_track(Some(track.clone()));
    let row = local_activity_row(&manager, Some(&playing), now + 20_000).unwrap();
    assert_eq!(row.paused_position, 48);
    assert!(row.paused && !row.large_url.is_empty());
    track.artist = "Another artist".into();
    manager.set_media_track(Some(track));
    assert!(
        !local_activity_row(&manager, Some(&playing), now)
            .unwrap()
            .paused
    );
}

/// Saves the custom games and shows them, and detection takes them.
fn changed(adapter: &mut Adapter, window: Option<YapfastWindow>) {
    adapter.layout.write(&adapter.root);
    if let Some(window) = window {
        adapter.layout.apply_custom_games(&window);
        refresh_game_activity(adapter, &window);
        adapter.refresh_own_activity(&window);
        adapter.show_members(&window);
    }
}

pub(super) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// How long a game has run, as the account's menu says after "Playing for".
pub(super) fn played_for(elapsed_ms: u64) -> String {
    let minutes = elapsed_ms / 60_000;
    let (hours, minutes) = (minutes / 60, minutes % 60);
    match (hours, minutes) {
        (0, 0) => "less than a minute".into(),
        (0, minutes) => format!("{minutes} min"),
        (hours, 0) => format!("{hours} hr"),
        (hours, minutes) => format!("{hours} hr {minutes} min"),
    }
}
