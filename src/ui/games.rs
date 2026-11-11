//! The game the Discord account shares: the switch for sharing it, the
//! games this device names itself, and how long the game shown has run.

use super::*;

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
            let key = id.to_lowercase();
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
                .set_rule(&id, enabled, priority, custom.clone());
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
                        executable: id.to_string(),
                        name: custom.unwrap_or_else(|| id.to_string()),
                        enabled,
                        priority,
                        custom_name: None,
                    });
            }
            adapter.layout.write(&adapter.root);
            refresh_game_activity(&mut adapter, &window);
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
                            let mut ad = adapter.borrow_mut();
                            ad.activity_manager.set_media_track(track);
                            refresh_game_activity(&mut ad, &window);
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

    // Sync live playing application (e.g. from Discord RPC or local game detection)
    if let Some(playing) = adapter.app.state.discord_playing.as_ref() {
        let key = playing.name.to_lowercase();
        let rule_enabled = adapter
            .activity_manager
            .rules
            .get(&key)
            .map(|r| r.enabled)
            .unwrap_or(true);
        let rule_priority = adapter
            .activity_manager
            .rules
            .get(&key)
            .map(|r| r.priority)
            .unwrap_or(0);
        let custom_name = adapter
            .activity_manager
            .rules
            .get(&key)
            .and_then(|r| r.custom_name.clone());

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
                if app.started_ms.is_none() {
                    app.started_ms = playing.started_ms;
                }
            })
            .or_insert_with(|| yapfast_core::activity::AppIdentity {
                id: key,
                display_name: custom_name.unwrap_or_else(|| playing.name.clone()),
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

    // Populate detected app rows for Slint
    let preferred = adapter.activity_manager.preferred_app_id.as_deref();
    let app_rows: Vec<DetectedAppRow> = adapter
        .activity_manager
        .registered_apps
        .values()
        .map(|app| DetectedAppRow {
            id: app.id.as_str().into(),
            name: app.display_name.as_str().into(),
            executable: app.id.as_str().into(),
            running: app.running,
            enabled: app.enabled,
            is_primary: preferred == Some(app.id.to_lowercase().as_str()),
            priority: app.priority as i32,
        })
        .collect();
    window.set_detected_apps(ModelRc::new(VecModel::from(app_rows)));
    window.set_active_simulated_scenario(adapter.layout.active_simulated_scenario.unwrap_or(0));

    // Resolve current activity
    let now = now_ms();
    #[cfg(feature = "discord")]
    let discord_activity = adapter.activity_manager.to_discord_activity(now);
    #[cfg(feature = "discord")]
    if let Some(act) = discord_activity {
        window.set_game_activity_preview(profile::activity_row(&act));
        window.set_has_game_activity_preview(true);
        let status = if adapter.activity_manager.active_scenario.is_some() {
            "SIMULATED SCENARIO (TEST/DEMO)".into()
        } else if adapter.app.state.discord_show_game {
            "ACTIVE (DISCORD SHARING ON)".into()
        } else {
            "LOCAL PREVIEW (SHARING DISABLED)".into()
        };
        window.set_game_activity_preview_status(status);
    } else {
        window.set_has_game_activity_preview(false);
    }
}

/// Saves the custom games and shows them, and detection takes them.
fn changed(adapter: &mut Adapter, window: Option<YapfastWindow>) {
    adapter.layout.write(&adapter.root);
    if let Some(window) = window {
        adapter.layout.apply_custom_games(&window);
        refresh_game_activity(adapter, &window);
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
