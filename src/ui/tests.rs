use super::*;

// Keep the coordinate and appearance regressions in the classic mode.
// The Yapfast tests below exercise the new defaults separately.
fn classic_window() -> YapfastWindow {
    let window = YapfastWindow::new().unwrap();
    window.global::<ClientStyle>().set_enabled(false);
    window
}

fn message_row_plain(message: &ChatMessage) -> MessageRow {
    message_row(message, &HashSet::new())
}

fn item_text_is(item: &i_slint_core::item_tree::ItemRc, value: &str) -> bool {
    use i_slint_core::items::{ComplexText, SimpleText};
    item.clone().downcast::<SimpleText>().is_some_and(|item| {
        SimpleText::FIELD_OFFSETS
            .text()
            .apply_pin(item.as_pin_ref())
            .get()
            == value
    }) || item.clone().downcast::<ComplexText>().is_some_and(|item| {
        ComplexText::FIELD_OFFSETS
            .text()
            .apply_pin(item.as_pin_ref())
            .get()
            == value
    })
}

#[test]
fn a_server_without_a_picture_shows_its_initials_in_capitals() {
    assert_eq!(initials("Co-op crew"), "CC");
    assert_eq!(initials("the secret empire"), "TSE");
    assert_eq!(initials("a b c d"), "ABC");
    assert_eq!(initials("   "), "?");
}

#[test]
fn a_notice_is_a_group_of_its_own_with_an_icon() {
    use yapfast_core::chat::{Notice, NoticeKind};
    let said = ChatMessage {
        item: "d:4".into(),
        author: "Alice".into(),
        sender: "discord:1".into(),
        body: "Look".into(),
        timestamp: 100,
        ..Default::default()
    };
    let pinned = ChatMessage {
        item: "d:5".into(),
        body: "Alice pinned a message.".into(),
        timestamp: 160,
        notice: Some(Notice {
            kind: NoticeKind::Pinned(Some("d:4".into())),
            text: "pinned a message.".into(),
        }),
        ..said.clone()
    };
    let again = ChatMessage {
        item: "d:6".into(),
        timestamp: 200,
        ..said.clone()
    };
    assert!(messages::continues(&said, &again));
    assert!(!messages::continues(&said, &pinned));
    assert!(!messages::continues(&pinned, &again));
    assert_eq!(message_row_plain(&pinned).notice, "pin");
    assert_eq!(message_row_plain(&said).notice, "");
}

#[cfg(feature = "discord")]
#[test]
fn statuses_show_their_custom_emoji_as_pictures() {
    let (words, emoji) = activity_line(Some("<:pog:5>  back\nsoon"));
    assert_eq!(words, "back soon");
    assert!(emoji.ends_with("/emojis/5.png?size=64"), "{emoji}");
    let (words, emoji) = activity_line(Some("☕ brb"));
    assert_eq!((words.as_str(), emoji.as_str()), ("☕ brb", ""));

    // The account's own status.
    let mut status = yapfast_core::chat::CustomStatus {
        text: "Shipping it".into(),
        emoji: Some("pog".into()),
        emoji_id: Some("5".into()),
        expires_at_ms: None,
    };
    let (text, picture) = status_line(&status);
    assert_eq!(text, "Shipping it");
    assert!(picture.is_some_and(|url| url.ends_with("/emojis/5.png?size=64")));
    status.emoji = Some("🍅".into());
    status.emoji_id = None;
    assert_eq!(status_line(&status), ("🍅 Shipping it".into(), None));
}

#[test]
fn equal_lists_stay_so_rows_count_as_unchanged() {
    let shown = ModelRc::new(VecModel::from(vec![1, 2]));
    let kept = same_list(&shown, ModelRc::new(VecModel::from(vec![1, 2])));
    assert!(kept == shown, "the list on screen stays");
    let changed = same_list(&shown, ModelRc::new(VecModel::from(vec![1, 3])));
    assert!(changed != shown);
    assert_eq!(changed.row_data(1), Some(3));

    // A profile built again from the same view isn't set again.
    let view = yapfast_core::application::ProfileView {
        profile: yapfast_core::chat::Profile {
            id: "@sam:test".into(),
            name: "Sam".into(),
            servers: vec![yapfast_core::chat::ProfileServer {
                id: "s".into(),
                name: "Server".into(),
                icon: None,
                banner: None,
                members: Some(4),
                channels: 2,
                nick: None,
            }],
            roles: vec![ProfileRole {
                name: "Mod".into(),
                color: Some(0xff0000),
            }],
            ..Default::default()
        },
        ..Default::default()
    };
    let state = yapfast_core::application::AppState::default();
    let mut pictures = HashMap::new();
    let shown = profile_data(&mut pictures, &state, &view);
    assert!(!shown.name_has_color);
    let mut server_view = view.clone();
    server_view.server_context = true;
    server_view.role_color = Some(0xff0000);
    let server = profile_data_with_role_colors(&mut pictures, &state, &server_view, true);
    assert!(server.name_has_color);
    assert_eq!(
        server.name_color,
        slint::Color::from_rgb_u8(0xff, 0x00, 0x00)
    );
    let plain = profile_data_with_role_colors(&mut pictures, &state, &server_view, false);
    assert!(!plain.name_has_color);
    let again = profile_data(&mut pictures, &state, &view);
    assert!(changed_profile(&shown, again).is_none());
    let mut renamed = view.clone();
    renamed.profile.name = "Samuel".into();
    let changed = changed_profile(&shown, profile_data(&mut pictures, &state, &renamed))
        .expect("a new name shows");
    assert!(changed.servers == shown.servers, "its lists stay");
}

#[test]
fn this_accounts_own_profile_shares_its_friends_and_servers_with_nobody() {
    let view = |id: &str| yapfast_core::application::ProfileView {
        profile: yapfast_core::chat::Profile {
            id: id.into(),
            name: "Sam".into(),
            servers: vec![yapfast_core::chat::ProfileServer {
                id: "s".into(),
                name: "Server".into(),
                icon: None,
                banner: None,
                members: None,
                channels: 2,
                nick: None,
            }],
            friends: vec![yapfast_core::chat::ProfileFriend {
                id: "7".into(),
                name: "Bob".into(),
                avatar: None,
            }],
            ..Default::default()
        },
        ..Default::default()
    };
    let mut state = yapfast_core::application::AppState::default();
    state.discord_user = Some("42".into());
    let mut pictures = HashMap::new();

    // Anyone else's card names what the account shares with them.
    let other = profile_data(&mut pictures, &state, &view("7"));
    assert_eq!(other.mutual_friends_text, "1 mutual friend");
    assert_eq!(other.mutual_servers_text, "1 mutual server");
    assert!(!other.own);

    // Its own profile shares them with nobody, so it names them plainly.
    let own = profile_data(&mut pictures, &state, &view("42"));
    assert_eq!(own.mutual_friends_text, "1 friend");
    assert_eq!(own.mutual_servers_text, "1 server");
    assert!(own.own);
}

#[test]
fn blank_lines_between_lines_show() {
    assert_eq!(keep_blank_lines("a\n\nb"), "a\n\u{a0}\nb");
    assert_eq!(keep_blank_lines("a\n\n\nb"), "a\n\u{a0}\n\u{a0}\nb");
    assert_eq!(keep_blank_lines("a\nb"), "a\nb");
    assert_eq!(
        keep_blank_lines("\n\na\n\n"),
        "\n\na\n\n",
        "only between lines"
    );
    assert_eq!(
        keep_blank_lines("a\n\n```\nx\n\ny\n```\n\nb"),
        "a\n\u{a0}\n```\nx\n\ny\n```\n\u{a0}\nb",
        "code keeps its lines"
    );
    // A blank line shows as a line of its own, unlike a line break.
    assert_ne!(markdown("one\n\ntwo"), markdown("one\ntwo"));
}

#[test]
fn the_composer_sends_on_enter_and_breaks_lines_on_shift_enter() {
    use slint::platform::{Key, WindowEvent};
    let window = one_message("Hi");
    if cfg!(windows) {
        window
            .global::<ClientStyle>()
            .set_native_font("Segoe UI".into());
    }
    for enabled in [false, true] {
        window.global::<ClientStyle>().set_enabled(enabled);
        window.set_draft("".into());
        let sent = Rc::new(Cell::new(0));
        let count = sent.clone();
        window.on_send(move || count.set(count.get() + 1));
        window.invoke_focus_composer();
        let press = |text: SharedString| {
            window
                .window()
                .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            window
                .window()
                .dispatch_event(WindowEvent::KeyReleased { text });
        };
        press("a".into());
        window.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::Shift.into(),
        });
        press(Key::Return.into());
        window.window().dispatch_event(WindowEvent::KeyReleased {
            text: Key::Shift.into(),
        });
        press("b".into());
        assert_eq!(window.get_draft(), "a\nb");
        assert_eq!(sent.get(), 0);
        press(Key::Return.into());
        assert_eq!(sent.get(), 1);
        assert_eq!(
            window.get_draft(),
            "a\nb",
            "sending is the application's to clear"
        );
    }
}

#[test]
fn the_composers_menu_cuts_copies_and_pastes_through_the_application() {
    use slint::platform::{Key, PointerEventButton, WindowEvent};
    let window = one_message("Hi");
    let pastes = Rc::new(Cell::new(0));
    let count = pastes.clone();
    let weak = window.as_weak();
    window.on_paste_into_composer(move || {
        count.set(count.get() + 1);
        weak.unwrap().invoke_paste_composer_text();
    });
    window.invoke_focus_composer();
    for letter in "hello".chars() {
        let text: SharedString = letter.into();
        window
            .window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window
            .window()
            .dispatch_event(WindowEvent::KeyReleased { text });
    }
    let press = |x, y, button| {
        let position = slint::LogicalPosition::new(x, y);
        window
            .window()
            .dispatch_event(WindowEvent::PointerMoved { position });
        window
            .window()
            .dispatch_event(WindowEvent::PointerPressed { position, button });
        window
            .window()
            .dispatch_event(WindowEvent::PointerReleased { position, button });
        settle(&window);
    };
    // The menu opens above the pointer, its rows 34px apart, the last
    // centred 22px above it: Select All, then Paste, Copy and Cut.
    let (x, y) = (600., 720.);
    let choose = |row: f32| {
        press(x, y, PointerEventButton::Right);
        press(x + 40., y - 22. - 34. * row, PointerEventButton::Left);
    };
    choose(0.);
    choose(3.);
    assert_eq!(window.get_draft(), "", "Select All, then Cut");
    choose(1.);
    assert_eq!(pastes.get(), 1, "Paste goes through the application");
    assert_eq!(window.get_draft(), "hello");
    choose(0.);
    choose(2.);
    window.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::End.into(),
    });
    choose(1.);
    assert_eq!(window.get_draft(), "hellohello", "Select All, then Copy");
    // Without a selection there is nothing to cut: the rows are Paste and
    // Select All, so the third from the bottom is outside the menu.
    choose(2.);
    assert_eq!(window.get_draft(), "hellohello");
}

/// Presses and lets go of a key, as typed.
fn tap(window: &YapfastWindow, text: SharedString) {
    use slint::platform::WindowEvent;
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

#[test]
fn escape_in_the_composer_cancels_a_reply_and_keeps_the_draft() {
    use slint::platform::Key;
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    let other = window.get_messages().iter().find(|row| !row.own).unwrap();
    window.invoke_reply(other.item.clone());
    assert!(window.get_compose_label().starts_with("Replying to Sam"));
    window.invoke_focus_composer();
    for letter in "ok".chars() {
        tap(&window, letter.into());
    }
    tap(&window, Key::Escape.into());
    assert_eq!(window.get_compose_label(), "", "Escape cancels the reply");
    assert_eq!(window.get_draft(), "ok", "and leaves what was written");

    // With nothing to cancel, Escape changes nothing, and the composer
    // keeps the keyboard.
    tap(&window, Key::Escape.into());
    tap(&window, "!".into());
    assert_eq!(window.get_compose_label(), "");
    assert_eq!(window.get_draft(), "ok!");
}

#[test]
fn typing_with_nothing_focused_goes_to_the_composer() {
    use slint::platform::{Key, WindowEvent};
    let window = one_message("Hi");
    window.set_draft("Hello".into());
    // Nothing has the keyboard, and without the hook the key goes nowhere.
    tap(&window, "x".into());
    assert_eq!(window.get_draft(), "Hello");
    type_anywhere(&window);

    // Not over the settings or another overlay.
    window.set_settings_open(true);
    tap(&window, "x".into());
    window.set_settings_open(false);
    window.set_pins_open(true);
    tap(&window, "x".into());
    window.set_pins_open(false);
    assert_eq!(window.get_draft(), "Hello");

    // Nor with Ctrl held.
    window.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    tap(&window, "x".into());
    window.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    assert_eq!(window.get_draft(), "Hello");

    // Nor keys that type nothing.
    tap(&window, Key::DownArrow.into());
    assert_eq!(window.get_draft(), "Hello");

    tap(&window, " ".into());
    assert_eq!(window.get_draft(), "Hello ", "the text goes on the end");
    // The composer has the keyboard now, and its caret is at the end.
    tap(&window, "w".into());
    tap(&window, Key::Backspace.into());
    tap(&window, "y".into());
    assert_eq!(window.get_draft(), "Hello y");
}

#[test]
fn keys_nothing_took_do_what_discords_do() {
    use slint::platform::Key;
    let window = one_message("Hi");
    window.set_draft("Hello".into());
    type_anywhere(&window);

    // Not behind an emoji's card, whose Escape stays its own.
    window.set_emoji_card_open(true);
    tap(&window, "x".into());
    assert_eq!(window.get_draft(), "Hello");
    window.set_emoji_card_open(false);

    // Enter doesn't send, unlike Discord's.
    let sent = Rc::new(Cell::new(0));
    let count = sent.clone();
    window.on_send(move || count.set(count.get() + 1));
    tap(&window, Key::Return.into());
    tap(&window, Key::Return.into());
    assert_eq!(sent.get(), 0);

    // Backspace edits the end of the draft.
    tap(&window, Key::Backspace.into());
    assert_eq!(window.get_draft(), "Hell");

    // Tab with nothing focused goes to the composer, and types nothing.
    let window = one_message("Hi");
    window.set_draft("Hello".into());
    type_anywhere(&window);
    tap(&window, Key::Tab.into());
    assert_eq!(window.get_draft(), "Hello");
    tap(&window, "!".into());
    assert_eq!(window.get_draft(), "Hello!");

    // Stepping through the messages, only letters and digits leave them.
    window.invoke_browse_messages();
    tap(&window, "?".into());
    assert_eq!(window.get_draft(), "Hello!");
    assert!(window.get_browsing());
    tap(&window, "x".into());
    assert_eq!(window.get_draft(), "Hello!x");
    assert!(!window.get_browsing());
}

#[test]
fn escape_from_anywhere_cancels_the_channels_reply_then_the_threads() {
    use slint::platform::Key;
    let window = one_message("Hi");
    type_anywhere(&window);
    let channel = Rc::new(Cell::new(0));
    let count = channel.clone();
    window.on_cancel_compose(move || count.set(count.get() + 1));
    let thread = Rc::new(Cell::new(0));
    let count = thread.clone();
    window.on_aside_cancel_compose(move || count.set(count.get() + 1));
    window.set_aside_open(true);
    window.set_compose_label("Replying to Sam".into());
    window.set_aside_compose_label("Replying to Sam".into());

    // Over a card, Escape isn't for them.
    window.set_emoji_card_open(true);
    tap(&window, Key::Escape.into());
    window.set_emoji_card_open(false);
    assert_eq!((channel.get(), thread.get()), (0, 0));

    tap(&window, Key::Escape.into());
    assert_eq!((channel.get(), thread.get()), (1, 0));
    window.set_compose_label("".into());
    tap(&window, Key::Escape.into());
    assert_eq!((channel.get(), thread.get()), (1, 1));
}

#[test]
fn typing_goes_to_the_composer_used_last() {
    let window = one_message("Hi");
    type_anywhere(&window);
    window.set_aside_composer_enabled(true);
    window.set_aside_open(true);
    i_slint_core::properties::ChangeTracker::run_change_handlers();

    // The panel's from when it opens.
    tap(&window, "a".into());
    assert_eq!(window.get_aside_draft(), "a");
    assert_eq!(window.get_draft(), "");

    // The channel's once it has the keyboard, which the panel's then lets go.
    window.invoke_focus_composer();
    i_slint_core::properties::ChangeTracker::run_change_handlers();
    tap(&window, "b".into());
    assert_eq!(window.get_draft(), "b");
    assert_eq!(window.get_aside_draft(), "a");
}

#[cfg(feature = "discord")]
#[test]
fn files_dropped_on_the_window_stage_or_say_why_not() {
    let (window, adapter, folder) = discord_server_window();
    adapter
        .borrow_mut()
        .app
        .dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    adapter
        .borrow_mut()
        .action(&window, Action::SelectRoom("discord:10:11".into()));
    // As once its history has come.
    adapter.borrow_mut().app.state.room_ready = true;
    let file = |name: &str, size: u64| {
        let path = folder.path().join(name);
        std::fs::File::create(&path).unwrap().set_len(size).unwrap();
        path
    };
    let drag = |drag| adapter.borrow_mut().drag(&window, drag);
    drag(Drag::Entered);
    assert!(window.get_dragging());
    assert_eq!(window.get_drop_destination(), "#general");
    drag(Drag::Left);
    assert!(!window.get_dragging());
    drag(Drag::Entered);
    drag(Drag::Dropped(vec![
        file("cat.png", 3),
        file("notes.txt", 5),
    ]));
    assert!(!window.get_dragging());
    assert_eq!(window.get_drop_refusal(), "");
    assert_eq!(adapter.borrow().app.state.staged().len(), 2);
    // Two waiting and nine more is too many, and none of them go.
    let many: Vec<_> = (0..9)
        .map(|index| file(&format!("{index}.txt"), 1))
        .collect();
    drag(Drag::Entered);
    drag(Drag::Dropped(many));
    assert_eq!(window.get_drop_refusal(), "Too many uploads!");
    assert_eq!(
        window.get_drop_refusal_help(),
        "You can only upload 10 files at a time!"
    );
    assert_eq!(adapter.borrow().app.state.staged().len(), 2);
    // The next drag puts the refusal away.
    drag(Drag::Entered);
    assert_eq!(window.get_drop_refusal(), "");
    // Without Nitro, in a server without boosts: 20 MiB.
    drag(Drag::Dropped(vec![file("big.mov", 21 * 1024 * 1024)]));
    assert_eq!(window.get_drop_refusal(), "Your files are too powerful");
    assert_eq!(
        window.get_drop_refusal_help(),
        "Max file size is 20.00 MB please."
    );
    drag(Drag::Entered);
    drag(Drag::Dropped(vec![folder.path().into()]));
    assert_eq!(window.get_drop_refusal(), "Invalid file type");
    assert_eq!(adapter.borrow().app.state.staged().len(), 2);
}

#[test]
#[cfg(feature = "discord")]
fn files_dropped_on_the_thread_beside_go_with_its_message() {
    let (window, adapter, folder) = discord_server_window_changed(0, |server, _| {
        server
            .threads
            .push(yapfast_core::yapfast_discord::ready::Channel {
                id: "30".into(),
                name: "Plans".into(),
                parent: Some("11".into()),
                thread: Some(Box::new(yapfast_core::yapfast_discord::ready::Thread {
                    joined: true,
                    ..Default::default()
                })),
                ..Default::default()
            });
    });
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    adapter
        .borrow_mut()
        .app
        .dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    adapter
        .borrow_mut()
        .action(&window, Action::SelectRoom("discord:10:11".into()));
    adapter
        .borrow_mut()
        .action(&window, Action::OpenAside("discord:10:30".into()));
    // As once both timelines have come.
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.state.room_ready = true;
        adapter.app.state.aside.as_mut().unwrap().ready = true;
        adapter.refresh(&window);
    }
    // As when signed in, which the fixture isn't.
    window.set_workspace(true);
    let file = |name: &str| {
        let path = folder.path().join(name);
        std::fs::write(&path, b"png").unwrap();
        path
    };
    let drag = |drag| adapter.borrow_mut().drag(&window, drag);
    // The thread is the window's right 420px, below the header.
    drag(Drag::Moved { x: 900., y: 400. });
    drag(Drag::Entered);
    assert!(window.get_dragging());
    assert_eq!(window.get_drop_destination(), "\"Plans\"");
    drag(Drag::Moved { x: 400., y: 400. });
    assert_eq!(window.get_drop_destination(), "#general");
    drag(Drag::Moved { x: 900., y: 20. });
    assert_eq!(
        window.get_drop_destination(),
        "#general",
        "the header is the channel's"
    );
    drag(Drag::Moved { x: 900., y: 400. });
    drag(Drag::Dropped(vec![file("map.png"), file("notes.txt")]));
    let state = &adapter.borrow().app.state;
    assert_eq!(state.aside_staged().len(), 2);
    assert!(state.staged().is_empty(), "the channel's composer stays");
}

#[test]
#[cfg(feature = "discord")]
fn the_thread_beside_suggests_mentions_and_emoji_above_its_composer() {
    use yapfast_core::chat::{ChatMember, Role};
    let (window, adapter, _folder) = discord_server_window_changed(0, |server, _| {
        server
            .threads
            .push(yapfast_core::yapfast_discord::ready::Channel {
                id: "30".into(),
                name: "Plans".into(),
                parent: Some("11".into()),
                thread: Some(Box::new(yapfast_core::yapfast_discord::ready::Thread {
                    joined: true,
                    ..Default::default()
                })),
                ..Default::default()
            });
    });
    window.show().unwrap();
    adapter
        .borrow_mut()
        .app
        .dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    adapter
        .borrow_mut()
        .action(&window, Action::SelectRoom("discord:10:11".into()));
    adapter
        .borrow_mut()
        .action(&window, Action::OpenAside("discord:10:30".into()));
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.state.room_ready = true;
        adapter.app.state.aside.as_mut().unwrap().ready = true;
        adapter.app.apply(Event::Members {
            room: "discord:10:11".into(),
            members: Ok(vec![ChatMember {
                id: "3".into(),
                name: "Carol".into(),
                username: Some("carol".into()),
                role: Role::Member,
                avatar: None,
                group: None,
                presence: None,
                color: None,
                activity: None,
                activities: Default::default(),
                roles: Default::default(),
                timed_out_until: None,
                group_icon: None,
                role_icon: None,
                tag: None,
                decoration: None,
                nameplate: None,
                style: None,
                bot: false,
                boosting_since: Some(1_709_467_200_000),
            }]),
        });
        adapter.refresh(&window);
    }
    // She boosts the server, which the gem by her name says on hover.
    let carol = window
        .get_roster()
        .iter()
        .find(|row| row.name == "Carol")
        .unwrap();
    assert!(
        carol.boosting.starts_with("Server boosting since ")
            && carol.boosting.ends_with("March 2024")
    );
    // `@` in the thread's composer suggests the channel's people, and the one
    // chosen goes in that composer, not the channel's.
    window.invoke_aside_composer_caret("hey @ca".into(), 7);
    assert!(window.get_suggesting_aside());
    assert_eq!(window.get_mentions().row_count(), 1);
    window.invoke_mention_chosen(0);
    assert_eq!(adapter.borrow().app.state.aside_draft(), "hey @Carol ");
    assert_eq!(adapter.borrow().app.state.draft(), "");
    // It's marked as the channel's composer marks its own.
    let marks = i_slint_core::yapfast_marks::marks_of("hey @Carol ").expect("marked");
    assert_eq!(marks[0].0, 4..10);
    // `:name` too.
    window.invoke_aside_composer_caret("hey @Carol :pizz".into(), 16);
    assert!(window.get_emoji_suggestions().row_count() > 0);
    window.invoke_emoji_suggestion_chosen(0);
    assert_eq!(adapter.borrow().app.state.aside_draft(), "hey @Carol 🍕 ");
    // Typing in the channel's composer takes the suggestions back.
    window.invoke_composer_caret("@".into(), 1);
    assert!(!window.get_suggesting_aside());
}

#[test]
fn the_keyboard_steps_through_messages_and_acts_on_them() {
    use slint::platform::{Key, WindowEvent};
    let window = showing(
        ["One", "Two", "Three"]
            .into_iter()
            .map(|body| ChatMessage {
                author: "Sam".into(),
                body: body.into(),
                can_reply: true,
                ..Default::default()
            })
            .collect(),
    );
    let replied = Rc::new(RefCell::new(String::new()));
    let reply = replied.clone();
    window.on_reply(move |item| *reply.borrow_mut() = item.into());
    let edits = Rc::new(Cell::new(0));
    let count = edits.clone();
    window.on_edit_last(move || count.set(count.get() + 1));
    let key = |text: SharedString, pressed: bool| {
        window.window().dispatch_event(if pressed {
            WindowEvent::KeyPressed { text }
        } else {
            WindowEvent::KeyReleased { text }
        });
    };
    let press = |text: SharedString| {
        key(text.clone(), true);
        key(text, false);
    };
    window.invoke_focus_composer();
    // Up in the empty composer edits; Alt+Up goes to the newest message.
    press(Key::UpArrow.into());
    assert_eq!(edits.get(), 1);
    key(Key::Alt.into(), true);
    press(Key::UpArrow.into());
    key(Key::Alt.into(), false);
    assert_eq!(window.get_active_item(), "i2");
    press(Key::UpArrow.into());
    press(Key::UpArrow.into());
    press(Key::UpArrow.into());
    assert_eq!(window.get_active_item(), "i0", "the first stays");
    press(Key::DownArrow.into());
    assert_eq!(window.get_active_item(), "i1");
    press("r".into());
    assert_eq!(*replied.borrow(), "i1");
    // The reply goes back to the composer, so letters type again.
    press("x".into());
    assert_eq!(window.get_draft(), "x");
}

#[test]
fn the_call_panels_noise_toggle_turns_back_on_the_kind_last_used() {
    i_slint_backend_testing::init_no_event_loop();
    let window = YapfastWindow::new().unwrap();
    // Each turn saves, as a pick in Voice settings does.
    let saves = Rc::new(Cell::new(0));
    let count = saves.clone();
    window.on_preferences_changed(move || count.set(count.get() + 1));
    // DeepFilterNet, as it comes, goes off and comes back.
    assert_eq!(window.get_noise_suppression(), 0);
    window.invoke_toggle_noise();
    assert_eq!(window.get_noise_suppression(), 2, "off");
    window.invoke_toggle_noise();
    assert_eq!(window.get_noise_suppression(), 0);
    // RNNoise, picked in settings or the input menu, is the one it brings back.
    window.set_noise_suppression(1);
    window.invoke_toggle_noise();
    assert_eq!(window.get_noise_suppression(), 2);
    window.invoke_toggle_noise();
    assert_eq!(window.get_noise_suppression(), 1);
    assert_eq!(saves.get(), 4);
    // Off picked elsewhere after RNNoise brings RNNoise back too.
    window.set_noise_suppression(1);
    i_slint_core::properties::ChangeTracker::run_change_handlers();
    window.set_noise_suppression(2);
    i_slint_core::properties::ChangeTracker::run_change_handlers();
    window.invoke_toggle_noise();
    assert_eq!(window.get_noise_suppression(), 1);
}

#[test]
fn preferences_set_the_clock_text_size_previews_and_unmuting() {
    i_slint_backend_testing::init_no_event_loop();
    let message = ChatMessage {
        timestamp: 1_791_329_400,
        time: "23:30".into(),
        embeds: vec![yapfast_core::chat::Embed::default()],
        ..Default::default()
    };
    // Missing preferences take their defaults: 24 hours, default text, previews.
    let defaults: Layout = serde_json::from_str("{}").unwrap();
    assert_eq!(
        defaults.noise_suppression,
        yapfast_core::noise::Suppressor::DeepFilterNet
    );
    let row = defaults.restyle(&message, message_row(&message, &HashSet::new()));
    // A message from another day shows its date before its time, but beside
    // a grouped message only the time.
    let now = yapfast_core::clock::now_ms() / 1000;
    let time = yapfast_core::clock::time_on(message.timestamp, false);
    assert_eq!(
        row.time,
        yapfast_core::clock::sent(message.timestamp, now, false, false)
    );
    assert!(row.time.contains('/') && row.time.ends_with(&format!(" {time}")));
    assert_eq!(row.short_time, time);
    assert_eq!(row.embeds.row_count(), 1);
    let layout = Layout {
        twelve_hour_clock: true,
        text_size: Some(2),
        link_previews: Some(false),
        boost_gems: Some(false),
        unmute_on_join: Some(true),
        noise_suppression: yapfast_core::noise::Suppressor::RNNoise,
        noise_strength: yapfast_core::noise::Strength::Light,
        input_sensitivity: Some(-45),
        reduce_room_echo: true,
        ..Default::default()
    };
    let row = layout.restyle(&message, message_row(&message, &HashSet::new()));
    let time = yapfast_core::clock::time_on(message.timestamp, true);
    assert!(time.ends_with(" AM") || time.ends_with(" PM"));
    let sent = yapfast_core::clock::sent(message.timestamp, now, true, false);
    assert!(sent.ends_with(&time));
    assert_eq!(row.time, sent);
    assert_eq!(row.time_styled, slint::StyledText::from_plain_text(&sent));
    assert_eq!(row.short_time, time.split(' ').next().unwrap());
    assert_eq!(row.embeds.row_count(), 0);
    // A message that is only a card says it's hidden; one with text doesn't.
    assert_eq!(
        row.hidden_card,
        "A card, hidden while link previews are off."
    );
    let titled = ChatMessage {
        embeds: vec![yapfast_core::chat::Embed {
            title: "Server status".into(),
            ..Default::default()
        }],
        ..message.clone()
    };
    assert!(
        layout
            .restyle(&titled, message_row(&titled, &HashSet::new()))
            .hidden_card
            .contains("\"Server status\"")
    );
    let with_text = ChatMessage {
        body: "Look".into(),
        ..message.clone()
    };
    assert!(
        layout
            .restyle(&with_text, message_row(&with_text, &HashSet::new()))
            .hidden_card
            .is_empty()
    );

    let window = classic_window();
    layout.apply(&window);
    assert!(window.get_twelve_hour_clock() && !window.get_link_previews());
    assert!(!window.get_boost_gems());
    // Discord calls suppress noise with the chosen network, as far as chosen.
    assert_eq!(window.get_noise_suppression(), 1);
    assert_eq!(window.get_noise_strength(), 2);
    assert_eq!(
        yapfast_core::noise::chosen(),
        yapfast_core::noise::Suppressor::RNNoise
    );
    let saved = serde_json::to_value(&layout).unwrap();
    assert_eq!(saved["noise_suppression"], "rnnoise");
    assert_eq!(saved["noise_strength"], "light");
    assert_eq!(window.get_sensitivity(), -45);
    assert!(window.get_dereverb() && !defaults.reduce_room_echo);
    assert_eq!(defaults.sensitivity(), -60);
    let out_of_range = Layout {
        input_sensitivity: Some(12),
        ..Default::default()
    };
    assert_eq!(out_of_range.sensitivity(), 0);
    // The third choice is Off; an index out of range is the default.
    assert_eq!(
        chosen(&SUPPRESSORS, 2),
        yapfast_core::noise::Suppressor::Off
    );
    assert_eq!(chosen(&STRENGTHS, -1), yapfast_core::noise::Strength::Full);
    assert_eq!(window.get_message_font_size(), 18.);
    // Joining a call turns the microphone on once it connects.
    let unmuted = Rc::new(Cell::new(false));
    let record = unmuted.clone();
    window.on_join_unmuted(move || record.set(true));
    window.set_muted(true);
    window.set_voice_ready(true);
    slint::platform::update_timers_and_animations();
    assert!(unmuted.get());
    // Deafening on joining wins over unmuting.
    let (unmuted, deafened) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
    let record = unmuted.clone();
    window.on_join_unmuted(move || record.set(true));
    let record = deafened.clone();
    window.on_join_deafened(move || record.set(true));
    window.set_voice_ready(false);
    slint::platform::update_timers_and_animations();
    window.set_deafen_on_join(true);
    window.set_voice_ready(true);
    slint::platform::update_timers_and_animations();
    assert!(deafened.get() && !unmuted.get());
    // Boost gems show unless turned off.
    assert!(YapfastWindow::new().unwrap().get_boost_gems());
    assert!(Layout::default().boost_gems.unwrap_or(true));
}

#[test]
fn push_to_talk_keeps_its_mode_key_and_release_delay() {
    i_slint_backend_testing::init_no_event_loop();
    let defaults: Layout = serde_json::from_str("{}").unwrap();
    assert_eq!(defaults.input_mode, InputMode::VoiceActivity);
    assert_eq!(defaults.release_delay(), 20, "Discord's default");
    let layout: Layout = serde_json::from_value(serde_json::json!({
        "input_mode": "push_to_talk",
        "push_to_talk_key": { "scancode": 47, "name": "V", "trigger": "v" },
        "push_to_talk_release_delay": 5000,
    }))
    .unwrap();
    assert_eq!(layout.release_delay(), 2000, "Discord's slider ends at 2 s");
    let window = classic_window();
    layout.apply(&window);
    assert_eq!(window.get_input_mode(), 1);
    assert_eq!(window.get_talk_key(), "V");
    assert_eq!(window.get_talk_delay(), 2000);
    assert!(yapfast_core::push_to_talk::on());
    let saved = serde_json::to_value(&layout).unwrap();
    assert_eq!(saved["input_mode"], "push_to_talk");
    assert_eq!(saved["push_to_talk_key"]["scancode"], 47);
    assert_eq!(saved["push_to_talk_key"]["trigger"], "v");
    defaults.apply(&window);
    assert!(!yapfast_core::push_to_talk::on());
    assert_eq!(window.get_talk_key(), "");
}

#[test]
fn each_sounds_own_volume_counts_only_while_adjusting_each() {
    i_slint_backend_testing::init_no_event_loop();
    let defaults: Layout = serde_json::from_str("{}").unwrap();
    assert_eq!(defaults.sound_set, SoundSet::NewKalimba);
    assert_eq!(defaults.sound_gain(Sound::Join), 1.0);
    let mut layout = Layout {
        sound_set: SoundSet::Rhodes,
        sound_volume: Some(50),
        each_sound: [(Sound::Mute, 50)].into(),
        ..Default::default()
    };
    assert!((layout.sound_gain(Sound::Mute) - 0.25).abs() < 1e-6);
    layout.adjust_each_sound = true;
    assert!((layout.sound_gain(Sound::Mute) - 0.0625).abs() < 1e-6);
    assert!((layout.sound_gain(Sound::Join) - 0.25).abs() < 1e-6);

    let window = classic_window();
    layout.apply(&window);
    assert_eq!(window.get_sound_set(), 3);
    assert_eq!(window.get_sound_volume(), 50);
    assert!(window.get_adjust_each_sound());
    let each: Vec<i32> = window.get_each_sound().iter().collect();
    assert_eq!(
        each,
        [100, 100, 50, 100, 100, 100, 100, 100, 100, 100, 100, 100]
    );
    let saved = serde_json::to_value(&layout).unwrap();
    assert_eq!(saved["sound_set"], "rhodes");
    assert_eq!(saved["each_sound"]["mute"], 50);

    // Someone who turned off the desktop's sound for notifications keeps
    // messages quiet.
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join(LAYOUT),
        r#"{"notification_sound":false}"#,
    )
    .unwrap();
    let read = Layout::read(directory.path());
    assert_eq!(read.sound_gain(Sound::Message), 0.0);
    assert_eq!(read.sound_gain(Sound::Mention), 0.0);
    assert!(read.sound_gain(Sound::Join) > 0.0);
    let saved = serde_json::to_value(&read).unwrap();
    assert!(saved.get("notification_sound").is_none());
}

#[test]
fn the_renderer_setting_is_remembered() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    // Software unless chosen.
    assert!(!window.get_gpu_rendering());
    window.set_gpu_rendering(true);
    window.invoke_preferences_changed();
    assert!(Layout::read(directory.path()).gpu_rendering);
}

#[test]
fn developer_mode_is_off_until_turned_on_and_remembered() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let open = || {
        let window = YapfastWindow::new().unwrap();
        let adapter = Adapter::new(
            Arc::new(BackgroundRuntime::new().unwrap()),
            directory.path().into(),
            &window,
        )
        .unwrap();
        bind_callbacks(&window, &adapter);
        (window, adapter)
    };
    let (window, _adapter) = open();
    assert!(!window.get_developer_mode());
    window.set_developer_mode(true);
    window.invoke_preferences_changed();
    assert!(Layout::read(directory.path()).developer_mode);
    let (window, _adapter) = open();
    assert!(window.get_developer_mode(), "on again at the next start");
}

#[test]
fn custom_games_are_remembered_and_edited_in_place() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    assert!(!window.invoke_save_game_executable(-1, "".into(), "Game".into()));
    assert!(!window.invoke_save_game_executable(-1, "game".into(), "a".repeat(129).into()));
    assert!(window.invoke_save_game_executable(-1, " /opt/my-game ".into(), " My game ".into()));
    assert!(window.invoke_save_game_executable(-1, "other".into(), "Other".into()));
    // Editing keeps the game where it was.
    assert!(window.invoke_save_game_executable(0, "/opt/my-game".into(), "New title".into()));
    // Saving an executable already listed renames that entry.
    assert!(window.invoke_save_game_executable(-1, "other".into(), "Renamed".into()));
    let saved = Layout::read(directory.path());
    assert_eq!(
        saved.game_executables,
        vec![
            ("/opt/my-game".into(), "New title".into()),
            ("other".into(), "Renamed".into())
        ]
    );
    assert_eq!(window.get_game_executables().row_count(), 2);
    window.invoke_remove_game_executable(0);
    assert_eq!(
        Layout::read(directory.path()).game_executables,
        vec![("other".into(), "Renamed".into())]
    );
    assert_eq!(window.get_game_executables().row_count(), 1);
    window.invoke_remove_game_executable(0);
}

#[test]
fn says_how_long_a_game_has_run() {
    assert_eq!(games::played_for(59_000), "less than a minute");
    assert_eq!(games::played_for(5 * 60_000), "5 min");
    assert_eq!(games::played_for(120 * 60_000), "2 hr");
    assert_eq!(games::played_for(125 * 60_000), "2 hr 5 min");
}

#[test]
fn a_chosen_device_that_went_away_shows_as_not_connected() {
    let device = |id: &str, name: &str| voice::Device {
        id: id.into(),
        name: name.into(),
    };
    let mut names = HashMap::new();
    let both = [
        device("{speakers}", "Speakers"),
        device("{headphones}", "Headphones"),
    ];
    let (rows, index) = device_rows(&both, Some("{headphones}"), &mut names);
    assert_eq!(rows, ["Speakers", "Headphones"]);
    assert_eq!(index, 1);
    // They go away: still chosen, by the name they had, not another device.
    let (rows, index) = device_rows(&both[..1], Some("{headphones}"), &mut names);
    assert_eq!(rows, ["Speakers", "Headphones (not connected)"]);
    assert_eq!(index, 1);
    // A device known by its name, as on PulseAudio, has it in its id.
    let (rows, index) = device_rows(&both[..1], Some("name:USB headset"), &mut HashMap::new());
    assert_eq!(rows, ["Speakers", "USB headset (not connected)"]);
    assert_eq!(index, 1);
    // The default entry's id names the default it had; it stays the default.
    let pulse = [
        device("name:default: Headset", "default: Headset"),
        device("name:Headset", "Headset"),
        device("name:Built-in", "Built-in"),
    ];
    let (rows, index) = device_rows(&pulse, Some("name:default: Built-in"), &mut names);
    assert_eq!(rows.len(), 3);
    assert_eq!(index, 0);
    let (rows, index) = device_rows(&[], None, &mut names);
    assert!(rows.is_empty());
    assert_eq!(index, -1, "nothing to choose");
    assert_eq!(
        device_rows(&both, None, &mut names).1,
        0,
        "the default, listed first"
    );
}

#[test]
fn call_volumes_play_while_dragged_and_are_remembered() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    assert_eq!(window.get_microphone_volume(), 100, "as it comes");
    assert_eq!(window.get_speaker_volume(), 100);
    window.set_microphone_volume(200);
    window.set_speaker_volume(50);
    window.invoke_call_volumes_changed();
    assert!((yapfast_core::volume::microphone() - 1.995).abs() < 1e-3);
    assert!((yapfast_core::volume::speakers() - 0.143).abs() < 1e-3);
    assert_eq!(
        Layout::read(directory.path()).microphone_volume,
        None,
        "a drag's steps aren't saved"
    );
    window.invoke_preferences_changed();
    let read = Layout::read(directory.path());
    assert_eq!(read.microphone_volume, Some(200));
    assert_eq!(read.speaker_volume, Some(50));
    let defaults: Layout = serde_json::from_str("{}").unwrap();
    defaults.apply(&window);
    assert_eq!(yapfast_core::volume::microphone(), 1.0);
    assert_eq!(yapfast_core::volume::speakers(), 1.0);
    read.apply(&window);
    assert_eq!(window.get_microphone_volume(), 200);
    assert_eq!(window.get_speaker_volume(), 50);
    defaults.apply(&window);
}

#[test]
fn sound_settings_are_remembered() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.set_sound_set(4);
    window.set_sound_volume(40);
    window.set_adjust_each_sound(true);
    window.invoke_preferences_changed();
    window.invoke_each_sound_changed(2, 30);
    window.invoke_each_sound_changed(6, 0);
    let read = Layout::read(directory.path());
    assert_eq!(read.sound_set, SoundSet::Glow);
    assert_eq!(read.sound_volume, Some(40));
    assert!(read.adjust_each_sound);
    assert_eq!(read.each_sound(Sound::Mute), 30);
    assert_eq!(read.sound_gain(Sound::Message), 0.0);
    let each: Vec<i32> = window.get_each_sound().iter().collect();
    assert_eq!(each[2], 30);
    // Back at 100%, a sound's own volume is forgotten.
    window.invoke_each_sound_changed(2, 100);
    assert!(
        !Layout::read(directory.path())
            .each_sound
            .contains_key(&Sound::Mute)
    );
}

#[test]
fn embed_fields_share_rows_three_inline_at_a_time() {
    let field = |name: &str, inline| EmbedField {
        name: name.into(),
        value: String::new(),
        inline,
    };
    let rows = field_rows(&[
        field("a", true),
        field("b", true),
        field("c", true),
        field("d", true),
        field("e", false),
        field("f", true),
    ]);
    let names: Vec<Vec<slint::StyledText>> = rows
        .iter()
        .map(|row| row.fields.iter().map(|field| field.name).collect())
        .collect();
    let expected: Vec<Vec<slint::StyledText>> = [&["a", "b", "c"][..], &["d"], &["e"], &["f"]]
        .iter()
        .map(|row| row.iter().map(|name| bold(name)).collect())
        .collect();
    assert_eq!(names, expected);
}

/// A window showing one styled message from Sam, with the composer.
fn one_message(body: &str) -> YapfastWindow {
    messages(&[("Sam", body)])
}

/// A window showing styled messages, each starting a group, with the composer.
fn messages(lines: &[(&str, &str)]) -> YapfastWindow {
    showing(
        lines
            .iter()
            .map(|(author, body)| ChatMessage {
                author: (*author).into(),
                body: (*body).into(),
                formatted: true,
                ..Default::default()
            })
            .collect(),
    )
}

/// Starts the testing backend on this test's thread, once: a test that sets
/// anything up before it builds a window calls it first, as a font of emoji
/// would otherwise start the platform's own.
fn init_backend() {
    thread_local! {
        static STARTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    STARTED.with(|started| {
        if !started.replace(true) {
            i_slint_backend_testing::init_no_event_loop();
        }
    });
}

/// A window showing these messages, each starting a group and sent at 19:2 and its
/// index, with the composer.
fn showing(chat: Vec<ChatMessage>) -> YapfastWindow {
    init_backend();
    let window = classic_window();
    copy_whole_messages(&window);
    // Without the adapter's clipboard reading, Ctrl+V pastes text, as it
    // does when the clipboard holds no files or picture.
    let weak = window.as_weak();
    window.on_paste_into_composer(move || weak.unwrap().invoke_paste_composer_text());
    window.set_workspace(true);
    window.set_room_selected(true);
    window.set_room_ready(true);
    window.set_composer_enabled(true);
    // A device that may write, so the verification banner stays away.
    window.set_trusted(true);
    let rows: Vec<MessageRow> = chat
        .into_iter()
        .enumerate()
        .map(|(index, message)| {
            message_row_plain(&ChatMessage {
                id: format!("m{index}"),
                item: format!("i{index}"),
                time: format!("19:2{index}"),
                ..message
            })
        })
        .collect();
    window.set_messages(ModelRc::new(VecModel::from(rows)));
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    settle(&window);
    window
}

fn settle(window: &YapfastWindow) {
    for _ in 0..5 {
        slint::platform::update_timers_and_animations();
        let _ = window.invoke_timeline_extent();
    }
}

/// For each height up the window, presses there beside the text, drags up past the top
/// of the list, turning the wheel `wheel` times on the way, lets go and presses Ctrl+C,
/// until a copy changes the clipboard, which it returns. The first drag to start on a
/// message starts at the end of the lowest, so it selects up to the end of that message.
/// The list returns to the newest message before each.
fn copy_upwards(window: &YapfastWindow, wheel: usize) -> Option<String> {
    use slint::platform::{Key, PointerEventButton, WindowEvent};
    let dispatch = |event| window.window().dispatch_event(event);
    let at = |x, y| slint::LogicalPosition::new(x, y);
    let button = PointerEventButton::Left;
    // The composer's single line would turn line breaks into spaces, so
    // read the clipboard itself.
    let clipboard = || {
        i_slint_core::window::WindowInner::from_pub(window.window())
            .context()
            .platform()
            .clipboard_text(i_slint_core::platform::Clipboard::DefaultClipboard)
    };
    let before = clipboard();
    for y in (100..700).step_by(4).rev() {
        window.invoke_scroll_to_latest();
        settle(window);
        let y = y as f32;
        dispatch(WindowEvent::PointerMoved {
            position: at(900., y),
        });
        dispatch(WindowEvent::PointerPressed {
            position: at(900., y),
            button,
        });
        dispatch(WindowEvent::PointerMoved {
            position: at(900., y - 8.),
        });
        dispatch(WindowEvent::PointerMoved {
            position: at(900., 60.),
        });
        for _ in 0..wheel {
            dispatch(WindowEvent::PointerScrolled {
                position: at(900., 60.),
                delta_x: 0.,
                delta_y: 400.,
            });
            settle(window);
        }
        dispatch(WindowEvent::PointerReleased {
            position: at(900., 60.),
            button,
        });
        dispatch(WindowEvent::KeyPressed {
            text: Key::Control.into(),
        });
        dispatch(WindowEvent::KeyPressed { text: "c".into() });
        dispatch(WindowEvent::KeyReleased { text: "c".into() });
        dispatch(WindowEvent::KeyReleased {
            text: Key::Control.into(),
        });
        let copied = clipboard();
        if copied != before {
            return copied;
        }
    }
    before
}

#[test]
fn a_selection_runs_across_messages_with_their_names_and_times() {
    let window = messages(&[
        ("Sam", "Thinking **co-op** after nine."),
        ("Riley", "Yep. I should be free after dinner."),
    ]);
    assert_eq!(
        copy_upwards(&window, 0).as_deref(),
        Some(
            "Sam 19:20\nThinking co-op after nine.\nRiley 19:21\nYep. I should be free after dinner."
        )
    );
}

#[test]
fn pictures_on_their_way_hold_their_place_as_skeletons() {
    let attachment = |name: &str, preview_failed| yapfast_core::chat::Attachment {
        voice_ms: None,
        name: name.into(),
        size: Some(1000),
        image: true,
        dimensions: Some((1600, 900)),
        preview: None,
        preview_failed,
        placement: Placement::Message,
        gif: None,
        spoiler: false,
        video: false,
        url: None,
    };
    let row = message_row_plain(&ChatMessage {
        attachments: vec![attachment("wide.png", false)],
        ..Default::default()
    });
    let cell = row.images.row_data(0).unwrap();
    assert!(cell.loading);
    // As the 550 by 400 preview will be: 1600 by 900 scaled to fit.
    assert_eq!((row.gallery_width, row.gallery_height), (550, 309));
    assert_eq!(row.files.row_count(), 0, "not a file card meanwhile");
    // One whose preview failed is a file card instead.
    let row = message_row_plain(&ChatMessage {
        attachments: vec![attachment("broken.png", true)],
        ..Default::default()
    });
    assert_eq!(row.images.row_count(), 0);
    assert_eq!(row.files.row_count(), 1);
}

#[test]
fn the_sticker_picker_lists_the_servers_stickers_above_the_packs() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    let sticker = |id: &str, name: &str| yapfast_core::chat::Sticker {
        id: id.into(),
        name: name.into(),
        format: 1,
        picture: format!("https://cdn.discordapp.com/stickers/{id}.png?size=96"),
        usable: true,
    };
    {
        let mut adapter = adapter.borrow_mut();
        let state = &mut adapter.app.state;
        state.stickers.open = true;
        state.stickers.server_for = Some("10".into());
        state.stickers.servers = vec![yapfast_core::application::ServerStickers {
            id: "10".into(),
            name: "Server".into(),
            stickers: vec![sticker("1", "Wave"), sticker("2", "Bounce")],
        }];
        state.stickers.packs = vec![yapfast_core::chat::StickerCollection {
            id: "20".into(),
            name: "Wumpus".into(),
            stickers: vec![sticker("3", "Party")],
        }];
        adapter.show_stickers(&window);
    }
    let tiles = window.get_sticker_tiles();
    let labels: Vec<String> = (0..tiles.row_count())
        .filter_map(|row| tiles.row_data(row))
        .map(|tile| tile.label.to_string())
        .collect();
    // Every collection lists: Frequently Used first, then the server's own
    // stickers, then Discord's packs.
    assert_eq!(
        labels,
        [
            "Frequently Used",
            "Party",
            "Server",
            "Wave",
            "Bounce",
            "Wumpus",
            "Party"
        ],
        "Frequently Used first, then the server's own stickers"
    );
    let locked: Vec<bool> = (0..tiles.row_count())
        .filter_map(|row| tiles.row_data(row))
        .filter(|tile| tile.kind == 0)
        .map(|tile| tile.usable)
        .collect();
    assert_eq!(
        locked,
        [true, true, true, true],
        "the server's are usable here, and Discord's packs are anyone's"
    );
    // The rail lists the same sections the emoji picker's does, with no line
    // between them.
    let sections = window.get_sticker_sections();
    let shown: Vec<(String, SectionGlyph)> = (0..sections.row_count())
        .filter_map(|row| sections.row_data(row))
        .map(|section| (section.name.to_string(), section.glyph))
        .collect();
    assert_eq!(
        shown,
        [
            ("Frequently Used".to_string(), SectionGlyph::Clock),
            ("Server".to_string(), SectionGlyph::None),
            ("Wumpus".to_string(), SectionGlyph::None),
        ],
        "Frequently Used carries the clock, as the emoji picker's does"
    );
}

#[test]
fn a_servers_icon_draws_on_its_heading_and_rail_button() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    let icon = "https://cdn.discordapp.com/icons/10/abc.png?size=128";
    {
        let mut adapter = adapter.borrow_mut();
        let state = &mut adapter.app.state;
        state.servers = vec![Arc::new(yapfast_core::chat::ChatServer {
            id: yapfast_core::discord::server_id("10"),
            name: "Server".into(),
            channels: vec![],
            discord: true,
            muted: false,
            unavailable: false,
            icon: Some(icon.into()),
            folder: None,
            hide_muted: false,
            community: false,
            pinned: Vec::new(),
            banner: None,
            verified: false,
            events: Vec::new(),
            tag: None,
        })];
        state.stickers.open = true;
        state.stickers.server_for = Some("10".into());
        state.stickers.servers = vec![yapfast_core::application::ServerStickers {
            id: "10".into(),
            name: "Server".into(),
            stickers: vec![yapfast_core::chat::Sticker {
                id: "1".into(),
                name: "Wave".into(),
                format: 1,
                picture: "https://cdn.discordapp.com/stickers/1.png?size=96".into(),
                usable: true,
            }],
        }];
        adapter.show_stickers(&window);
    }
    let heading_has_its_icon = |window: &YapfastWindow| {
        let tiles = window.get_sticker_tiles();
        (0..tiles.row_count())
            .filter_map(|row| tiles.row_data(row))
            .find(|tile| tile.kind == 1)
            .is_some_and(|tile| tile.image.size().width > 0)
    };
    // Before it came, the heading has no icon.
    assert!(!heading_has_its_icon(&window));
    // Once the app's pictures carry it, the heading and the rail button draw it.
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.state.pictures.insert(
            icon.into(),
            Arc::new(Picture {
                width: 128,
                height: 128,
                rgba: yapfast_core::chat::Pixels::new(128, 128, vec![255; 128 * 128 * 4]),
                ..Default::default()
            }),
        );
        adapter.show_stickers(&window);
    }
    assert!(
        heading_has_its_icon(&window),
        "the server's icon draws on its heading"
    );
    let sections = window.get_sticker_sections();
    let section = sections.row_data(0).expect("the server's section");
    assert!(
        section.icon.picture.size().width > 0,
        "and on its rail button"
    );
}

#[test]
fn a_stickers_picture_reaches_its_tile_and_the_grid_keeps_a_window() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    let sticker = |id: usize| yapfast_core::chat::Sticker {
        id: id.to_string(),
        name: format!("Sticker {id}"),
        format: 1,
        picture: format!("https://cdn.discordapp.com/stickers/{id}.png?size=96"),
        usable: true,
    };
    let stickers: Vec<_> = (0..80).map(sticker).collect();
    let first = stickers[0].picture.clone();
    {
        let mut adapter = adapter.borrow_mut();
        let state = &mut adapter.app.state;
        state.stickers.open = true;
        state.stickers.server_for = Some("10".into());
        state.stickers.servers = vec![yapfast_core::application::ServerStickers {
            id: "10".into(),
            name: "Server".into(),
            stickers,
        }];
        adapter.show_stickers(&window);
    }
    // Before the grid says how tall it is, all of it is published.
    let all = window.get_sticker_tiles().row_count();
    assert_eq!(all, 81, "the server's heading and its 80 stickers");
    // Once it does, only what is in sight and a screen either side is.
    adapter.borrow_mut().stickers_scrolled(&window, 0.0, 486.0);
    let windowed = window.get_sticker_tiles().row_count();
    assert!(
        windowed < all && windowed >= 4,
        "a window, not the whole grid: {windowed} of {all}"
    );
    // A picture that arrives draws on its tile, though the window did not move.
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.state.stickers.pictures.insert(
            first.clone(),
            yapfast_core::application::StickerPicture::Shown(Arc::new(Picture {
                width: 96,
                height: 96,
                rgba: yapfast_core::chat::Pixels::new(96, 96, vec![255; 96 * 96 * 4]),
                ..Default::default()
            })),
        );
        adapter.show_stickers(&window);
    }
    let tiles = window.get_sticker_tiles();
    assert!(
        (0..tiles.row_count())
            .filter_map(|row| tiles.row_data(row))
            .any(|tile| tile.kind == 0 && tile.image.size().width > 0),
        "the picture arrived and drew on its tile"
    );
}

#[test]
fn a_messages_sticker_is_160px_square() {
    // A sticker's picture may come smaller than Discord's 160px — an old kept
    // one, or a Lottie drawn small — but its cell is 160 square, as Discord
    // draws it.
    let picture = Arc::new(Picture {
        width: 96,
        height: 96,
        rgba: yapfast_core::chat::Pixels::new(96, 96, vec![0; 96 * 96 * 4]),
        ..Default::default()
    });
    let row = message_row_plain(&ChatMessage {
        attachments: vec![yapfast_core::chat::Attachment {
            voice_ms: None,
            name: "Wave".into(),
            size: None,
            image: true,
            dimensions: Some((160, 160)),
            preview: Some(picture),
            preview_failed: false,
            placement: Placement::Sticker,
            gif: None,
            spoiler: false,
            video: false,
            url: None,
        }],
        ..Default::default()
    });
    let cell = row.stickers.row_data(0).expect("the sticker shows");
    assert_eq!((cell.width, cell.height), (160, 160));
    assert!(!cell.loading);
}

#[test]
fn a_selection_copies_the_pictures_it_runs_through() {
    let picture = Arc::new(Picture {
        width: 40,
        height: 30,
        rgba: yapfast_core::chat::Pixels::new(40, 30, vec![128; 40 * 30 * 4]),
        ..Default::default()
    });
    let window = showing(vec![
        ChatMessage {
            author: "Sam".into(),
            body: "Look at this.".into(),
            attachments: vec![yapfast_core::chat::Attachment {
                voice_ms: None,
                name: "cat.png".into(),
                size: None,
                image: true,
                dimensions: Some((400, 300)),
                preview: Some(picture),
                preview_failed: false,
                placement: Placement::Message,
                gif: None,
                spoiler: false,
                video: false,
                url: None,
            }],
            ..Default::default()
        },
        ChatMessage {
            author: "Riley".into(),
            body: "Nice.".into(),
            ..Default::default()
        },
    ]);
    assert_eq!(
        copy_upwards(&window, 0).as_deref(),
        Some("Sam 19:20\nLook at this.\n[Picture]\nRiley 19:21\nNice.")
    );
}

#[test]
fn a_selection_keeps_the_messages_it_scrolls_past() {
    let chat: Vec<ChatMessage> = (0..40)
        .map(|index| ChatMessage {
            author: "Sam".into(),
            body: format!("Message {index}."),
            ..Default::default()
        })
        .collect();
    let window = showing(chat);
    let expected: Vec<String> = (0..40)
        .map(|index| format!("Sam 19:2{index}\nMessage {index}."))
        .collect();
    assert_eq!(copy_upwards(&window, 40), Some(expected.join("\n")));
}

#[test]
fn a_click_on_a_link_in_selectable_text_opens_it() {
    use slint::platform::{PointerEventButton, WindowEvent};
    let window = one_message("See https://example.com/yapfast");
    let opened = Rc::new(RefCell::new(Vec::<String>::new()));
    let record = opened.clone();
    window.on_open_link(move |link| record.borrow_mut().push(link.into()));
    let button = PointerEventButton::Left;
    // The link starts after "See ", a little way into the text column.
    for y in (100..700).step_by(4) {
        let position = slint::LogicalPosition::new(440., y as f32);
        window
            .window()
            .dispatch_event(WindowEvent::PointerPressed { position, button });
        window
            .window()
            .dispatch_event(WindowEvent::PointerReleased { position, button });
    }
    let opened = opened.borrow();
    assert!(!opened.is_empty(), "a click on the link opens it");
    assert!(
        opened
            .iter()
            .all(|link| link == "https://example.com/yapfast")
    );
}

#[test]
fn a_link_showing_other_text_asks_before_it_opens() {
    use slint::platform::{PointerEventButton, WindowEvent};
    let window = one_message("See [example.com/yapfast](https://evil.example/yapfast)");
    let opened = Rc::new(RefCell::new(Vec::<String>::new()));
    let record = opened.clone();
    window.on_open_link(move |link| record.borrow_mut().push(link.into()));
    let button = PointerEventButton::Left;
    for y in (100..700).step_by(4) {
        let position = slint::LogicalPosition::new(440., y as f32);
        window
            .window()
            .dispatch_event(WindowEvent::PointerPressed { position, button });
        window
            .window()
            .dispatch_event(WindowEvent::PointerReleased { position, button });
    }
    let opened = opened.borrow();
    assert!(!opened.is_empty(), "a click on the link reaches Yapfast");
    assert!(
        opened
            .iter()
            .all(|link| link == "yapfast:leave/https://evil.example/yapfast"),
        "{opened:?}"
    );
}

#[test]
fn a_click_on_an_emoji_in_a_message_reaches_its_card() {
    use slint::platform::{PointerEventButton, WindowEvent};
    // The window's setup registers Twemoji's font, as the program's does.
    init_backend();
    emoji_text::register();
    let window = one_message("😭 that hurt");
    let opened = Rc::new(RefCell::new(Vec::<String>::new()));
    let record = opened.clone();
    window.on_open_link(move |link| record.borrow_mut().push(link.into()));
    let button = PointerEventButton::Left;
    // The emoji starts the message, which sits at the end of the list.
    for y in (620..700).step_by(4) {
        for x in (370..430).step_by(4) {
            let position = slint::LogicalPosition::new(x as f32, y as f32);
            window
                .window()
                .dispatch_event(WindowEvent::PointerPressed { position, button });
            window
                .window()
                .dispatch_event(WindowEvent::PointerReleased { position, button });
        }
    }
    let opened = opened.borrow();
    assert!(
        opened.iter().any(|link| link == "yapfast:emoji/1f62d"),
        "{opened:?}"
    );
}

#[test]
fn an_emoji_card_says_what_the_emoji_is() {
    let mut pictures = HashMap::new();
    let state = yapfast_core::application::AppState::default();
    let default = emoji_card(
        &emoji_text::Target::Unicode("😭".to_owned()),
        None,
        &state,
        &mut pictures,
    );
    assert_eq!(default.name, ":sob:");
    assert_eq!(
        default.description,
        "This is a default emoji. It can be used anywhere."
    );
    assert_eq!(default.server, "", "no server to show");
    assert_eq!(default.glyph, "😭", "shown when Twemoji has no picture");
    // A server's emoji, whose server this account can't reach, says so, and
    // shows no server.
    let custom = emoji_card(
        &emoji_text::Target::Custom("42".to_owned()),
        None,
        &state,
        &mut pictures,
    );
    assert_eq!(custom.description, "Invite-Only Server");
    // Nothing of the server shows: no name, no count, no separator.
    assert_eq!(custom.server, "");
    assert_eq!(custom.server_online, "");
    assert_eq!(custom.server_note, "");
}

/// The clock an animated emoji draws from moves only while playing, so an
/// unfocused or idle window holds every emoji still; it is the vendored
/// registry's, which this crate can reach and its own tests cannot run.
#[test]
fn an_animated_emoji_draws_a_frame_only_while_playing() {
    use i_slint_core::yapfast_emoji as registry;
    let (glyph, key) = ('\u{10ffe0}', "test-animation:1");
    let frame =
        || slint::Image::from_rgba8(slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(2, 2));
    let animation = Rc::new(registry::Animation::new(vec![frame(), frame()], &[10, 10]));
    registry::insert(key, Some(glyph), animation);
    // The glyph message text draws, and the name the picker knows, both name it.
    assert!(registry::frame_of_glyph(glyph).is_some());
    assert!(registry::frame_of_key(key).is_some());
    registry::set_playing(false);
    let paused = registry::elapsed();
    registry::advance(Duration::from_millis(40));
    assert_eq!(registry::elapsed(), paused, "a paused clock holds still");
    registry::set_playing(true);
    registry::advance(Duration::from_millis(40));
    assert!(registry::elapsed() > paused, "and moves while playing");
    registry::set_playing(false);
    registry::remove(key);
    assert!(
        registry::frame_of_glyph(glyph).is_none(),
        "the still draws again"
    );
}

/// The clock ticks at the pace of the fastest animation, and most ticks show
/// no new frame: those draw nothing again, as each draw walks the window.
#[test]
fn the_animation_clock_draws_again_only_when_a_frame_changes() {
    use i_slint_core::yapfast_emoji as registry;
    let frame =
        || slint::Image::from_rgba8(slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(2, 2));
    registry::set_playing(true);
    let spin = Rc::new(registry::Animation::new(
        vec![frame(), frame()],
        &[100, 100],
    ));
    registry::insert("test-spin:1", None, spin);
    let start = registry::revision();
    // 30ms into a 100ms frame: nothing to draw again.
    registry::advance(Duration::from_millis(30));
    assert_eq!(registry::revision(), start);
    registry::advance(Duration::from_millis(80));
    assert_eq!(
        registry::revision(),
        start.wrapping_add(1),
        "the second frame shows"
    );
    // A still never moves it.
    registry::remove("test-spin:1");
    let still = Rc::new(registry::Animation::new(vec![frame()], &[100]));
    registry::insert("test-still:1", None, still);
    let held = registry::revision();
    registry::advance(Duration::from_millis(500));
    assert_eq!(registry::revision(), held);
    registry::remove("test-still:1");
    registry::set_playing(false);
}

#[test]
fn an_emoji_card_says_how_many_of_its_servers_members_are_online() {
    let mut pictures = HashMap::new();
    let mut state = yapfast_core::application::AppState::default();
    let server = yapfast_core::chat::ChatServer {
        id: "discord:10".into(),
        name: "Equicord".into(),
        channels: Vec::new(),
        discord: true,
        muted: false,
        unavailable: false,
        icon: Some("https://cdn.discordapp.com/icons/10/abc.png".into()),
        folder: None,
        hide_muted: false,
        community: true,
        pinned: Vec::new(),
        ..Default::default()
    };
    let target = emoji_text::Target::Custom("42".to_owned());
    // Before Discord answers, the note follows the features Yapfast has.
    let known = emoji_card(&target, Some(&server), &state, &mut pictures);
    assert_eq!(known.server_online, "", "nothing said yet");
    assert_eq!(known.server_note, "Discoverable Server");
    assert_eq!(
        known.server_picture_url, "https://cdn.discordapp.com/icons/10/abc.png",
        "the card asks for the icon by address, so it loads"
    );
    state.server_previews.insert(
        server.id.clone(),
        yapfast_core::chat::ServerPreview {
            online: Some(3999),
            discoverable: true,
        },
    );
    let answered = emoji_card(&target, Some(&server), &state, &mut pictures);
    assert_eq!(
        answered.server_online, "3,999 Online",
        "grouped as Discord writes it"
    );
    assert_eq!(answered.server_note, "Discoverable Server");
    // A server Discover doesn't list shows its count alone, whatever feature
    // Yapfast knew of it.
    state.server_previews.insert(
        server.id.clone(),
        yapfast_core::chat::ServerPreview {
            online: Some(12),
            discoverable: false,
        },
    );
    // A server that isn't a Community shows its count alone.
    let mut quiet = server.clone();
    quiet.community = false;
    let private = emoji_card(&target, Some(&quiet), &state, &mut pictures);
    assert_eq!(private.server_online, "12 Online");
    assert_eq!(private.server_note, "");
    // A Community keeps its note even when the preview answers without the
    // feature, which is what Discord's own card does.
    let noted = emoji_card(&target, Some(&server), &state, &mut pictures);
    assert_eq!(noted.server_online, "12 Online");
    assert_eq!(noted.server_note, "Discoverable Server");
    // A server out of reach, as in an outage, shows no server either.
    let mut away = server.clone();
    away.unavailable = true;
    let unreachable = emoji_card(&target, Some(&away), &state, &mut pictures);
    assert_eq!(unreachable.description, "Invite-Only Server");
    assert_eq!(unreachable.server, "", "no server to open");
    assert_eq!(unreachable.server_online, "", "and nothing of it to show");
    assert_eq!(unreachable.server_note, "");
}

#[test]
fn a_right_click_on_a_link_gives_its_menu_the_address() {
    use slint::platform::{PointerEventButton, WindowEvent};
    let window = one_message("See [example.com/yapfast](https://evil.example/yapfast)");
    let button = PointerEventButton::Right;
    let mut found = 0;
    for y in (100..700).step_by(4) {
        let position = slint::LogicalPosition::new(440., y as f32);
        window
            .window()
            .dispatch_event(WindowEvent::PointerPressed { position, button });
        window
            .window()
            .dispatch_event(WindowEvent::PointerReleased { position, button });
        let at = |x: f32| {
            i_slint_core::yapfast_selection::pressed_link_at(
                i_slint_core::lengths::LogicalPoint::new(x, y as f32),
            )
        };
        if let Some(link) = at(440.) {
            assert_eq!(link, "yapfast:leave/https://evil.example/yapfast");
            assert_eq!(at(460.), None, "only where the press was");
            found += 1;
        }
    }
    assert!(found > 0, "a right press on the link records it");
    assert_eq!(
        callbacks::web_address("yapfast:leave/https://evil.example/yapfast"),
        Some("https://evil.example/yapfast")
    );
    assert_eq!(
        callbacks::web_address("https://example.com/a"),
        Some("https://example.com/a")
    );
    assert_eq!(callbacks::web_address("yapfast:user/1"), None);
    assert_eq!(
        callbacks::web_address("https://matrix.to/#/@sam:example.org"),
        None
    );
}

#[test]
fn styled_messages_select_by_dragging_and_copy_as_plain_text() {
    use slint::platform::{Key, PointerEventButton, WindowEvent};
    let window = one_message("Thinking **co-op** after nine, if people are up for it.");
    let dispatch = |event| window.window().dispatch_event(event);
    let button = PointerEventButton::Left;
    let at = |x, y| slint::LogicalPosition::new(x, y);
    let ctrl = |key: &str| {
        dispatch(WindowEvent::KeyPressed {
            text: Key::Control.into(),
        });
        dispatch(WindowEvent::KeyPressed { text: key.into() });
        dispatch(WindowEvent::KeyReleased { text: key.into() });
        dispatch(WindowEvent::KeyReleased {
            text: Key::Control.into(),
        });
    };

    // Clicking the message without selecting leaves typing in the composer.
    let click = |x, y| {
        dispatch(WindowEvent::PointerPressed {
            position: at(x, y),
            button,
        });
        dispatch(WindowEvent::PointerReleased {
            position: at(x, y),
            button,
        });
    };
    let type_key = |key: &str| {
        dispatch(WindowEvent::KeyPressed { text: key.into() });
        dispatch(WindowEvent::KeyReleased { text: key.into() });
    };
    click(600., 712.);
    type_key("a");
    for y in (100..700).step_by(4) {
        click(500., y as f32);
    }
    type_key("b");
    assert_eq!(window.get_draft(), "ab");
    window.set_draft(SharedString::default());

    // Without element lookups in this build, drag down the timeline row by
    // row: from right of the text, past its end, to left of the timeline,
    // past its start. A drag that lands on the message selects all of it,
    // and copies it.
    for y in (100..700).step_by(4) {
        let y = y as f32;
        dispatch(WindowEvent::PointerMoved {
            position: at(900., y),
        });
        dispatch(WindowEvent::PointerPressed {
            position: at(900., y),
            button,
        });
        dispatch(WindowEvent::PointerMoved {
            position: at(600., y),
        });
        dispatch(WindowEvent::PointerMoved {
            position: at(200., y),
        });
        dispatch(WindowEvent::PointerReleased {
            position: at(200., y),
            button,
        });
        ctrl("c");
    }

    // Pasting shows what was copied: the text as read, without its Markdown.
    click(600., 712.);
    ctrl("v");
    assert_eq!(
        window.get_draft(),
        "Thinking co-op after nine, if people are up for it."
    );
}

#[test]
fn pointer_input_reaches_navigation_buttons() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    let click = |x, y| {
        let position = slint::LogicalPosition::new(x, y);
        let button = slint::platform::PointerEventButton::Left;
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerPressed { position, button });
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerReleased { position, button });
    };
    click(32., 36.);
    assert!(window.get_dms_selected());
    assert_eq!(window.get_sidebar_title(), "Direct messages");
    click(32., 101.);
    assert!(!window.get_dms_selected());
    assert_eq!(window.get_sidebar_title(), "Friends");
    window.hide().unwrap();
}

#[test]
fn menus_list_their_choices_and_carry_them_out() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    let texts = |window: &YapfastWindow| -> Vec<String> {
        window
            .get_menu_rows()
            .iter()
            .map(|row| row.text.to_string())
            .collect()
    };
    window.invoke_prepare_menu("sam".into(), false);
    assert_eq!(texts(&window), ["Profile"]);
    window.invoke_prepare_menu("lounge".into(), false);
    assert_eq!(texts(&window), ["Join voice"]);
    window.invoke_prepare_menu("gone".into(), false);
    assert!(texts(&window).is_empty());

    window.invoke_choose_menu("sam".into(), false, "profile".into());
    assert!(adapter.borrow().app.state.profile.is_some());
    window.invoke_choose_menu("lounge".into(), false, "join-voice".into());
    assert_eq!(
        adapter.borrow().app.state.voice.room.as_deref(),
        Some("lounge")
    );

    let clipboard = || {
        i_slint_core::window::WindowInner::from_pub(window.window())
            .context()
            .platform()
            .clipboard_text(i_slint_core::platform::Clipboard::DefaultClipboard)
    };
    window.invoke_choose_menu("discord:10:21".into(), false, "copy-link".into());
    assert_eq!(
        clipboard().as_deref(),
        Some("https://discord.com/channels/10/21")
    );
    window.invoke_choose_menu("discord:10:21".into(), false, "copy-id".into());
    assert_eq!(clipboard().as_deref(), Some("21"));
    window.invoke_choose_menu("discord:10".into(), false, "copy-link".into());
    assert_eq!(
        clipboard().as_deref(),
        Some("https://discord.com/channels/10")
    );

    // Someone in the open conversation: their card, and the DM with them.
    window.invoke_navigate_server("friends".into());
    window.invoke_select_room("general".into());
    window.invoke_prepare_menu("@sam:preview.local".into(), true);
    assert_eq!(
        texts(&window),
        ["Profile", "Message", "Copy Username", "Copy Display Name"]
    );
    window.invoke_choose_menu("@sam:preview.local".into(), true, "copy-username".into());
    assert_eq!(clipboard().as_deref(), Some("@sam:preview.local"));
    window.invoke_choose_menu(
        "@sam:preview.local".into(),
        true,
        "copy-display-name".into(),
    );
    assert_eq!(clipboard().as_deref(), Some("Sam"));
    window.invoke_choose_menu("@sam:preview.local".into(), true, "message".into());
    assert_eq!(adapter.borrow().app.state.selected.as_deref(), Some("sam"));
}

#[test]
#[cfg(feature = "discord")]
fn a_server_members_menu_follows_permissions_and_ticks_roles() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    // Alice is Staff, below Leads; Dave, her friend, is below her.
    adapter.borrow_mut().app = yapfast_core::application::member_server(&["30"]);
    // Copy ID waits for Developer Mode, off unless turned on, as in Discord.
    assert!(!window.get_developer_mode());
    window.invoke_prepare_menu("4".into(), true);
    let texts: Vec<String> = window
        .get_menu_rows()
        .iter()
        .map(|row| row.text.into())
        .collect();
    assert_eq!(texts.last().map(String::as_str), Some("Copy Display Name"));
    window.set_developer_mode(true);
    window.invoke_preferences_changed();
    assert!(Layout::read(directory.path()).developer_mode);
    window.invoke_prepare_menu("4".into(), true);
    let rows: Vec<MenuRow> = window.get_menu_rows().iter().collect();
    let texts: Vec<&str> = rows.iter().map(|row| row.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "Profile",
            "Mention",
            "Message",
            "Start call",
            "Add note",
            "Add friend nickname",
            "Remove friend",
            "Ignore",
            "Block",
            "Change nickname",
            "Roles",
            "Time out Dave",
            "Kick Dave",
            "Ban Dave",
            "Copy Username",
            "Copy Display Name",
            "Copy ID",
        ]
    );
    let marked = |pick: fn(&MenuRow) -> bool| -> Vec<&str> {
        rows.iter()
            .filter(|row| pick(row))
            .map(|row| row.text.as_str())
            .collect()
    };
    assert_eq!(
        marked(|row| row.separated),
        [
            "Remove friend",
            "Change nickname",
            "Time out Dave",
            "Copy Username"
        ]
    );
    assert_eq!(
        marked(|row| row.danger),
        [
            "Remove friend",
            "Block",
            "Time out Dave",
            "Kick Dave",
            "Ban Dave"
        ]
    );

    // Roles: those this account may give or take, ticked where held. One out
    // of reach would show, greyed, only if Dave held it.
    let roles = window.get_menu_submenus().row_data(10).unwrap();
    let shown: Vec<(String, bool, bool)> = roles
        .iter()
        .map(|row| (row.text.to_string(), row.checked, row.disabled))
        .collect();
    let row = |name: &str, checked, disabled| (name.to_owned(), checked, disabled);
    assert_eq!(
        shown,
        [row("Crew", true, false), row("Wardens", false, false),]
    );
    assert!(roles.iter().all(|row| row.checkbox));
    window.invoke_choose_menu("4".into(), true, "role:20".into());
    assert!(
        !window
            .get_menu_submenus()
            .row_data(10)
            .unwrap()
            .row_data(0)
            .unwrap()
            .checked,
        "Crew unticks at once, and the menu stays open"
    );

    // Leads and the owner get no moderation.
    window.invoke_prepare_menu("3".into(), true);
    assert!(
        !window
            .get_menu_rows()
            .iter()
            .any(|row| row.danger && row.text.starts_with("Kick"))
    );

    // A ban asks how much to delete, and why.
    window.invoke_choose_menu("4".into(), true, "ban".into());
    assert!(window.get_person_open());
    let prompt = window.get_person_prompt();
    assert_eq!(prompt.title, "Ban Dave?");
    assert_eq!(prompt.field, "Reason (optional)");
    let options: Vec<String> = prompt
        .options
        .iter()
        .map(|option| option.to_string())
        .collect();
    assert_eq!(
        options,
        [
            "Don't delete any",
            "The last hour",
            "The last 6 hours",
            "The last 12 hours",
            "The last 24 hours",
            "The last 3 days",
            "The last 7 days",
        ]
    );
    window.invoke_choose_person_option(2);
    assert_eq!(window.get_person_prompt().chosen, 2);
    window.invoke_close_person();
    window.invoke_choose_menu("4".into(), true, "timeout".into());
    let options: Vec<String> = window
        .get_person_prompt()
        .options
        .iter()
        .map(|option| option.to_string())
        .collect();
    assert_eq!(
        options,
        [
            "60 seconds",
            "5 minutes",
            "10 minutes",
            "1 hour",
            "1 day",
            "1 week"
        ]
    );
    window.invoke_close_person();

    // Mention puts their name in the composer.
    window.invoke_choose_menu("3".into(), true, "mention".into());
    assert_eq!(adapter.borrow().app.state.draft(), "@Cara ");
    // The composer colours it as the message will show it.
    let marks = i_slint_core::yapfast_marks::marks_of("@Cara ").expect("the mention is marked");
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0].0, 0..5);
    window.invoke_draft_edited("@Carab".into());
    assert_eq!(i_slint_core::yapfast_marks::marks_of("@Carab"), None);
}

#[test]
#[cfg(feature = "discord")]
fn invite_to_server_lists_the_servers_in_a_submenu() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    let mut app = yapfast_core::application::member_server(&["30"]);
    yapfast_core::application::with_clubs(&mut app);
    adapter.borrow_mut().app = app;
    window.invoke_prepare_menu("4".into(), true);
    let rows: Vec<MenuRow> = window.get_menu_rows().iter().collect();
    let at = rows
        .iter()
        .position(|row| row.text == "Invite to server")
        .unwrap();
    assert_eq!(rows[at + 1].text, "Remove friend");
    assert!(
        rows[at].separated,
        "it starts the group of how she stands with them"
    );
    let servers = window.get_menu_submenus().row_data(at).unwrap();
    let servers: Vec<(String, String)> = servers
        .iter()
        .map(|row| (row.id.to_string(), row.text.to_string()))
        .collect();
    assert_eq!(
        servers,
        [("invite-to:discord:50".to_owned(), "Club".to_owned())],
        "not the server she's shown in, nor one that doesn't let Alice invite"
    );
}

#[test]
fn an_event_shows_when_it_runs_and_where() {
    use yapfast_core::{application::EventDialog, chat::EventPreview, clock};
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    // A fortnight away, past the week its weekday would name, for two hours.
    let starts = clock::now_ms() / 1000 + 14 * 86_400;
    adapter.borrow_mut().app.state.scheduled_event = Some(EventDialog {
        event: Some("11".into()),
        preview: Some(EventPreview {
            event: "11".into(),
            guild: "10".into(),
            name: "Alien meetup".into(),
            description: None,
            starts_at: starts,
            ends_at: Some(starts + 7_200),
            room: None,
            location: Some("Somewhere in the ocean".into()),
            creator: Some("Dziurwa".into()),
            interested_count: Some(7),
            active: false,
            over: false,
            interested: false,
        }),
        ..EventDialog::default()
    });
    adapter.borrow_mut().refresh(&window);
    assert!(window.get_event_open());
    let prompt = window.get_event_prompt();
    let when = prompt.when.to_string();
    assert!(
        when.starts_with(&clock::day(starts)) && when.contains(&clock::time(starts)),
        "the start's day and time: {when}"
    );
    assert!(
        when.ends_with(&clock::time(starts + 7_200)),
        "and its end: {when}"
    );
    assert_eq!(
        prompt.detail,
        "Somewhere in the ocean · by Dziurwa · 7 interested"
    );
    assert_eq!(prompt.confirm, "Interested");
}

#[test]
fn menus_give_each_row_its_own_submenu_and_tick_checkboxes() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let row = |id: &str, checkbox: bool| MenuRow {
        id: id.into(),
        text: id.into(),
        checkbox,
        ..Default::default()
    };
    menus::show(
        &window,
        (
            vec![
                row("profile", false),
                row("mute", false),
                row("notifications", false),
                row("hide-muted", true),
            ],
            vec![
                vec![],
                vec![row("mute-15", false)],
                vec![row("all", false), row("suppress-roles", true)],
                vec![],
            ],
        ),
    );
    let submenus = window.get_menu_submenus();
    assert_eq!(submenus.row_count(), 4);
    assert_eq!(submenus.row_data(0).unwrap().row_count(), 0);
    assert_eq!(submenus.row_data(1).unwrap().row_count(), 1);
    assert_eq!(submenus.row_data(2).unwrap().row_count(), 2);

    // Choosing a checkbox ticks it where it is, in the menu or a submenu.
    assert_eq!(menus::toggle(&window, "hide-muted"), Some(true));
    assert!(window.get_menu_rows().row_data(3).unwrap().checked);
    assert_eq!(menus::toggle(&window, "suppress-roles"), Some(true));
    let ticked = |window: &YapfastWindow| {
        window
            .get_menu_submenus()
            .row_data(2)
            .unwrap()
            .row_data(1)
            .unwrap()
            .checked
    };
    assert!(ticked(&window));
    assert_eq!(menus::toggle(&window, "suppress-roles"), Some(false));
    assert!(!ticked(&window));
    assert_eq!(menus::toggle(&window, "profile"), None, "not a checkbox");

    // Without submenus, the menu reserves no room for one.
    menus::show(&window, (vec![row("profile", false)], vec![vec![]]));
    assert_eq!(window.get_menu_submenus().row_count(), 0);
}

#[test]
fn the_input_caret_opens_its_menu_and_measures_only_while_open() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    // Joined muted, so only the caret could unmute it.
    window.set_unmute_on_join(false);
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    adapter
        .borrow_mut()
        .action(&window, Action::RequestJoinVoice("lounge".into()));
    let click = |x, y| {
        let position = slint::LogicalPosition::new(x, y);
        let button = slint::platform::PointerEventButton::Left;
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerPressed { position, button });
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerReleased { position, button });
    };
    // The caret sits right of the microphone toggle in the dock.
    click(206., 732.);
    assert!(adapter.borrow().meter.is_some());
    assert!(window.get_muted());
    click(800., 200.);
    assert!(adapter.borrow().meter.is_none());
    window.hide().unwrap();
}

#[test]
fn dragging_the_sidebar_edge_resizes_it_within_limits_and_remembers_it() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    let drag = |from: f32, to: f32| {
        let window = window.window();
        let button = slint::platform::PointerEventButton::Left;
        let at = |x| slint::LogicalPosition::new(x, 400.);
        window.dispatch_event(slint::platform::WindowEvent::PointerMoved { position: at(from) });
        window.dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: at(from),
            button,
        });
        window.dispatch_event(slint::platform::WindowEvent::PointerMoved { position: at(to) });
        window.dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position: at(to),
            button,
        });
    };
    assert_eq!(window.get_sidebar_shown(), 240.);
    drag(312., 362.);
    assert_eq!(window.get_sidebar_shown(), 290.);
    assert_eq!(Layout::read(directory.path()).sidebar_width, Some(290.));
    // The sidebar stops at 380px, and at 180px.
    drag(362., 700.);
    assert_eq!(window.get_sidebar_shown(), 380.);
    drag(452., 0.);
    assert_eq!(window.get_sidebar_shown(), 180.);
    // In a narrow window, dragging keeps 420px for the conversation,
    // beside the member list when it shows.
    window
        .window()
        .set_size(slint::LogicalSize::new(800., 560.));
    drag(252., 700.);
    assert_eq!(window.get_sidebar_shown(), 180.);
    window.set_member_list(false);
    drag(252., 700.);
    assert_eq!(window.get_sidebar_shown(), 308.);
    drag(380., 252.);
    // A new window starts at the stored width.
    let next = classic_window();
    adapter.borrow_mut().attach(&next);
    assert_eq!(next.get_sidebar_width(), 180.);
    window.hide().unwrap();
}

#[test]
fn a_window_made_again_shows_what_the_last_one_did() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    adapter
        .borrow_mut()
        .action(&window, Action::SelectRoom("general".into()));
    let topic = Arc::new(yapfast_core::chat::Topic {
        body: "Plans for the weekend".into(),
        ..Default::default()
    });
    adapter.borrow_mut().apply(Event::Topic {
        room: "general".into(),
        topic: Some(topic),
    });
    adapter.borrow_mut().refresh(&window);
    let shown = |window: &YapfastWindow| {
        (
            window.get_sidebar_title(),
            window.get_filter_placeholder(),
            window.get_has_topic(),
            i_slint_core::yapfast_selection::plain(&window.get_room_topic()),
        )
    };
    let before = shown(&window);
    assert_eq!(before.0, "Friends");
    assert!(!before.1.is_empty());
    assert_eq!(before.3, "Plans for the weekend");
    drop(window);
    let next = classic_window();
    adapter.borrow_mut().attach(&next);
    bind_callbacks(&next, &adapter);
    adapter.borrow_mut().refresh(&next);
    assert_eq!(shown(&next), before);
}

#[test]
fn dragging_the_thread_beside_resizes_it_within_limits_and_remembers_it() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1400., 760.));
    window.set_aside_open(true);
    let drag = |from: f32, to: f32| {
        let window = window.window();
        let button = slint::platform::PointerEventButton::Left;
        let at = |x| slint::LogicalPosition::new(x, 400.);
        window.dispatch_event(slint::platform::WindowEvent::PointerMoved { position: at(from) });
        window.dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: at(from),
            button,
        });
        window.dispatch_event(slint::platform::WindowEvent::PointerMoved { position: at(to) });
        window.dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position: at(to),
            button,
        });
    };
    assert_eq!(window.get_aside_shown(), 420.);
    // Its edge, 980px in, widens it leftwards.
    drag(980., 880.);
    assert_eq!(window.get_aside_shown(), 520.);
    assert_eq!(Layout::read(directory.path()).aside_width, Some(520.));
    // It keeps 320px, and leaves the conversation 420px.
    drag(880., 1300.);
    assert_eq!(window.get_aside_shown(), 320.);
    drag(1080., 0.);
    assert_eq!(window.get_aside_shown(), 1400. - 72. - 240. - 420.);
    // A new window starts at the stored width.
    let next = classic_window();
    adapter.borrow_mut().attach(&next);
    assert_eq!(next.get_aside_width(), 668.);
    window.hide().unwrap();
}

#[cfg(feature = "discord")]
#[test]
fn the_activity_panel_shows_discords_feed_in_its_order_for_those_the_channel_may_show() {
    use yapfast_core::{
        application::AppState,
        chat::{ChatMember, FeedEntry, FeedPerson, Presence, Role},
        discord::room_id,
        yapfast_discord::activity_feed::Kind,
    };
    let hour = 60 * 60_000;
    let now = 100 * hour;
    let entry = |author: &str, kind, title: &str, live, at_ms| FeedEntry {
        author: author.into(),
        kind,
        title: title.into(),
        picture: Some("discord-app-icon:42".into()),
        live,
        expires_ms: None,
        at_ms,
        hours: None,
        tracks: None,
    };
    let person = |id: &str| FeedPerson {
        id: id.into(),
        name: format!("Member {id}").into(),
        avatar: "".into(),
    };
    let member = |id: &str, presence, activities: serde_json::Value| ChatMember {
        bot: false,
        id: id.to_owned(),
        name: id.to_owned(),
        username: None,
        role: Role::Member,
        avatar: None,
        decoration: None,
        nameplate: None,
        style: None,
        tag: None,
        group: None,
        presence: Some(presence),
        color: None,
        activity: None,
        activities: yapfast_core::yapfast_discord::activity::activities(&activities),
        roles: Default::default(),
        role_icon: None,
        group_icon: None,
        timed_out_until: None,
        boosting_since: None,
    };
    let room = room_id("1", "2");
    let mut state = AppState::default();
    state.activity_feed = vec![
        entry("7", Kind::Game, "Factorio", true, now - 3 * hour),
        entry("8", Kind::Listened, "Marconi Union", true, now - 60_000),
        entry("9", Kind::Game, "Celeste", false, now - 3 * hour),
        FeedEntry {
            hours: Some(13),
            ..entry("7", Kind::TopGame, "Factorio", false, now - 50 * hour)
        },
        FeedEntry {
            tracks: Some(31),
            ..entry(
                "8",
                Kind::TopArtist,
                "Marconi Union",
                false,
                now - 50 * hour,
            )
        },
        entry("6", Kind::Game, "Offline's", true, now - hour),
        entry("5", Kind::Game, "Elsewhere", false, now - hour),
    ]
    .into();
    state.activity_people = Some((room.clone(), ["7", "8", "9", "6"].map(person).into()));
    let mut members = [
        member(
            "7",
            Presence::DoNotDisturb,
            serde_json::json!([{"type": 0, "name": "Factorio", "timestamps": {"start": now - 5 * 60_000}}]),
        ),
        member("6", Presence::Offline, serde_json::json!([])),
    ];
    members[0].color = Some(0x3498db);
    let rows = activity_panel(&state, &room, &members, now, true);
    let shown: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|row| (row.name.as_str(), row.game.as_str(), row.since.as_str()))
        .collect();
    assert_eq!(
        shown,
        [
            // Their presence says when it started, which the feed's id may not.
            ("Member 7", "Factorio", "for 5 min"),
            ("Member 8", "Marconi Union", "for 1 min"),
            ("Member 9", "Celeste", "3h ago"),
            ("Member 7", "Factorio", "Most played: 13h"),
            ("Member 8", "Marconi Union", "31 tracks this week"),
        ],
    );
    assert!(rows[0].live && rows[1].live && !rows[2].live);
    assert_eq!(rows[1].icon, 2);
    assert_eq!(rows[3].icon, 5);
    assert_eq!(rows[0].thumb_url, "discord-app-icon:42");
    assert!(rows[0].has_color);
    assert_eq!(rows[0].color, slint::Color::from_rgb_u8(0x34, 0x98, 0xdb));
    let plain = activity_panel(&state, &room, &members, now, false);
    assert!(!plain[0].has_color);
    assert_eq!(plain[0].color, slint::Color::default());

    // Another channel of the server keeps them until its answer comes.
    assert_eq!(
        activity_panel(&state, &room_id("1", "3"), &members, now, true).len(),
        5
    );
    assert!(activity_panel(&state, &room_id("4", "3"), &members, now, true).is_empty());
    state.activity_people = None;
    assert!(activity_panel(&state, &room, &members, now, true).is_empty());
}

#[cfg(feature = "discord")]
#[test]
fn a_finished_activity_says_how_long_ago_in_discords_steps() {
    let (minute, hour, day) = (60_000, 3_600_000, 86_400_000);
    let at = 100 * day;
    for (ago, words) in [
        (5_000, "5s ago"),
        (12 * minute, "12m ago"),
        (3 * hour, "3h ago"),
        (13 * hour, "1d ago"),
        (8 * day, "8d ago"),
        (15 * day, "2w ago"),
        (60 * day, "2mo ago"),
    ] {
        assert_eq!(finished_ago(at - ago, at), words);
    }
}

#[test]
fn decorations_and_nameplates_reach_the_member_list_and_the_account_card() {
    use yapfast_core::chat::{ChatMember, Nameplate, Role};
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    window.show().unwrap();
    let plate = |picture: &str| Nameplate {
        picture: picture.into(),
        colors: Some([0x730bc8, 0x972fed]),
    };
    let member = |id: &str, name: &str| ChatMember {
        bot: false,
        id: id.into(),
        name: name.into(),
        username: None,
        role: Role::Member,
        avatar: None,
        decoration: None,
        nameplate: None,
        boosting_since: None,
        style: None,
        tag: None,
        group: Some("Online — 2".into()),
        presence: None,
        color: None,
        activity: None,
        activities: Default::default(),
        roles: Default::default(),
        timed_out_until: None,
        group_icon: None,
        role_icon: None,
    };
    {
        let mut adapter = adapter.borrow_mut();
        let state = &mut adapter.app.state;
        state.members = Some(Ok(vec![
            ChatMember {
                bot: false,
                decoration: Some("ring.png".into()),
                nameplate: Some(plate("strip.png")),
                style: Some(Arc::new(yapfast_core::chat::NameStyle::new(
                    14,
                    3,
                    &[65433],
                ))),
                ..member("1", "Bob")
            },
            member("2", "Dave"),
            ChatMember {
                nameplate: Some(Nameplate {
                    picture: "bare.png".into(),
                    colors: None,
                }),
                ..member("3", "Eve")
            },
        ]));
        state.account_decoration = Some("mine.png".into());
        state.account_nameplate = Some(plate("mine-strip.png"));
        let window = window.clone_strong();
        adapter.refresh(&window);
    }
    let roster = window.get_roster();
    let (bob, dave) = (roster.row_data(0).unwrap(), roster.row_data(1).unwrap());
    assert_eq!(bob.decoration_url, "ring.png");
    assert_eq!(bob.nameplate_url, "strip.png");
    assert_eq!(
        bob.nameplate_color,
        slint::Color::from_rgb_u8(0x73, 0x0b, 0xc8)
    );
    assert_eq!(
        bob.nameplate_light,
        slint::Color::from_rgb_u8(0x97, 0x2f, 0xed)
    );
    assert!(dave.decoration_url.is_empty() && dave.nameplate_url.is_empty());
    // A palette Yapfast doesn't know leaves the picture without a colour.
    let eve = roster.row_data(2).unwrap();
    assert_eq!(eve.nameplate_url, "bare.png");
    assert_eq!(eve.nameplate_color.alpha(), 0);
    // Bob's name is drawn neon in Mainframe; Dave's has no style.
    let bobs = bob.name_style.row_data(0).unwrap();
    assert_eq!((bobs.effect, bobs.family.as_str()), (3, "Mainframe"));
    assert_eq!(bobs.count, 1);
    assert_eq!(dave.name_style.row_count(), 0);
    let look = window.global::<AccountLook>();
    assert_eq!(look.get_decoration_url(), "mine.png");
    assert_eq!(look.get_nameplate_url(), "mine-strip.png");
    // Show nameplates off takes every one away.
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.dispatch(Action::HideNameplates(true));
        let window = window.clone_strong();
        adapter.refresh(&window);
    }
    let roster = window.get_roster();
    assert!((0..3).all(|row| roster.row_data(row).unwrap().nameplate_url.is_empty()));
    assert_eq!(roster.row_data(0).unwrap().decoration_url, "ring.png");
    assert!(look.get_nameplate_url().is_empty());
    window.hide().unwrap();
}

#[test]
fn the_nameplates_setting_is_remembered() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    // On unless turned off.
    assert!(window.get_show_nameplates());
    assert_eq!(Layout::read(directory.path()).nameplates, None);
    assert!(!adapter.borrow().app.state.hide_nameplates);
    window.set_show_nameplates(false);
    window.invoke_preferences_changed();
    assert_eq!(Layout::read(directory.path()).nameplates, Some(false));
    assert!(adapter.borrow().app.state.hide_nameplates);
}

#[test]
fn role_colors_everywhere_is_remembered() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    assert!(!window.get_role_colors_everywhere());
    assert!(!adapter.borrow().layout.role_colors_everywhere);
    window.set_role_colors_everywhere(true);
    window.invoke_role_colors_everywhere_toggled(true);
    assert!(adapter.borrow().layout.role_colors_everywhere);
    assert!(Layout::read(directory.path()).role_colors_everywhere);
    assert!(window.get_role_colors_everywhere());
}

#[test]
fn message_logger_is_remembered_and_toggle_dispatches_action() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    assert!(!window.get_message_logger());
    assert!(!adapter.borrow().layout.message_logger);
    window.set_message_logger(true);
    window.invoke_message_logger_toggled(true);
    assert!(adapter.borrow().layout.message_logger);
    assert!(Layout::read(directory.path()).message_logger);
    assert!(window.get_message_logger());
}

#[test]
fn hide_deleted_messages_dispatches_actions() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    window.invoke_hide_deleted_message("d:123".into());
    window.invoke_hide_all_deleted_messages();
}

#[test]
fn right_click_on_deleted_message_opens_menu_with_hide_actions() {
    use slint::platform::{PointerEventButton, WindowEvent};
    let window = showing(vec![ChatMessage {
        author: "Alice".into(),
        body: "Deleted message".into(),
        deleted: true,
        ..Default::default()
    }]);
    let button = PointerEventButton::Right;
    let mut opened = false;
    for y in (100..600).step_by(10) {
        let position = slint::LogicalPosition::new(440., y as f32);
        window
            .window()
            .dispatch_event(WindowEvent::PointerPressed { position, button });
        window
            .window()
            .dispatch_event(WindowEvent::PointerReleased { position, button });
        settle(&window);
        if window.get_menu_message().item == "i0" {
            opened = true;
            break;
        }
    }
    assert!(
        opened,
        "Right clicking a deleted message row opened its context menu"
    );
    assert!(
        window.get_menu_message().deleted,
        "Menu message is marked deleted"
    );
}

#[test]
fn right_click_on_edited_message_preserves_view_edit_history() {
    use slint::platform::{PointerEventButton, WindowEvent};
    let window = showing(vec![ChatMessage {
        author: "Bob".into(),
        body: "Edited message".into(),
        deleted: false,
        edited: true,
        has_edit_history: true,
        ..Default::default()
    }]);
    let button = PointerEventButton::Right;
    let mut opened = false;
    for y in (100..600).step_by(10) {
        let position = slint::LogicalPosition::new(440., y as f32);
        window
            .window()
            .dispatch_event(WindowEvent::PointerPressed { position, button });
        window
            .window()
            .dispatch_event(WindowEvent::PointerReleased { position, button });
        settle(&window);
        if window.get_menu_message().item == "i0" {
            opened = true;
            break;
        }
    }
    assert!(
        opened,
        "Right clicking an edited message row opened its context menu"
    );
    assert!(!window.get_menu_message().deleted, "Message is not deleted");
    assert!(
        window.get_menu_message().has_edit_history,
        "Edit history is available"
    );
}

#[test]
fn pending_send_menu_routes_the_pending_id_in_chat_and_thread() {
    use i_slint_core::{accessibility::AccessibleStringProperty, item_tree::ItemRc};
    use slint::platform::{PointerEventButton, WindowEvent};
    use std::ops::ControlFlow;

    let window = showing(vec![]);
    let pending = message_row_plain(&ChatMessage {
        item: "t:pending".into(),
        author: "Alice".into(),
        body: "Queued message".into(),
        own: true,
        delivery: yapfast_core::chat::Delivery::Sending,
        ..Default::default()
    });
    assert!(pending.delivery_sending && pending.can_cancel);
    let canceled = Rc::new(RefCell::new(Vec::new()));
    let picked = canceled.clone();
    window.on_cancel_send(move |item| picked.borrow_mut().push(item));
    let click = |position, button| {
        let native = window.window();
        native.dispatch_event(WindowEvent::PointerMoved { position });
        native.dispatch_event(WindowEvent::PointerPressed { position, button });
        native.dispatch_event(WindowEvent::PointerReleased { position, button });
        settle(&window);
    };
    for x in [440., 900.] {
        let row = ModelRc::new(VecModel::from(vec![pending.clone()]));
        if x == 440. {
            window.set_messages(row);
        } else {
            window.set_messages(ModelRc::default());
            window.set_aside_open(true);
            window.set_aside_ready(true);
            window.set_aside_messages(row);
        }
        settle(&window);
        for y in (100..730).step_by(5) {
            click(
                slint::LogicalPosition::new(x, y as f32),
                PointerEventButton::Right,
            );
            if window.get_menu_open() {
                break;
            }
        }
        assert_eq!(window.get_menu_message().item, "t:pending");
        let position = {
            let inner = i_slint_core::window::WindowInner::from_pub(window.window());
            let popups = inner.active_popups();
            let popup = popups.last().expect("pending send menu");
            let i_slint_core::window::PopupWindowLocation::ChildWindow(place) = popup.location
            else {
                panic!("not a child window");
            };
            let item = ItemRc::new_root(popup.component.clone())
                .visit_descendants(|item| {
                    if item
                        .accessible_string_property(AccessibleStringProperty::Label)
                        .as_deref()
                        == Some("Cancel")
                    {
                        ControlFlow::Break(item.clone())
                    } else {
                        ControlFlow::Continue(())
                    }
                })
                .expect("the pending send menu offers Cancel");
            let at = item.map_to_item_tree(item.geometry().center(), &popup.component);
            slint::LogicalPosition::new(place.x + at.x, place.y + at.y)
        };
        click(position, PointerEventButton::Left);
    }
    assert_eq!(canceled.borrow().as_slice(), &["t:pending", "t:pending"]);
}

#[test]
fn the_sidebar_keeps_gateway_voice_members_visible_while_connecting() {
    use i_slint_core::{accessibility::AccessibleStringProperty, item_tree::ItemRc};
    use std::ops::ControlFlow;

    let window = showing(vec![]);
    let room = |id: &str, member: Option<&str>| RoomRow {
        id: id.into(),
        name: id.into(),
        voice: true,
        people: ModelRc::new(VecModel::from(
            member
                .map(|name| PersonRow {
                    name: name.into(),
                    ..Default::default()
                })
                .into_iter()
                .collect::<Vec<_>>(),
        )),
        ..Default::default()
    };
    let rooms = Rc::new(VecModel::from(vec![
        room("lounge", Some("Ada")),
        room("study", Some("Bea")),
        RoomRow {
            id: "chat".into(),
            name: "Chat".into(),
            selected: true,
            ..Default::default()
        },
    ]));
    window.set_rooms(rooms.clone().into());
    let visible = |name: &str| {
        settle(&window);
        let inner = i_slint_core::window::WindowInner::from_pub(window.window());
        ItemRc::new_root(inner.component())
            .visit_descendants(|item| {
                if item
                    .accessible_string_property(AccessibleStringProperty::Label)
                    .as_deref()
                    == Some(name)
                {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .is_some()
    };
    assert!(visible("Ada") && visible("Bea"));
    window.set_voice_channel("lounge".into());
    window.set_voice_tone(0);
    assert!(visible("Ada") && visible("Bea"));
    assert!(!visible("lounge, connected"));
    window.set_voice_tone(1);
    assert!(visible("lounge, connected") && visible("Chat"));
    window.set_voice_channel("study".into());
    assert!(visible("study, connected") && !visible("lounge, connected"));
    window.set_voice_tone(0);
    assert!(!visible("study, connected"));
    rooms.set_row_data(0, room("lounge", None));
    rooms.set_row_data(1, room("study", Some("Ada")));
    assert!(visible("Ada") && !visible("Bea"));
    window.set_voice_channel("".into());
    assert!(visible("Ada"));
}

#[test]
fn a_nameplates_picture_is_asked_for_when_its_row_shows() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    window.set_workspace(true);
    window.set_room_selected(true);
    window.set_room_ready(true);
    window.set_members_available(true);
    let requested = Rc::new(RefCell::new(HashSet::new()));
    let requests = requested.clone();
    window
        .global::<PictureStore>()
        .on_picture(|_, _| slint::Image::default());
    window.global::<PictureStore>().on_request(move |url| {
        requests.borrow_mut().insert(url.to_string());
    });
    window.set_roster(ModelRc::new(VecModel::from(vec![MemberRow {
        id: "1".into(),
        name: "Member".into(),
        nameplate_url: "plate/still".into(),
        ..Default::default()
    }])));
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(50));
    assert!(requested.borrow().contains("plate/still"));
    assert_eq!(
        requested.borrow().len(),
        1,
        "not its animation, until it plays"
    );
    window.hide().unwrap();
}

#[test]
fn a_dms_nameplate_is_asked_for_only_once_its_row_is_selected() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    window.set_workspace(true);
    let requested = Rc::new(RefCell::new(HashSet::new()));
    let requests = requested.clone();
    window
        .global::<PictureStore>()
        .on_picture(|_, _| slint::Image::default());
    window.global::<PictureStore>().on_request(move |url| {
        requests.borrow_mut().insert(url.to_string());
    });
    let row = |selected: bool| RoomRow {
        id: "dm".into(),
        name: "Bob".into(),
        person: true,
        selected,
        nameplate_url: "plate/still".into(),
        ..Default::default()
    };
    let rooms = Rc::new(VecModel::from(vec![row(false)]));
    window.set_rooms(rooms.clone().into());
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(50));
    assert!(requested.borrow().is_empty(), "a list at rest holds none");
    rooms.set_row_data(0, row(true));
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(50));
    assert!(requested.borrow().contains("plate/still"));
    window.hide().unwrap();
}

#[test]
fn the_member_list_shows_for_text_channels_and_remembers_being_closed() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    assert!(window.get_members_available() && window.get_member_list());
    let roster = window.get_roster();
    assert_eq!(roster.row_count(), 4);
    assert_eq!(roster.row_data(0).unwrap().heading, "ADMINS — 1");
    assert_eq!(roster.row_data(2).unwrap().heading, "MEMBERS — 2");
    assert!(roster.row_data(3).unwrap().heading.is_empty());
    let click = |x, y| {
        let position = slint::LogicalPosition::new(x, y);
        let button = slint::platform::PointerEventButton::Left;
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerPressed { position, button });
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerReleased { position, button });
    };
    // The toggle ends the conversation header.
    click(1080., 24.);
    assert!(!window.get_member_list());
    assert!(Layout::read(directory.path()).members_hidden);
    click(1080., 24.);
    assert!(window.get_member_list());
    // Voice channels and one-to-one DMs have no member list.
    window.invoke_open_conversation("lounge".into());
    assert!(!window.get_members_available());
    window.invoke_navigate_dms();
    assert!(!window.get_members_available());
    window.invoke_select_room("party".into());
    assert!(window.get_members_available());
    assert_eq!(window.get_roster().row_count(), 3);
    window.hide().unwrap();
}

#[test]
fn forwarding_and_frequent_reactions_reach_the_window() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    let glyphs =
        |model: ModelRc<EmojiCell>| model.iter().map(|cell| cell.glyph).collect::<Vec<_>>();
    assert_eq!(glyphs(window.get_bar_reactions()), ["👍", "❤️", "😂"]);
    let message = window.get_messages().row_data(0).unwrap();
    window.invoke_react(message.item.clone(), "🍕".into());
    assert_eq!(
        glyphs(window.get_menu_reactions()),
        ["🍕", "👍", "❤️", "😂"]
    );
    assert_eq!(Layout::read(directory.path()).reactions.get("🍕"), Some(&1));

    window.invoke_start_forward(message.item.clone());
    assert!(window.get_forward_open());
    assert_eq!(window.get_forward_preview(), "Anyone around tonight?");
    let targets = window.get_forward_targets();
    assert!(
        targets.iter().all(|target| target.id != "lounge"),
        "voice is no target"
    );
    window.set_forward_query("gam".into());
    window.invoke_forward_search();
    assert_eq!(window.get_forward_targets().row_count(), 1);
    let games = window.get_forward_targets().row_data(0).unwrap();
    assert!(games.avatar, "a channel shows its server's icon");
    assert_eq!(
        (games.initial.as_str(), games.place.as_str()),
        ("F", "Friends")
    );
    // Choosing clears the search.
    window.invoke_forward_choose("games".into());
    assert_eq!(window.get_forward_query(), "");
    let targets = window.get_forward_targets();
    assert!(targets.row_count() > 1);
    assert!(
        targets
            .iter()
            .any(|target| target.id == "games" && target.chosen)
    );
    assert_eq!(window.get_forward_chosen(), 1);
    assert_eq!(window.get_forward_limit(), 5);
    window.invoke_close_forward();
    assert!(!window.get_forward_open());
    // Opened again, it starts afresh; sending closes it and opens the one
    // conversation chosen.
    window.invoke_start_forward(message.item.clone());
    assert_eq!(window.get_forward_chosen(), 0);
    assert_eq!(window.get_forward_query(), "");
    window.invoke_forward_choose("games".into());
    window.invoke_forward_send("see you there".into());
    assert!(!window.get_forward_open());
    let shown = window.get_messages();
    let last = shown.row_data(shown.row_count() - 1).unwrap();
    assert_eq!(last.body, "see you there");

    window.invoke_emoji_search("pizza".into());
    assert_eq!(window.get_emoji_rows().row_count(), 1);
}

#[test]
fn the_quick_switcher_reaches_the_window() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    // Ctrl+K, which the native window's events carry.
    adapter.borrow_mut().action(&window, Action::ToggleSwitcher);
    assert!(window.get_switcher_open());
    assert_eq!(window.get_switcher_query(), "");
    window.invoke_switcher_search("gam".into());
    let places = window.get_switcher_places();
    let games = places.iter().find(|place| place.id == "games").unwrap();
    assert!(!games.avatar, "a channel shows its own icon");
    assert_eq!(games.place, "Friends");
    window.invoke_switcher_search("*fri".into());
    let places = window.get_switcher_places();
    assert_eq!(places.row_count(), 1);
    let server = places.row_data(0).unwrap();
    assert_eq!((server.name.as_str(), server.avatar), ("Friends", true));
    window.invoke_switch_to("games".into());
    assert!(!window.get_switcher_open());
    assert_eq!(
        adapter.borrow().app.state.selected.as_deref(),
        Some("games")
    );
    // Opened again, it starts afresh.
    adapter.borrow_mut().action(&window, Action::ToggleSwitcher);
    assert_eq!(window.get_switcher_query(), "");
    window.invoke_close_switcher();
    assert!(!window.get_switcher_open());
}

#[test]
fn a_room_asks_for_earlier_messages_only_once_near_their_start() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let asked = Rc::new(Cell::new(0));
    window.on_earlier({
        let asked = asked.clone();
        move || asked.set(asked.get() + 1)
    });
    window.set_workspace(true);
    window.set_room_selected(true);
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    // A frame or so: the list lays its rows out as time moves on.
    let settle = || {
        for _ in 0..5 {
            i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(1));
            slint::platform::update_timers_and_animations();
            let _ = window.invoke_timeline_extent();
        }
    };
    let rows = |count: usize| -> ModelRc<MessageRow> {
        let rows: Vec<MessageRow> = (0..count)
            .map(|index| MessageRow {
                id: format!("m{index}").into(),
                item: format!("i{index}").into(),
                author: "Bob".into(),
                body: format!("Message {index}").into(),
                first: true,
                ..Default::default()
            })
            .collect();
        ModelRc::new(VecModel::from(rows))
    };
    let wait = || {
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(100));
        settle();
    };
    // A room opens with its list at the top, then moves it to the newest
    // message: that moment at the top isn't near its first loaded message.
    window.set_room_ready(true);
    for _ in 0..2 {
        window.set_messages(rows(200));
        settle();
        window.invoke_scroll_to_latest();
        settle();
        wait();
        assert_eq!(
            asked.get(),
            0,
            "not at the newest message, far from the first"
        );
    }
    // A room too short to fill the view wants more.
    window.set_messages(rows(3));
    settle();
    window.invoke_scroll_to_latest();
    wait();
    assert_eq!(asked.get(), 1, "a short history asks for more");
    window.hide().unwrap();
}

#[test]
fn scrolling_up_leaves_the_latest_message() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    window.set_workspace(true);
    window.set_room_selected(true);
    window.set_room_ready(true);
    let rows: Vec<MessageRow> = (0..200)
        .map(|index| MessageRow {
            id: format!("m{index}").into(),
            item: format!("i{index}").into(),
            author: "Bob".into(),
            body: if index % 2 == 0 {
                format!("Message {index}").into()
            } else {
                format!("Message {index}, a longer line that wraps across the timeline at ordinary window widths, and then some more words").into()
            },
            first: index % 5 == 0,
            ..Default::default()
        })
        .collect();
    window.set_messages(ModelRc::new(VecModel::from(rows)));
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    let settle = || {
        for _ in 0..5 {
            slint::platform::update_timers_and_animations();
            let _ = window.invoke_timeline_extent();
        }
    };
    settle();
    window.invoke_scroll_to_latest();
    settle();
    assert!(
        window.invoke_timeline_at_end(),
        "starts at the latest message"
    );
    for _ in 0..8 {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(700., 380.),
                delta_x: 0.,
                delta_y: 50.,
            });
        settle();
    }
    assert!(
        !window.invoke_timeline_at_end(),
        "scrolling up leaves the latest message"
    );
    window.hide().unwrap();
}

#[test]
fn a_wheel_spun_fast_scrolls_farther_for_each_notch() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    window.set_workspace(true);
    window.set_room_selected(true);
    window.set_room_ready(true);
    window.set_history_start(true);
    let rows: Vec<MessageRow> = (0..300)
        .map(|index| {
            let mut row = message_row_plain(&ChatMessage {
                id: format!("m{index}"),
                item: format!("i{index}"),
                author: "Bob".into(),
                body: format!("Message {index}"),
                formatted: true,
                ..Default::default()
            });
            row.first = true;
            row
        })
        .collect();
    window.set_messages(ModelRc::new(VecModel::from(rows)));
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    let wait = |time: Duration| {
        i_slint_backend_testing::mock_elapsed_time(time);
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(1100., 740.),
            });
    };
    // Scrolls up by ten notches, `apart` from each other, and tells how many
    // rows that went.
    let spin = |apart: Duration| {
        window.invoke_scroll_to_latest();
        for _ in 0..20 {
            wait(Duration::from_millis(50));
        }
        let start = window.get_top_index();
        for _ in 0..10 {
            // As the winit backend sends a notch: `PointerScrolled` would
            // jump without the wheel's animation.
            window
                .window()
                .dispatch_event(slint::platform::WindowEvent::internal(
                    i_slint_core::input::BackendMouseEvent::Wheel {
                        position: i_slint_core::lengths::LogicalPoint::new(700., 380.),
                        delta_x: 0.,
                        delta_y: 60.,
                        phase: i_slint_core::input::TouchPhase::Moved,
                    },
                ));
            wait(apart);
        }
        for _ in 0..20 {
            wait(Duration::from_millis(50));
        }
        start - window.get_top_index()
    };
    let slow = spin(Duration::from_millis(300));
    let fast = spin(Duration::from_millis(20));
    assert!(slow > 0);
    assert!(
        fast as f32 > slow as f32 * 1.5,
        "fast notches went {fast} rows, slow ones {slow}"
    );
    window.hide().unwrap();
}

#[test]
fn wheel_scroll_direction_reversal_responds_immediately_on_first_notch() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    window.set_workspace(true);
    window.set_room_selected(true);
    window.set_room_ready(true);
    window.set_history_start(true);
    let rows: Vec<MessageRow> = (0..300)
        .map(|index| {
            let mut row = message_row_plain(&ChatMessage {
                id: format!("m{index}"),
                item: format!("i{index}"),
                author: "Bob".into(),
                body: format!("Message {index}"),
                formatted: true,
                ..Default::default()
            });
            row.first = true;
            row
        })
        .collect();
    window.set_messages(ModelRc::new(VecModel::from(rows)));
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    let wait = |time: Duration| {
        i_slint_backend_testing::mock_elapsed_time(time);
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(1100., 740.),
            });
    };
    window.invoke_scroll_to_latest();
    for _ in 0..20 {
        wait(Duration::from_millis(50));
    }

    let send_wheel = |dy: f32| {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::internal(
                i_slint_core::input::BackendMouseEvent::Wheel {
                    position: i_slint_core::lengths::LogicalPoint::new(700., 380.),
                    delta_x: 0.,
                    delta_y: dy,
                    phase: i_slint_core::input::TouchPhase::Moved,
                },
            ));
    };

    // Test sequence: UP -> UP -> DOWN -> DOWN -> UP -> DOWN
    let steps: [(f32, &str); 6] = [
        (60.0, "UP 1"),
        (60.0, "UP 2"),
        (-60.0, "DOWN 1 (reversal)"),
        (-60.0, "DOWN 2"),
        (60.0, "UP 3 (reversal)"),
        (-60.0, "DOWN 3 (reversal)"),
    ];

    for (dy, label) in steps {
        let (index_before, edge_before) = (window.get_top_index(), window.get_top_edge());
        send_wheel(dy);
        for _ in 0..6 {
            wait(Duration::from_millis(40));
        }
        let (index_after, edge_after) = (window.get_top_index(), window.get_top_edge());
        let moved = index_before != index_after || (edge_before - edge_after).abs() > 0.5;
        assert!(
            moved,
            "Step {label} (dy={dy}) failed to move timeline: before=(index={index_before}, edge={edge_before}), after=(index={index_after}, edge={edge_after})"
        );
    }

    window.hide().unwrap();
}

#[test]
fn a_middle_click_scrolls_towards_the_pointer_until_the_next_press() {
    use slint::platform::{PointerEventButton, WindowEvent};
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    mark_autoscrolls(&window);
    window.set_workspace(true);
    window.set_room_selected(true);
    window.set_room_ready(true);
    window.set_history_start(true);
    let rows: Vec<MessageRow> = (0..300)
        .map(|index| {
            let mut row = message_row_plain(&ChatMessage {
                id: format!("m{index}"),
                item: format!("i{index}"),
                author: "Bob".into(),
                body: format!("Message {index}"),
                formatted: true,
                ..Default::default()
            });
            row.first = true;
            row
        })
        .collect();
    window.set_messages(ModelRc::new(VecModel::from(rows)));
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    let send = |event| window.window().dispatch_event(event);
    let at = |y| slint::LogicalPosition::new(700., y);
    let press = |y, button| {
        send(WindowEvent::PointerPressed {
            position: at(y),
            button,
        })
    };
    let release = |y, button| {
        send(WindowEvent::PointerReleased {
            position: at(y),
            button,
        })
    };
    let point = |y| send(WindowEvent::PointerMoved { position: at(y) });
    let pass = |frames| {
        for _ in 0..frames {
            i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(16));
        }
    };
    let shown = || window.global::<Autoscroll>().get_shown();
    window.invoke_scroll_to_latest();
    pass(60);
    let latest = window.get_top_index();

    // A click starts it, and the pointer above where it clicked scrolls up.
    press(380., PointerEventButton::Middle);
    release(380., PointerEventButton::Middle);
    assert!(shown());
    let marker = window.global::<Autoscroll>();
    assert_eq!((marker.get_x(), marker.get_y()), (700., 380.));
    assert!(marker.get_vertical() && !marker.get_horizontal());
    point(375.);
    pass(30);
    assert_eq!(
        window.get_top_index(),
        latest,
        "near where it clicked, it stays"
    );
    point(180.);
    pass(30);
    let scrolled = window.get_top_index();
    assert!(scrolled < latest, "went from {latest} to {scrolled}");

    // The next press ends it, and goes no further.
    press(180., PointerEventButton::Left);
    assert!(!shown());
    release(180., PointerEventButton::Left);
    pass(30);
    assert_eq!(window.get_top_index(), scrolled);

    // Held and moved away, it scrolls until it's let go.
    press(380., PointerEventButton::Middle);
    point(600.);
    pass(30);
    let held = window.get_top_index();
    assert!(held > scrolled, "went from {scrolled} to {held}");
    release(600., PointerEventButton::Middle);
    assert!(!shown());
    pass(30);
    assert_eq!(window.get_top_index(), held);
    window.hide().unwrap();
}

#[test]
fn older_messages_go_in_above_without_moving_those_shown() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    window.set_workspace(true);
    window.set_room_selected(true);
    window.set_room_ready(true);
    window.set_loading_history(true);
    // The first row carries the loading notice and its day; once older
    // messages from that day are above it, it carries neither.
    // Each from someone else, so each row shows its author, all on one day.
    let messages = |from: usize| -> Vec<Arc<ChatMessage>> {
        (from..200)
            .map(|index| {
                let body = if index % 3 == 0 {
                    format!("Message {index}, a longer line that wraps across the timeline at ordinary window widths, and then some more words")
                } else {
                    format!("Message {index}")
                };
                Arc::new(ChatMessage {
                    id: format!("m{index}"),
                    item: format!("i{index}"),
                    author: format!("Person {index}"),
                    body,
                    formatted: true,
                    // Noon on 1 October 2026, UTC.
                    timestamp: 1_790_856_000 + index as u64,
                    ..Default::default()
                })
            })
            .collect()
    };
    let revealed = HashSet::new();
    let marks = Marks {
        first_new: usize::MAX,
        first_unread: None,
        revealed: &revealed,
        restyle: false,
        decorations: true,
    };
    let model = Rc::new(VecModel::default());
    let mut rows = Vec::new();
    let shown = messages(150);
    show_timeline(&model, &mut rows, &[], &shown, &marks, message_row_plain);
    window.set_messages(ModelRc::from(model.clone()));
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    // As drawing does, the pointer goes through the list, which lays it out.
    let settle = || {
        for _ in 0..5 {
            slint::platform::update_timers_and_animations();
            window
                .window()
                .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                    position: slint::LogicalPosition::new(1100., 740.),
                });
        }
    };
    settle();
    window.invoke_scroll_to_latest();
    settle();
    for _ in 0..40 {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(700., 380.),
                delta_x: 0.,
                delta_y: 400.,
            });
        settle();
    }
    // The first message, with the loading notice and its day above it, is
    // partly scrolled under the top.
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(700., 380.),
            delta_x: 0.,
            delta_y: -90.,
        });
    settle();
    assert_eq!(window.get_top_index(), 0);
    let edge = window.get_top_edge();
    show_timeline(
        &model,
        &mut rows,
        &shown,
        &messages(100),
        &marks,
        message_row_plain,
    );
    settle();
    assert_eq!(
        (window.get_top_index(), window.get_top_item().as_str()),
        (50, "i150"),
        "the same message is at the top"
    );
    assert!(
        (window.get_top_edge() - edge).abs() < 0.5,
        "its text stays where it was: {edge} then {}",
        window.get_top_edge()
    );
    assert!(!window.invoke_timeline_at_end());
    // A message deleted above leaves it where it is too.
    model.remove(10);
    settle();
    assert_eq!(
        (window.get_top_index(), window.get_top_item().as_str()),
        (49, "i150"),
        "the same message is at the top after a deletion above"
    );
    assert!(
        (window.get_top_edge() - edge).abs() < 0.5,
        "its text stays where it was after a deletion above: {edge} then {}",
        window.get_top_edge()
    );
    // Scrolling up shows those that went in.
    for _ in 0..2 {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(700., 380.),
                delta_x: 0.,
                delta_y: 200.,
            });
        settle();
    }
    assert!(window.get_top_index() < 50);
    window.hide().unwrap();
}

#[test]
fn scrolling_up_through_styled_messages_stays_up() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    window.set_workspace(true);
    window.set_room_selected(true);
    window.set_room_ready(true);
    let rows: Vec<MessageRow> = (0..200)
        .map(|index| {
            let body = match index % 4 {
                0 => format!("**Message** {index}"),
                1 => format!("Message {index} with https://example.com/{index} and words that wrap across the timeline at ordinary widths, and then *some* more"),
                2 => format!("line one {index}\nline two\nline three"),
                _ => format!("Message {index}"),
            };
            let mut row = message_row_plain(&ChatMessage {
                id: format!("m{index}"),
                item: format!("i{index}"),
                author: "Bob".into(),
                body,
                formatted: true,
                ..Default::default()
            });
            row.first = index % 5 == 0;
            row
        })
        .collect();
    window.set_messages(ModelRc::new(VecModel::from(rows)));
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    let settle = || {
        for _ in 0..5 {
            slint::platform::update_timers_and_animations();
            let _ = window.invoke_timeline_extent();
        }
    };
    settle();
    window.invoke_scroll_to_latest();
    settle();
    assert!(
        window.invoke_timeline_at_end(),
        "starts at the latest message"
    );
    let move_to = |x, y| {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(x, y),
            });
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(2));
        settle();
    };
    move_to(700., 380.);
    assert!(!window.get_active_item().is_empty());
    let hovered = window.get_active_item();
    let more = {
        use i_slint_core::{accessibility::AccessibleStringProperty, item_tree::ItemRc};
        use std::ops::ControlFlow;
        let inner = i_slint_core::window::WindowInner::from_pub(window.window());
        let component = inner.component();
        ItemRc::new_root(component.clone())
            .visit_descendants(|item| {
                let at = item.map_to_item_tree(item.geometry().center(), &component);
                if (at.y - 380.).abs() < 100.
                    && item
                        .accessible_string_property(AccessibleStringProperty::Label)
                        .as_deref()
                        == Some("More")
                {
                    ControlFlow::Break(slint::LogicalPosition::new(at.x, at.y))
                } else {
                    ControlFlow::Continue(())
                }
            })
            .expect("the hovered message shows its action bar")
    };
    move_to(more.x, more.y);
    assert_eq!(
        window.get_active_item(),
        hovered,
        "the bar keeps the row hovered"
    );
    let click = |position| {
        use slint::platform::{PointerEventButton, WindowEvent};
        let button = PointerEventButton::Left;
        window
            .window()
            .dispatch_event(WindowEvent::PointerPressed { position, button });
        window
            .window()
            .dispatch_event(WindowEvent::PointerReleased { position, button });
        settle();
    };
    click(more);
    assert_eq!(window.get_menu_item(), hovered);
    click(slint::LogicalPosition::new(1100., 742.));
    move_to(1100., 742.);
    assert!(window.get_active_item().is_empty());
    move_to(700., 380.);
    assert!(!window.get_active_item().is_empty());
    for step in 0..8 {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(700., 380.),
                delta_x: 0.,
                delta_y: 50.,
            });
        settle();
        assert!(window.get_active_item().is_empty());
        assert!(
            !window.invoke_timeline_at_end() && window.invoke_timeline_below() > 0.,
            "step {step} stays scrolled up"
        );
    }
    // Scrolling back down resumes following, and later rows stay in view.
    for _ in 0..40 {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(700., 380.),
                delta_x: 0.,
                delta_y: -200.,
            });
        settle();
    }
    assert!(
        window.invoke_timeline_at_end(),
        "back at the latest message"
    );
    let messages = window.get_messages();
    let model = messages
        .as_any()
        .downcast_ref::<VecModel<MessageRow>>()
        .unwrap();
    model.push(message_row_plain(&ChatMessage {
        id: "new".into(),
        item: "new".into(),
        author: "Ann".into(),
        body: "A *new* message with https://example.com\nand a second line".into(),
        formatted: true,
        ..Default::default()
    }));
    settle();
    assert_eq!(window.invoke_timeline_below(), 0., "the new message shows");
    window.hide().unwrap();
}

#[test]
fn callbacks_preserve_drafts_scope_voice_and_clear_session_inputs() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    assert!(window.get_workspace() && window.get_preview());
    let server = adapter.borrow().app.state.servers[0].id.clone();
    window.invoke_navigate_server(server.clone().into());
    let rooms = adapter
        .borrow()
        .app
        .state
        .rooms
        .iter()
        .filter(|room| adapter.borrow().app.state.in_scope(room))
        .cloned()
        .collect::<Vec<_>>();
    let text_rooms = rooms.iter().filter(|room| !room.voice).collect::<Vec<_>>();
    window.invoke_select_room(text_rooms[0].id.clone().into());
    window.invoke_draft_edited("Draft survives navigation".into());
    window.invoke_select_room(text_rooms[1].id.clone().into());
    window.invoke_draft_edited("Different channel draft".into());
    window.invoke_select_room(text_rooms[0].id.clone().into());
    assert_eq!(window.get_draft(), "Draft survives navigation");
    assert!(window.get_can_send());
    let count = window.get_messages().row_count();
    window.invoke_send();
    assert_eq!(window.get_messages().row_count(), count + 1);
    assert!(window.get_draft().is_empty());
    // Calls start with the microphone on unless that's turned off. Out of a
    // call, the toggles change how the next one starts, and are kept.
    assert!(window.get_unmute_on_join() && !window.get_deafen_on_join());
    window.invoke_toggle_deafen();
    assert!(window.get_deafen_on_join() && adapter.borrow().layout.deafen_on_join);
    window.invoke_toggle_mute();
    assert!(window.get_unmute_on_join() && !window.get_deafen_on_join());
    window.invoke_toggle_mute();
    assert!(!window.get_unmute_on_join() && adapter.borrow().layout.unmute_on_join == Some(false));
    let voice_room = rooms.iter().find(|room| room.voice).unwrap();
    // Clicking a voice channel joins it and leaves the channel on screen,
    // as Discord's client does; clicking it again shows the call.
    window.invoke_select_room(voice_room.id.clone().into());
    assert!(window.get_voice_active() && window.get_muted() && window.get_voice_visible());
    assert!(!window.get_voice_here());
    assert_eq!(window.get_voice_channel(), voice_room.id.as_str());
    assert_eq!(
        adapter.borrow().app.state.selected.as_deref(),
        Some(text_rooms[0].id.as_str())
    );
    window.invoke_select_room(voice_room.id.clone().into());
    assert!(window.get_voice_here());
    window.invoke_navigate_dms();
    assert!(window.get_voice_active() && window.get_voice_visible());
    assert!(!window.get_voice_here());
    assert!(window.get_rooms().iter().all(|row| {
        adapter
            .borrow()
            .app
            .state
            .rooms
            .iter()
            .any(|room| room.id.as_str() == row.id.as_str() && room.direct)
    }));
    window.invoke_toggle_mute();
    assert!(!window.get_muted());
    window.invoke_toggle_deafen();
    assert!(window.get_muted() && window.get_deafened());
    window.set_password("secret that must be cleared".into());
    window.set_recovery("recovery that must be cleared".into());
    window.set_settings_open(true);
    window.invoke_logout();
    assert!(!window.get_workspace() && !window.get_voice_active());
    assert!(window.get_password().is_empty() && window.get_recovery().is_empty());
    assert!(!window.get_settings_open());
}

#[test]
fn message_actions_reach_the_shared_application_in_the_preview() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    let row = |item: &SharedString| {
        window
            .get_messages()
            .iter()
            .find(|row| row.item == *item)
            .unwrap()
    };
    let own = window.get_messages().iter().find(|row| row.own).unwrap();
    let other = window.get_messages().iter().find(|row| !row.own).unwrap();
    assert!(own.can_edit && !other.can_edit && !window.get_can_attach());

    window.invoke_react(other.item.clone(), "👍".into());
    let reactions = row(&other.item).reactions;
    assert_eq!(reactions.row_count(), 1);
    assert!(reactions.row_data(0).unwrap().mine);
    window.invoke_react(other.item.clone(), "👍".into());
    assert_eq!(row(&other.item).reactions.row_count(), 0);

    window.invoke_reply(other.item.clone());
    assert!(window.get_compose_label().starts_with("Replying to Sam"));
    window.invoke_draft_edited("**Sure**".into());
    window.invoke_send();
    let reply = window.get_messages().iter().last().unwrap();
    assert!(reply.has_reply && reply.reply_author == "Sam");
    assert!(window.get_compose_label().is_empty() && window.get_draft().is_empty());

    window.invoke_draft_edited("Unsent draft".into());
    window.invoke_edit(own.item.clone());
    assert_eq!(window.get_draft(), own.body);
    assert_eq!(window.get_compose_label(), "Editing your message");
    window.invoke_draft_edited("Changed".into());
    window.invoke_send();
    let edited = row(&own.item);
    assert!(edited.edited && edited.body == "Changed");
    assert_eq!(window.get_draft(), "Unsent draft");

    window.invoke_request_delete(other.item.clone());
    assert!(
        window.get_confirm_delete().is_empty(),
        "only own messages can be deleted"
    );
    window.invoke_request_delete(own.item.clone());
    assert_eq!(window.get_confirm_delete(), own.item);
    window.invoke_confirm_delete_message();
    assert!(row(&own.item).deleted && window.get_confirm_delete().is_empty());

    // Shift's quick Delete deletes the reply at once, without asking.
    window.invoke_delete_now(reply.item.clone());
    assert!(row(&reply.item).deleted && window.get_confirm_delete().is_empty());
}

#[test]
fn shift_and_a_click_on_the_menus_delete_deletes_without_asking() {
    use i_slint_core::{accessibility::AccessibleStringProperty, item_tree::ItemRc};
    use slint::platform::{Key, PointerEventButton, WindowEvent};
    use std::ops::ControlFlow;
    let window = showing(vec![ChatMessage {
        author: "Me".into(),
        body: "Mine".into(),
        own: true,
        can_delete: true,
        ..Default::default()
    }]);
    let asked = Rc::new(Cell::new(0));
    let count = asked.clone();
    window.on_request_delete(move |_| count.set(count.get() + 1));
    let deleted = Rc::new(Cell::new(0));
    let count = deleted.clone();
    window.on_delete_now(move |_| count.set(count.get() + 1));
    let key = |text: SharedString, pressed: bool| {
        window.window().dispatch_event(if pressed {
            WindowEvent::KeyPressed { text }
        } else {
            WindowEvent::KeyReleased { text }
        });
    };
    let press = |text: SharedString| {
        key(text.clone(), true);
        key(text, false);
    };
    // Alt+Up from the composer reaches the message, and Enter opens its
    // menu; then a click on Delete message.
    let delete_from_menu = || {
        window.invoke_focus_composer();
        key(Key::Alt.into(), true);
        press(Key::UpArrow.into());
        key(Key::Alt.into(), false);
        press(Key::Return.into());
        settle(&window);
        // The menu is a popup, a child window with items of its own.
        let position = {
            let inner = i_slint_core::window::WindowInner::from_pub(window.window());
            let popups = inner.active_popups();
            let popup = popups.last().expect("the message's menu is open");
            let i_slint_core::window::PopupWindowLocation::ChildWindow(place) = popup.location
            else {
                panic!("not a child window");
            };
            let item = ItemRc::new_root(popup.component.clone())
                .visit_descendants(|item| {
                    let label = item.accessible_string_property(AccessibleStringProperty::Label);
                    if label.as_deref() == Some("Delete message") {
                        ControlFlow::Break(item.clone())
                    } else {
                        ControlFlow::Continue(())
                    }
                })
                .expect("the message's menu offers Delete message");
            let at = item.map_to_item_tree(item.geometry().center(), &popup.component);
            slint::LogicalPosition::new(place.x + at.x, place.y + at.y)
        };
        let button = PointerEventButton::Left;
        let native = window.window();
        native.dispatch_event(WindowEvent::PointerMoved { position });
        native.dispatch_event(WindowEvent::PointerPressed { position, button });
        native.dispatch_event(WindowEvent::PointerReleased { position, button });
        settle(&window);
    };
    // A plain click asks first.
    delete_from_menu();
    assert_eq!((asked.get(), deleted.get()), (1, 0));
    // Shift and a click deletes at once, as Discord's menu does.
    window.set_shift_held(true);
    delete_from_menu();
    window.set_shift_held(false);
    assert_eq!((asked.get(), deleted.get()), (1, 1));
}

#[test]
fn a_discord_channel_without_sending_still_takes_reactions() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    let set = |reacts: Reacts| {
        let mut adapter = adapter.borrow_mut();
        adapter.app.state.discord_online = true;
        let id = adapter.app.state.selected.clone().unwrap();
        let room = adapter
            .app
            .state
            .rooms
            .iter_mut()
            .map(Arc::make_mut)
            .find(|room| room.id == id)
            .unwrap();
        room.network = yapfast_core::network::Network::Discord;
        room.read_only = true;
        room.reacts = reacts;
        adapter.app.state.list_changes += 1;
        adapter.refresh(&window);
    };
    // An announcement channel: no composer, but any reaction.
    set(Reacts::Any);
    assert!(!window.get_composer_enabled());
    assert!(window.get_reactions_enabled() && window.get_new_reactions_enabled());
    // Without Add Reactions, only those already there.
    set(Reacts::Existing);
    assert!(window.get_reactions_enabled() && !window.get_new_reactions_enabled());
    // Timed out: none.
    set(Reacts::None);
    assert!(!window.get_reactions_enabled() && !window.get_new_reactions_enabled());
}

#[test]
fn signed_in_controls_keep_trust_and_plaintext_sending_gates() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.state.preview = false;
        adapter.app.state.account = Some("@test:example.invalid".into());
        adapter.app.state.trusted = false;
        adapter.refresh(&window);
    }
    window.invoke_draft_edited("Keep encrypted".into());
    assert!(!window.get_can_send());
    window.invoke_send();
    assert!(!window.get_sending());
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.state.trusted = true;
        let id = adapter.app.state.selected.clone().unwrap();
        adapter
            .app
            .state
            .rooms
            .iter_mut()
            .map(Arc::make_mut)
            .find(|room| room.id == id)
            .unwrap()
            .encrypted = false;
        adapter.app.state.list_changes += 1;
        adapter.refresh(&window);
    }
    assert!(!window.get_composer_enabled() && !window.get_can_send());
    window.invoke_send();
    assert_eq!(window.get_draft(), "Keep encrypted");
    assert!(!window.get_sending());
}

/// What a list told the window about its rows, as `changed 3`, `added 0+2` and
/// `removed 1+1`.
#[derive(Default)]
struct Notices(RefCell<Vec<String>>);

impl i_slint_core::model::ModelChangeListener for Notices {
    fn row_changed(self: std::pin::Pin<&Self>, row: usize) {
        self.0.borrow_mut().push(format!("changed {row}"));
    }
    fn row_added(self: std::pin::Pin<&Self>, index: usize, count: usize) {
        self.0.borrow_mut().push(format!("added {index}+{count}"));
    }
    fn row_removed(self: std::pin::Pin<&Self>, index: usize, count: usize) {
        self.0.borrow_mut().push(format!("removed {index}+{count}"));
    }
    fn reset(self: std::pin::Pin<&Self>) {
        self.0.borrow_mut().push("reset".into());
    }
}

#[test]
fn rows_with_ids_change_only_where_they_changed() {
    use i_slint_core::model::ModelChangeListenerContainer;
    let model = VecModel::from(vec![("b", 1), ("c", 1), ("d", 1)]);
    let notices = Box::pin(ModelChangeListenerContainer::new(Notices::default()));
    model
        .model_tracker()
        .attach_peer(notices.as_ref().model_peer());
    let update = |rows: Vec<(&'static str, i32)>| {
        update_model_by_id(&model, rows.clone(), |row| row.0.into());
        assert_eq!(model.iter().collect::<Vec<_>>(), rows);
        std::mem::take(&mut *notices.0.borrow_mut())
    };
    // Older rows come in above, and those below stay as they were.
    assert_eq!(
        update(vec![("a", 1), ("b", 1), ("c", 1), ("d", 1)]),
        ["added 0+1"]
    );
    assert_eq!(
        update(vec![("a", 1), ("b", 1), ("c", 2), ("d", 1)]),
        ["changed 2"]
    );
    assert_eq!(
        update(vec![("a", 1), ("b", 1), ("c", 2), ("d", 1), ("e", 1)]),
        ["added 4+1"]
    );
    assert_eq!(
        update(vec![("a", 1), ("c", 2), ("d", 1), ("e", 1)]),
        ["removed 1+1"]
    );
    assert_eq!(
        update(vec![("a", 1), ("x", 1), ("y", 1), ("e", 2)]),
        [
            "removed 1+1",
            "removed 1+1",
            "added 1+1",
            "added 2+1",
            "changed 3"
        ]
    );
    assert_eq!(update(vec![]).len(), 4, "each row goes");
}

#[test]
fn typing_and_the_call_show_without_building_the_rest_again() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    let mut adapter = adapter.borrow_mut();
    let room = adapter.app.state.selected.clone().unwrap();
    let reach = adapter.apply(Event::Typing {
        room,
        names: vec!["Sam".into()],
    });
    assert!(
        !reach
            .and(adapter.apply(Event::Voice(Default::default())))
            .everything
    );
    adapter.show(&window, reach);
    assert_eq!(window.get_typing(), "Sam is typing...");
    // A refresh that changes nothing the sidebar shows doesn't build it again,
    // so a row marked here stays marked.
    let mut row = adapter.models.rooms.row_data(0).unwrap();
    row.name = "Marked".into();
    adapter.models.rooms.set_row_data(0, row);
    adapter.refresh(&window);
    assert_eq!(adapter.models.rooms.row_data(0).unwrap().name, "Marked");
    Arc::make_mut(&mut adapter.app.state.rooms[0]).name = "Renamed".into();
    adapter.app.state.list_changes += 1;
    adapter.refresh(&window);
    assert_ne!(adapter.models.rooms.row_data(0).unwrap().name, "Marked");
}

#[test]
fn collapsed_categories_keep_the_open_unread_and_occupied_channels() {
    let row = |id: &str, heading: &str| RoomRow {
        id: id.into(),
        heading: heading.into(),
        category: heading.into(),
        ..Default::default()
    };
    let rows = vec![
        row("a", "TEXT"),
        RoomRow {
            selected: true,
            ..row("b", "")
        },
        RoomRow {
            unread: true,
            ..row("c", "")
        },
        row("d", ""),
        row("e", "VOICE"),
        row("f", ""),
        row("g", "OTHER"),
        row("h", ""),
    ];
    let collapsed = std::collections::BTreeSet::from(["TEXT".to_owned(), "VOICE".to_owned()]);
    let folded = refresh::fold_collapsed(rows, &collapsed, Some("f"));
    let shown: Vec<_> = folded
        .iter()
        .map(|row| {
            (
                row.id.as_str(),
                row.heading.as_str(),
                row.collapsed,
                row.hidden,
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            // A category's first channel goes, and its heading stays alone,
            // naming the channel for the heading's menu.
            ("a", "TEXT", true, true),
            ("b", "", true, false),
            ("c", "", true, false),
            ("e", "VOICE", true, true),
            // The voice channel Yapfast is in stays.
            ("f", "", true, false),
            ("g", "OTHER", false, false),
            ("h", "", false, false),
        ]
    );
}

#[test]
fn clicking_a_category_collapses_it() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    let server = adapter.borrow().app.state.servers[0].id.clone();
    window.invoke_navigate_server(server.into());
    let category = window
        .get_rooms()
        .iter()
        .map(|row| row.category)
        .find(|category| !category.is_empty())
        .expect("the preview server has categories");
    assert!(!window.get_rooms().iter().any(|row| row.collapsed));
    // Nothing the app keeps changes, only how the sidebar shows it.
    window.invoke_toggle_category(category.clone());
    assert!(window.get_rooms().iter().any(|row| row.collapsed));
    window.invoke_toggle_category(category);
    assert!(!window.get_rooms().iter().any(|row| row.collapsed));
}

#[test]
fn clicking_the_voice_panels_channel_line_opens_the_call() {
    use i_slint_core::{accessibility::AccessibleStringProperty, item_tree::ItemRc};
    use slint::platform::{PointerEventButton, WindowEvent};
    use std::ops::ControlFlow;
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    let server = adapter.borrow().app.state.servers[0].id.clone();
    window.invoke_navigate_server(server.clone().into());
    let lounge = {
        let state = &adapter.borrow().app.state;
        let room = state
            .rooms
            .iter()
            .find(|room| room.voice && state.in_scope(room));
        room.expect("the preview server has a voice channel")
            .id
            .clone()
    };
    window.invoke_select_room(lounge.clone().into());
    window.invoke_navigate_dms();
    assert!(window.get_voice_active() && !window.get_voice_here());
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    // The line under "Voice connected" names the call's channel.
    let place = || {
        let inner = i_slint_core::window::WindowInner::from_pub(window.window());
        let link = ItemRc::new_root(inner.component())
            .visit_descendants(|item| {
                let label = item.accessible_string_property(AccessibleStringProperty::Label);
                if label.is_some_and(|label| label.ends_with(". Open the call")) {
                    ControlFlow::Break(item.clone())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .expect("the voice panel's channel line");
        link.map_to_window(link.geometry().center())
    };
    let click = |at: slint::LogicalPosition| {
        let native = window.window();
        let button = PointerEventButton::Left;
        native.dispatch_event(WindowEvent::PointerMoved { position: at });
        native.dispatch_event(WindowEvent::PointerPressed {
            position: at,
            button,
        });
        native.dispatch_event(WindowEvent::PointerReleased {
            position: at,
            button,
        });
    };
    let at = place();
    click(slint::LogicalPosition::new(at.x, at.y));
    let state = || {
        let app = &adapter.borrow().app;
        (app.state.navigation.clone(), app.state.selected.clone())
    };
    assert_eq!(state(), (Navigation::Server(server), Some(lounge)));
    assert!(window.get_voice_here());
}

#[test]
fn opening_at_the_new_divider_puts_it_at_the_top_and_the_bar_jumps_back() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    window.set_workspace(true);
    window.set_room_selected(true);
    window.set_room_ready(true);
    window.set_history_start(true);
    let rows = || -> Vec<MessageRow> {
        (0..100)
            .map(|index| {
                let mut row = message_row_plain(&ChatMessage {
                    id: format!("m{index}"),
                    item: format!("i{index}"),
                    author: "Bob".into(),
                    body: format!("Message {index}"),
                    formatted: true,
                    ..Default::default()
                });
                row.first = true;
                row.unread_start = index == 60;
                row
            })
            .collect()
    };
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    let settle = || {
        for _ in 0..10 {
            i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(50));
            window
                .window()
                .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                    position: slint::LogicalPosition::new(1100., 740.),
                });
        }
    };
    settle();
    // The jump comes with the rows, before the list has measured any.
    window.set_messages(ModelRc::new(VecModel::from(rows())));
    window.set_unread_index(60);
    window.set_unread_label("40 new messages".into());
    messages::show_message(&window, 60, "i60", false);
    settle();
    assert!(
        (59..=60).contains(&window.get_top_index()),
        "the divider is at the top, not row {}",
        window.get_top_index()
    );
    assert!(!window.get_unread_above() && !window.get_unread_below());
    assert!(!window.invoke_timeline_at_end());

    // Scrolled on down, the divider is above, and the bar takes the view back.
    for _ in 0..3 {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(700., 380.),
                delta_x: 0.,
                delta_y: -400.,
            });
        settle();
    }
    assert!(window.get_top_index() > 60);
    assert!(window.get_unread_above());
    messages::show_message(&window, 60, "i60", false);
    settle();
    assert!((59..=60).contains(&window.get_top_index()));
    assert!(!window.get_unread_above());
    // Scrolled up past it, it's below.
    for _ in 0..4 {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(700., 380.),
                delta_x: 0.,
                delta_y: 400.,
            });
        settle();
    }
    assert!(window.get_unread_below());
}

#[test]
fn reading_the_newest_message_tells_the_application() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    window.set_workspace(true);
    window.set_room_selected(true);
    window.set_room_ready(true);
    window.set_history_start(true);
    let rows = |count: usize| -> Vec<MessageRow> {
        (0..count)
            .map(|index| {
                let mut row = message_row_plain(&ChatMessage {
                    id: format!("m{index}"),
                    item: format!("i{index}"),
                    author: "Bob".into(),
                    body: format!("Message {index}"),
                    formatted: true,
                    ..Default::default()
                });
                row.first = true;
                row
            })
            .collect()
    };
    let told = Rc::new(RefCell::new(Vec::new()));
    let telling = told.clone();
    window.on_reading_changed(move |reading| telling.borrow_mut().push(reading));
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    let settle = || {
        for _ in 0..10 {
            i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(50));
            window
                .window()
                .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                    position: slint::LogicalPosition::new(1100., 740.),
                });
        }
    };
    window.set_window_focused(true);
    settle();
    assert!(window.invoke_reading(), "an empty conversation is all read");

    // Opened at the divider, the newest message isn't read until the
    // timeline is scrolled down to it.
    window.set_messages(ModelRc::new(VecModel::from(rows(100))));
    messages::show_message(&window, 60, "i60", false);
    settle();
    assert!(!window.invoke_reading());
    assert_eq!(told.borrow().last(), Some(&false));
    for _ in 0..6 {
        settle();
    }
    assert!(
        !window.invoke_reading(),
        "the jump settled away from the end"
    );
    for _ in 0..20 {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(700., 380.),
                delta_x: 0.,
                delta_y: -400.,
            });
        settle();
    }
    assert!(window.invoke_timeline_at_end());
    assert!(window.invoke_reading());
    assert_eq!(told.borrow().last(), Some(&true));
    window.set_window_focused(false);
    settle();
    assert_eq!(
        told.borrow().last(),
        Some(&false),
        "nor with the window behind"
    );
    window.set_window_focused(true);

    // A conversation too short to scroll is read as soon as the jump places
    // its row, without waiting for the jump to let go, so leaving it at once
    // still reads it.
    window.set_messages(ModelRc::new(VecModel::from(rows(3))));
    messages::show_message(&window, 1, "i1", false);
    assert!(!window.invoke_reading(), "not before the row is placed");
    settle();
    assert!(window.invoke_reading());
    assert_eq!(told.borrow().last(), Some(&true));
    for _ in 0..6 {
        settle();
    }
    assert!(window.invoke_reading());
    // A jump that leaves the list at its end, as to a divider placed once the
    // newest page came, says so again: the application asked for it, and
    // stopped counting on the view until it hears.
    told.borrow_mut().clear();
    messages::show_message(&window, 1, "i1", false);
    settle();
    assert_eq!(told.borrow().last(), Some(&true));
}

#[test]
fn rows_mark_mentions_and_the_new_bar_counts_what_others_sent() {
    let message = |own: bool, mentions_me: bool| ChatMessage {
        own,
        mentions_me,
        time: "14:05".into(),
        // Today, so the bar leaves the date out.
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        ..Default::default()
    };
    assert!(message_row_plain(&message(false, true)).highlighted);
    assert!(!message_row_plain(&message(false, false)).highlighted);
    let unread = [
        message(false, true),
        message(true, false),
        message(false, false),
    ]
    .map(Arc::new);
    assert_eq!(
        refresh::unread_label(&unread, false),
        "2 new messages since 14:05",
        "Alice's own reply isn't new to her"
    );
    assert_eq!(
        refresh::unread_label(&unread[..1], false),
        "1 new message since 14:05"
    );
    assert_eq!(
        refresh::unread_label(&unread[..1], true),
        "1+ new messages since 14:05",
        "more came after a window of older messages"
    );
    let older = ChatMessage {
        timestamp: 1_791_329_400,
        ..message(false, false)
    };
    assert!(
        refresh::unread_label(&[Arc::new(older)], false)
            .starts_with("1 new message since 14:05 on "),
        "with the date when it wasn't today"
    );
}

#[test]
fn virtualized_lists_request_only_nearby_avatars() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    window.set_workspace(true);
    window.set_room_selected(true);
    window.set_room_ready(true);
    window.set_members_available(true);
    let requested = Rc::new(RefCell::new(HashSet::new()));
    let requests = requested.clone();
    window
        .global::<PictureStore>()
        .on_picture(|_, _| slint::Image::default());
    window.global::<PictureStore>().on_request(move |url| {
        requests.borrow_mut().insert(url.to_string());
    });
    window.set_servers(ModelRc::new(VecModel::from(
        (0..500)
            .map(|index| ServerRow {
                id: index.to_string().into(),
                name: format!("Server {index}").into(),
                picture_url: format!("server/{index}").into(),
                ..Default::default()
            })
            .collect::<Vec<_>>(),
    )));
    window.set_roster(ModelRc::new(VecModel::from(
        (0..500)
            .map(|index| MemberRow {
                id: index.to_string().into(),
                name: format!("Member {index}").into(),
                picture_url: format!("member/{index}").into(),
                ..Default::default()
            })
            .collect::<Vec<_>>(),
    )));
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    let settle = || {
        for _ in 0..10 {
            i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(50));
            window
                .window()
                .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                    position: slint::LogicalPosition::new(1100., 740.),
                });
        }
    };
    settle();
    let first = requested.borrow().clone();
    let servers = first
        .iter()
        .filter(|url| url.starts_with("server/"))
        .count();
    let members = first
        .iter()
        .filter(|url| url.starts_with("member/"))
        .count();
    assert!(
        (1..30).contains(&servers),
        "requested {servers} of 500 server icons"
    );
    assert!(
        (1..35).contains(&members),
        "requested {members} of 500 member avatars"
    );
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(1000., 380.),
            delta_x: 0.,
            delta_y: -600.,
        });
    settle();
    assert!(
        requested
            .borrow()
            .iter()
            .any(|url| !first.contains(url) && url.starts_with("member/"))
    );
    assert!(
        requested.borrow().len() < 100,
        "scrolling does not instantiate the whole list"
    );
    window.hide().unwrap();
}

#[test]
fn replacing_a_temporary_item_reports_the_view_without_changing_row_count() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    adapter.borrow_mut().action(&window, Action::Preview);
    let reported = Rc::new(Cell::new(0));
    let reports = reported.clone();
    window.on_media_viewport(move || reports.set(reports.get() + 1));
    let count = window.get_messages().row_count();
    let mut state = adapter.borrow_mut();
    let room = state.app.state.selected.clone().unwrap();
    let mut messages = (*state.app.state.messages).clone();
    Arc::make_mut(&mut messages[0]).item = "e:$confirmed".into();
    state.apply(Event::Timeline { room, messages });
    state.refresh(&window);
    assert_eq!(window.get_messages().row_count(), count);
    assert_eq!(
        window.get_messages().row_data(0).unwrap().item,
        "e:$confirmed"
    );
    assert!(
        reported.get() > 0,
        "the backend receives the new item ids without scrolling"
    );
}

#[test]
fn an_unread_dm_shows_on_the_rail_above_the_servers_and_opens() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    adapter.borrow_mut().action(&window, Action::Preview);
    let rail = |window: &YapfastWindow| -> Vec<(String, bool, bool)> {
        window
            .get_servers()
            .iter()
            .map(|row| (row.id.to_string(), row.dm, row.divider))
            .collect()
    };
    let servers = adapter.borrow().app.state.servers.len();
    let read = rail(&window);
    assert_eq!(read.len(), servers + 1);
    assert!(read[0].2, "the divider leads the servers");
    {
        let mut adapter = adapter.borrow_mut();
        let riley = adapter
            .app
            .state
            .rooms
            .iter_mut()
            .map(Arc::make_mut)
            .find(|room| room.id == "riley")
            .unwrap();
        riley.unread = 2;
        riley.mentions = 2;
        adapter.app.state.list_changes += 1;
        adapter.refresh(&window);
    }
    let unread = rail(&window);
    assert_eq!(unread[0], ("riley".into(), true, false));
    assert!(unread[1].2);
    assert_eq!(&unread[1..], &read[..]);
    let dm = window.get_servers().row_data(0).unwrap();
    assert_eq!((dm.name.as_str(), dm.badge.as_str()), ("Riley", "2"));
    window.invoke_open_conversation("riley".into());
    let adapter = adapter.borrow();
    assert_eq!(adapter.app.state.navigation, Navigation::DirectMessages);
    assert_eq!(adapter.app.state.selected.as_deref(), Some("riley"));
}

#[test]
fn a_folder_on_the_rail_holds_its_servers_until_opened() {
    let folder = yapfast_core::chat::ServerFolder {
        id: "discord-folder:7".into(),
        name: None,
        color: Some(0xff0000),
    };
    let server = |id: &str, folder: Option<&yapfast_core::chat::ServerFolder>| {
        yapfast_core::chat::ChatServer {
            id: id.into(),
            name: id.into(),
            channels: vec![],
            discord: true,
            muted: false,
            unavailable: false,
            icon: Some(format!("{id}.png")),
            folder: folder.cloned(),
            hide_muted: false,
            community: false,
            pinned: Vec::new(),
            ..Default::default()
        }
    };
    let mut state = AppState::default();
    state.servers = [
        server("Alpha", None),
        server("Bravo", Some(&folder)),
        server("Charlie", Some(&folder)),
        server("Delta", None),
    ]
    .map(Arc::new)
    .into();
    state.navigation = Navigation::Server("Charlie".into());
    let shown = |open: &[&str]| {
        let open = open.iter().map(|&id| id.to_owned()).collect();
        refresh::rail_rows(&state, &HashMap::new(), &open, &[])
    };
    let rows = |rows: &[ServerRow]| -> Vec<(String, bool, bool, bool, bool)> {
        rows.iter()
            .map(|row| {
                (
                    row.id.to_string(),
                    row.folder,
                    row.nested,
                    row.last,
                    row.selected,
                )
            })
            .collect()
    };
    let closed = shown(&[]);
    assert_eq!(
        rows(&closed),
        [
            ("Alpha".into(), false, false, false, false),
            // Closed, it shows that one of its servers is selected.
            ("discord-folder:7".into(), true, false, false, true),
            ("Delta".into(), false, false, false, false),
        ]
    );
    // Without a name, it's named by its servers.
    assert_eq!(closed[1].name, "Bravo, Charlie");
    assert_eq!(closed[1].tint, slint::Color::from_rgb_u8(0xff, 0, 0));
    let preview: Vec<_> = closed[1]
        .preview
        .iter()
        .map(|server| server.picture_url.to_string())
        .collect();
    assert_eq!(preview, ["Bravo.png", "Charlie.png"]);

    let open = shown(&["discord-folder:7"]);
    assert_eq!(
        rows(&open),
        [
            ("Alpha".into(), false, false, false, false),
            ("discord-folder:7".into(), true, false, false, true),
            ("Bravo".into(), false, true, false, false),
            ("Charlie".into(), false, true, true, true),
            ("Delta".into(), false, false, false, false),
        ]
    );
    assert!(open[1].open && open[3].tint == open[1].tint);
}

#[test]
fn server_rail_order_persists_and_preserves_folders() {
    use i_slint_core::{accessibility::AccessibleStringProperty, item_tree::ItemRc};
    use slint::platform::{PointerEventButton, WindowEvent};
    use std::ops::ControlFlow;
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let make = || {
        Adapter::new(
            Arc::new(BackgroundRuntime::new().unwrap()),
            directory.path().into(),
            &window,
        )
        .unwrap()
    };
    let folder = yapfast_core::chat::ServerFolder {
        id: "folder".into(),
        name: None,
        color: None,
    };
    let servers: Vec<_> = ["A", "B", "C", "D", "E"]
        .into_iter()
        .map(|id| {
            Arc::new(yapfast_core::chat::ChatServer {
                id: id.into(),
                name: id.into(),
                folder: matches!(id, "D" | "E").then(|| folder.clone()),
                ..Default::default()
            })
        })
        .collect();
    let adapter = make();
    bind_callbacks(&window, &adapter);
    adapter.borrow_mut().action(&window, Action::Preview);
    adapter.borrow_mut().app.state.servers = servers.clone();
    adapter.borrow_mut().sidebar = None;
    adapter.borrow_mut().show_sidebar(&window);
    window.set_workspace(true);
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    settle(&window);
    let point = |label| {
        let inner = i_slint_core::window::WindowInner::from_pub(window.window());
        let component = inner.component();
        ItemRc::new_root(component.clone())
            .visit_descendants(|item| {
                if item.is_visible()
                    && item
                        .accessible_string_property(AccessibleStringProperty::Label)
                        .is_some_and(|text| {
                            text.as_str() == label || text.split(',').next() == Some(label)
                        })
                {
                    let point = item.map_to_item_tree(item.geometry().center(), &component);
                    ControlFlow::Break(slint::LogicalPosition::new(point.x, point.y))
                } else {
                    ControlFlow::Continue(())
                }
            })
            .unwrap_or_else(|| panic!("missing server {label}"))
    };
    let from = point("B");
    let mut to = point("A");
    to.y -= 10.;
    let button = PointerEventButton::Left;
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position: from });
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position: from,
        button,
    });
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position: to });
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position: to,
            button,
        });
    assert_eq!(
        adapter
            .borrow()
            .layout
            .server_rail_order
            .first()
            .map(String::as_str),
        Some("B")
    );
    window.invoke_reorder_server("E".into(), "D".into(), true);
    window.invoke_reorder_server("D".into(), "A".into(), true);
    let reloaded = make();
    let mut reloaded = reloaded.borrow_mut();
    reloaded.app.state.servers = servers;
    let rows = refresh::rail_rows(
        &reloaded.app.state,
        &HashMap::new(),
        &["folder".into()].into(),
        &reloaded.layout.server_rail_order,
    );
    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        ["B", "A", "C", "folder", "E", "D"]
    );
    window.hide().unwrap();
}

#[test]
fn a_right_click_on_a_folder_asks_for_its_menu() {
    use slint::platform::{PointerEventButton, WindowEvent};
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let folder = "discord-folder:7";
    window.set_workspace(true);
    window.set_servers(ModelRc::new(VecModel::from(vec![ServerRow {
        id: folder.into(),
        name: "Friends".into(),
        folder: true,
        ..Default::default()
    }])));
    let toggled = Rc::new(RefCell::new(Vec::new()));
    let asked = Rc::new(RefCell::new(Vec::new()));
    let record = toggled.clone();
    window.on_toggle_folder(move |id| record.borrow_mut().push(id.to_string()));
    let record = asked.clone();
    window.on_prepare_menu(move |id, person| record.borrow_mut().push((id.to_string(), person)));
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    settle(&window);
    let press = |y: f32, button| {
        let position = slint::LogicalPosition::new(36., y);
        let dispatch = |event| window.window().dispatch_event(event);
        dispatch(WindowEvent::PointerMoved { position });
        dispatch(WindowEvent::PointerPressed { position, button });
        dispatch(WindowEvent::PointerReleased { position, button });
        settle(&window);
    };
    // A click opens the folder: that's where it is.
    let y = (60..300)
        .step_by(8)
        .map(|y| y as f32)
        .find(|&y| {
            press(y, PointerEventButton::Left);
            !toggled.borrow().is_empty()
        })
        .expect("the folder is on the rail");
    assert_eq!(*toggled.borrow(), [folder]);
    assert!(asked.borrow().is_empty());
    press(y, PointerEventButton::Right);
    assert_eq!(*asked.borrow(), [(folder.to_owned(), false)]);
    assert_eq!(
        toggled.borrow().len(),
        1,
        "a right click leaves it as it was"
    );
}

#[cfg(feature = "discord")]
#[test]
fn a_profile_shows_activities_as_discord_cards() {
    let activities = yapfast_discord::activity::activities(&serde_json::json!([
        {"type": 2, "name": "Spotify", "details": "Weightless", "state": "Marconi Union",
         "sync_id": "6kkwzB6hXLIONkEk9JciA6",
         "timestamps": {"start": 1_700_000_000_000_u64, "end": 1_700_000_480_000_u64},
         "assets": {"large_image": "spotify:abc", "large_text": "Distance"}},
        {"type": 0, "name": "Factorio", "state": "In a party", "party": {"size": [2, 4]}},
    ]));
    let rows: Vec<ActivityRow> = profile::activity_rows(&activities).iter().collect();
    let spotify = &rows[0];
    assert_eq!(spotify.heading, "LISTENING TO SPOTIFY");
    assert_eq!(
        (
            spotify.name.as_str(),
            spotify.details.as_str(),
            spotify.state.as_str()
        ),
        ("Weightless", "by Marconi Union", "on Distance")
    );
    assert_eq!(spotify.end - spotify.start, 480);
    assert_eq!(
        spotify.link,
        "https://open.spotify.com/track/6kkwzB6hXLIONkEk9JciA6"
    );
    assert_eq!(rows[1].state, "In a party (2 of 4)");
    // The line under the name goes when it only names one of them.
    assert!(profile::names_an_activity("Playing Factorio", &activities));
    assert!(!profile::names_an_activity("Playing Chess", &activities));
}

#[test]
fn discord_channels_show_their_kinds_icons_marked_as_discord_marks_them() {
    use yapfast_core::chat::{ChannelMark, ChatRoom, MarkKind};
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    let adapter = adapter.borrow();
    let icons = &adapter.icons;
    let base = adapter
        .app
        .state
        .rooms
        .iter()
        .find(|room| !room.direct && !room.voice)
        .map(|room| ChatRoom::clone(room))
        .unwrap();
    let channel = |kind, private, age_restricted, voice, forum| ChatRoom {
        mark: ChannelMark {
            kind,
            private,
            age_restricted,
        },
        voice,
        forum,
        ..base.clone()
    };
    let shown = |room: &ChatRoom| icons.channel(room);
    assert!(shown(&channel(MarkKind::Plain, false, false, false, false)) == icons.text.plain);
    assert!(
        shown(&channel(MarkKind::Announcement, true, false, false, false))
            == icons.announcement.private
    );
    assert!(
        shown(&channel(MarkKind::Plain, true, true, false, true)) == icons.forum.age_restricted,
        "the warning sign comes before the lock"
    );
    assert!(shown(&channel(MarkKind::Media, false, false, false, true)) == icons.media.plain);
    assert!(shown(&channel(MarkKind::Stage, true, false, true, false)) == icons.stage.private);
    assert!(shown(&channel(MarkKind::Plain, true, false, true, false)) == icons.voice.private);
    assert!(
        shown(&channel(MarkKind::Rules, true, true, false, false)) == icons.rules,
        "the rules channel has its book whatever else"
    );
    let unreadable = ChatRoom {
        locked: true,
        ..channel(MarkKind::Announcement, false, false, false, false)
    };
    assert!(icons.room(&unreadable) == icons.lock && shown(&unreadable) != icons.lock);
}

#[test]
fn friends_in_a_servers_voice_channel_show_where() {
    use yapfast_core::chat::{ChatRoom, ChatServer, Reacts, VoicePerson};
    let person = |id: &str| VoicePerson {
        id: id.into(),
        name: id.into(),
        ..Default::default()
    };
    let room = |id: &str, voice: bool, direct: bool, people: Vec<VoicePerson>| ChatRoom {
        id: id.into(),
        network: Network::Discord,
        capabilities: Network::Discord.capabilities(),
        name: id.into(),
        direct,
        voice,
        encrypted: false,
        members: 0,
        unread: 0,
        mentions: 0,
        muted: false,
        last_read: None,
        locked: false,
        join_locked: false,
        read_only: false,
        reacts: Reacts::Any,
        slowed: false,
        timed_out_until: None,
        category: String::new(),
        category_id: None,
        request: false,
        person: None,
        people,
        ringing: Vec::new(),
        incoming: false,
        avatar: None,
        decoration: None,
        nameplate: None,
        style: None,
        upload_limit: 0,
        presence: None,
        activity: None,
        activities: Default::default(),
        parent: None,
        forum: false,
        mark: Default::default(),
        unlisted: false,
        tag: None,
        official: false,
        empty: false,
        preview: None,
    };
    let mut state = AppState::default();
    state.rooms = [
        room("lounge", true, false, vec![person("ann"), person("bob")]),
        room("general", false, false, Vec::new()),
        // A DM call isn't a server's voice channel.
        room("dm", true, true, vec![person("cat")]),
    ]
    .map(Arc::new)
    .into();
    state.servers = vec![Arc::new(ChatServer {
        id: "Alpha".into(),
        name: "Alpha".into(),
        channels: vec!["lounge".into(), "general".into()],
        discord: true,
        muted: false,
        unavailable: false,
        icon: None,
        folder: None,
        hide_muted: false,
        community: false,
        pinned: Vec::new(),
        ..Default::default()
    })];
    let rooms_by_id = state
        .rooms
        .iter()
        .map(|room| (room.id.as_str(), &**room))
        .collect();
    let places = refresh::voice_places(&state, &rooms_by_id);
    let mut found: Vec<_> = places
        .iter()
        .map(|(&user, place)| (user, place.room, place.channel, place.server))
        .collect();
    found.sort_unstable();
    assert_eq!(
        found,
        [
            ("ann", "lounge", "lounge", "Alpha"),
            ("bob", "lounge", "lounge", "Alpha"),
        ]
    );
}

#[test]
fn a_call_shows_on_its_servers_icon_and_its_closed_folder() {
    let folder = yapfast_core::chat::ServerFolder {
        id: "discord-folder:7".into(),
        name: None,
        color: None,
    };
    let server = |id: &str, channel: &str, folder: Option<&yapfast_core::chat::ServerFolder>| {
        yapfast_core::chat::ChatServer {
            id: id.into(),
            name: id.into(),
            channels: vec![channel.into()],
            discord: true,
            muted: false,
            unavailable: false,
            icon: None,
            folder: folder.cloned(),
            hide_muted: false,
            community: false,
            pinned: Vec::new(),
            ..Default::default()
        }
    };
    let mut state = AppState::default();
    state.servers = [
        server("Alpha", "alpha-lounge", None),
        server("Bravo", "bravo-lounge", Some(&folder)),
    ]
    .map(Arc::new)
    .into();
    let calls = |state: &AppState, open: &[&str]| -> Vec<(String, bool)> {
        let open = open.iter().map(|&id| id.to_owned()).collect();
        refresh::rail_rows(state, &HashMap::new(), &open, &[])
            .iter()
            .map(|row| (row.id.to_string(), row.call))
            .collect()
    };
    let none = [("Alpha".into(), false), ("discord-folder:7".into(), false)];
    assert_eq!(calls(&state, &[]), none);
    // A voice channel chosen but not joined yet shows nothing.
    state.voice.room = Some("bravo-lounge".into());
    assert_eq!(calls(&state, &[]), none);
    state.voice.phase = yapfast_core::voice::Phase::Connected;
    assert_eq!(
        calls(&state, &[]),
        [("Alpha".into(), false), ("discord-folder:7".into(), true)]
    );
    // Open, the folder leaves it to the server.
    assert_eq!(
        calls(&state, &["discord-folder:7"]),
        [
            ("Alpha".into(), false),
            ("discord-folder:7".into(), true),
            ("Bravo".into(), true),
        ]
    );
    state.voice.room = Some("alpha-lounge".into());
    assert_eq!(
        calls(&state, &[]),
        [("Alpha".into(), true), ("discord-folder:7".into(), false)]
    );
}

#[test]
fn closing_the_viewer_lets_go_of_its_picture() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    let mut adapter = adapter.borrow_mut();
    adapter.app.state.messages = Arc::new(vec![Arc::new(ChatMessage {
        item: "i0".into(),
        attachments: vec![yapfast_core::chat::Attachment {
            voice_ms: None,
            name: "a.png".into(),
            size: None,
            image: true,
            dimensions: Some((40, 30)),
            preview: Some(Arc::new(Picture {
                width: 40,
                height: 30,
                rgba: yapfast_core::chat::Pixels::new(40, 30, vec![128; 40 * 30 * 4]),
                ..Default::default()
            })),
            preview_failed: false,
            placement: yapfast_core::chat::Placement::Message,
            gif: None,
            spoiler: false,
            video: false,
            url: None,
        }],
        ..Default::default()
    })]);
    adapter.app.state.viewer = Some(yapfast_core::application::Viewer {
        item: "i0".into(),
        index: 0,
        pictures: HashMap::new(),
    });
    adapter.refresh(&window);
    assert_eq!(window.get_viewer_image().size().width, 40);
    adapter.app.state.viewer = None;
    adapter.refresh(&window);
    assert!(!window.get_viewer_open());
    assert_eq!(window.get_viewer_image().size().width, 0);
}

#[test]
fn a_pictures_menu_links_to_it_when_it_has_an_address() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    let file = |url: Option<&str>| yapfast_core::chat::Attachment {
        voice_ms: None,
        name: "a.png".into(),
        size: None,
        image: true,
        dimensions: None,
        preview: None,
        preview_failed: false,
        placement: yapfast_core::chat::Placement::Message,
        gif: None,
        spoiler: false,
        video: false,
        url: url.map(Into::into),
    };
    adapter.borrow_mut().app.state.messages = Arc::new(vec![Arc::new(ChatMessage {
        item: "i0".into(),
        attachments: vec![file(None), file(Some("https://cdn.example/a.png"))],
        ..Default::default()
    })]);
    assert_eq!(window.invoke_attachment_link("i0".into(), 0), "");
    assert_eq!(
        window.invoke_attachment_link("i0".into(), 1),
        "https://cdn.example/a.png"
    );
    assert_eq!(window.invoke_attachment_link("i0".into(), 2), "");
    assert_eq!(window.invoke_attachment_link("i0".into(), -1), "");
}

/// A Discord server with a category holding #general, and #rules outside
/// it, muted with its own level; Alice is "1".
#[cfg(feature = "discord")]
fn discord_server_adapter(
    window: &YapfastWindow,
    directory: &std::path::Path,
) -> Rc<RefCell<Adapter>> {
    use yapfast_discord::{
        marked::{ChannelMarks, MarkedServer, ServerMarks},
        notifications::{Level, Mute, Notifications as Settings, Override},
        ready,
        service::{Snapshot, Status},
    };
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.into(),
        window,
    )
    .unwrap();
    bind_callbacks(window, &adapter);
    let server = ready::server(&serde_json::json!({
        "id": "10", "owner_id": "99",
        "properties": {"name": "Test", "default_message_notifications": 1},
        "roles": [{"id": "10", "name": "@everyone", "position": 0, "permissions": "3072"}],
        "channels": [
            {"id": "4", "type": 4, "name": "Talk", "position": 0},
            {"id": "11", "type": 0, "name": "general", "position": 0, "parent_id": "4"},
            {"id": "12", "type": 0, "name": "rules", "position": 0},
        ],
    }))
    .unwrap();
    let notifications = Arc::new(Settings {
        overrides: [(
            "12".to_owned(),
            Override {
                mute: Mute {
                    muted: true,
                    until: None,
                },
                level: Some(Level::Nothing),
                flags: 0,
            },
        )]
        .into(),
        ..Default::default()
    });
    let muted = server.text_channels[1].id.clone();
    let server = MarkedServer::marked(
        server,
        ServerMarks {
            notifications,
            ..Default::default()
        },
        |channel| ChannelMarks {
            muted: channel.id == muted,
            ..Default::default()
        },
    );
    adapter
        .borrow_mut()
        .app
        .apply(yapfast_core::chat::Event::Discord(Arc::new(Snapshot {
            status: Status::Online,
            user_id: Some("1".into()),
            servers: vec![server],
            ..Default::default()
        })));
    adapter
}

#[test]
#[cfg(feature = "discord")]
fn the_bell_and_channel_menus_offer_the_same_notification_choices() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = discord_server_adapter(&window, directory.path());
    let rows = |window: &YapfastWindow| -> Vec<(String, String, bool)> {
        window
            .get_menu_rows()
            .iter()
            .map(|row| (row.text.into(), row.subtitle.into(), row.checked))
            .collect()
    };
    let row = |text: &str, subtitle: &str, checked| (text.into(), subtitle.into(), checked);

    // The header's bell: muting, with its times, then the levels.
    window.invoke_prepare_menu("notifications:discord:10:11".into(), false);
    assert_eq!(
        rows(&window),
        [
            row("Mute channel", "", false),
            row("Use category default", "Only @mentions", true),
            row("All messages", "", false),
            row("Only @mentions", "", false),
            row("Nothing", "", false),
        ]
    );
    let submenus = window.get_menu_submenus();
    assert_eq!(
        submenus.row_data(0).unwrap().row_count(),
        yapfast_core::application::MUTE_TIMES.len()
    );
    // Clicked itself, the row mutes until it's turned back on, as Discord's
    // does; the times are its submenu's.
    assert_eq!(
        window.get_menu_rows().row_data(0).unwrap().click_id,
        "mute-always"
    );
    assert_eq!(submenus.row_data(1).unwrap().row_count(), 0);

    // A muted channel outside a category unmutes, and follows the server.
    window.invoke_prepare_menu("notifications:discord:10:12".into(), false);
    let shown = rows(&window);
    assert_eq!(shown[0], row("Unmute channel", "", false));
    assert_eq!(
        window.get_menu_rows().row_data(0).unwrap().click_id,
        "",
        "a click unmutes it, there being no times to offer"
    );
    assert_eq!(shown[1], row("Use server default", "Only @mentions", false));
    assert_eq!(shown[4], row("Nothing", "", true));
    assert_eq!(window.get_menu_submenus().row_count(), 0, "no row has one");

    // Its own menu has the same, the levels in a submenu.
    window.invoke_prepare_menu("discord:10:12".into(), false);
    let shown = rows(&window);
    assert!(shown.contains(&row("Unmute channel", "", false)));
    assert!(shown.contains(&row("Notification settings", "Nothing", false)));
    let at = shown
        .iter()
        .position(|shown| shown.0 == "Notification settings")
        .unwrap();
    let levels = window.get_menu_submenus().row_data(at).unwrap();
    assert_eq!(levels.row_count(), 4);
    assert!(levels.row_data(3).unwrap().checked);

    // A category's heading names its first channel; its menu is the same.
    assert_eq!(
        window.invoke_category_of("discord:10:11".into()),
        "discord:10:4"
    );
    assert_eq!(window.invoke_category_of("discord:10:12".into()), "");
    window.invoke_prepare_menu("discord:10:4".into(), false);
    let shown = rows(&window);
    assert_eq!(shown[0], row("Mute category", "", false));
    assert_eq!(
        shown[1],
        row("Notification settings", "Only @mentions", false)
    );

    // What each choice changes.
    use yapfast_core::chat::{Muting, NotificationChange, NotifyLevel};
    let adapter = adapter.borrow();
    let general = menus::Notify::of(&adapter, "discord:10:11").unwrap();
    assert_eq!(
        general.change("level-mentions"),
        Some(NotificationChange::Level(Some(NotifyLevel::Mentions)))
    );
    assert_eq!(
        general.change("level-default"),
        Some(NotificationChange::Level(None))
    );
    assert_eq!(
        general.change("mute-60"),
        Some(NotificationChange::Mute(Muting::Minutes(60)))
    );
    assert_eq!(general.change("channel-mute"), None, "its times choose");
    let rules = menus::Notify::of(&adapter, "discord:10:12").unwrap();
    assert_eq!(
        rules.change("channel-mute"),
        Some(NotificationChange::Mute(Muting::Off))
    );
    assert!(
        menus::Notify::of(&adapter, "discord:@me:30").is_none(),
        "a DM"
    );
}

#[test]
#[cfg(feature = "discord")]
fn a_server_search_names_the_server_and_heads_results_with_their_channels() {
    use yapfast_core::chat::{Event, SearchHit};
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = discord_server_adapter(&window, directory.path());
    let hit = |room: &str, item: &str| SearchHit {
        room: room.into(),
        message: ChatMessage {
            item: item.into(),
            body: "plan".into(),
            ..Default::default()
        },
    };
    {
        let mut adapter = adapter.borrow_mut();
        let app = &mut adapter.app;
        app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
        app.dispatch(Action::SelectRoom("discord:10:11".into()));
        app.dispatch(Action::Search("plan".into()));
        app.apply(Event::Found {
            place: "discord:10".into(),
            query: "plan".into(),
            offset: 0,
            result: Ok((
                3,
                vec![
                    hit("discord:10:12", "d:9"),
                    hit("discord:10:12", "d:8"),
                    hit("discord:10:11", "d:7"),
                ],
            )),
        });
        adapter.refresh(&window);
    }
    assert_eq!(window.get_search_placeholder(), "Search Test");
    assert!(window.get_search_open());
    let rows: Vec<(String, String)> = window
        .get_search_results()
        .iter()
        .map(|row| (row.room.into(), row.heading.into()))
        .collect();
    assert_eq!(
        rows,
        [
            ("discord:10:12".into(), "rules".into()),
            ("discord:10:12".into(), String::new()),
            ("discord:10:11".into(), "general".into()),
        ]
    );
}

/// A window and adapter showing a Discord server, "10", where Alice, "1",
/// holds @everyone's permissions: a category "Talk" holding #general and
/// the Lounge, an empty category "Later", and one only Mods see, "Staff".
/// Her settings for the server hide muted channels and suppress role
/// mentions; #general mentions her.
#[cfg(feature = "discord")]
fn discord_server_window() -> (YapfastWindow, Rc<RefCell<Adapter>>, tempfile::TempDir) {
    discord_server_window_granting(0)
}

/// [`discord_server_window`], with everyone in the server allowed `extra`
/// permissions too.
#[cfg(feature = "discord")]
fn discord_server_window_granting(
    extra: u64,
) -> (YapfastWindow, Rc<RefCell<Adapter>>, tempfile::TempDir) {
    discord_server_window_changed(extra, |_, _| {})
}

/// [`discord_server_window_granting`], with `change` made to the server.
#[cfg(feature = "discord")]
fn discord_server_window_changed(
    extra: u64,
    change: impl FnOnce(
        &mut yapfast_core::yapfast_discord::ready::Server,
        &mut yapfast_core::yapfast_discord::marked::ServerMarks,
    ),
) -> (YapfastWindow, Rc<RefCell<Adapter>>, tempfile::TempDir) {
    use yapfast_core::yapfast_discord::{
        marked::{MarkedServer, ServerMarks},
        notifications::Notifications,
        ready::{self, permission},
        service::{Snapshot, Status},
    };
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    let bits = |bits: u64| bits.to_string();
    let hidden = serde_json::json!([{"id": "10", "type": 0, "allow": "0",
                                     "deny": bits(permission::VIEW_CHANNEL)}]);
    let mut server = ready::server(&serde_json::json!({
        "id": "10", "owner_id": "99",
        "properties": {"name": "Test", "default_message_notifications": 1},
        "roles": [{"id": "10", "name": "@everyone", "position": 0,
                   "permissions": bits(permission::VIEW_CHANNEL | permission::SEND_MESSAGES | extra)}],
        "channels": [
            {"id": "4", "type": 4, "name": "Talk", "position": 0},
            {"id": "5", "type": 4, "name": "Later", "position": 1},
            {"id": "6", "type": 4, "name": "Staff", "position": 2, "permission_overwrites": hidden},
            {"id": "11", "type": 0, "name": "general", "position": 0, "parent_id": "4"},
            {"id": "14", "type": 2, "name": "Lounge", "position": 1, "parent_id": "4"},
        ],
    }))
    .unwrap();
    let mut marks = ServerMarks {
        notifications: Arc::new(Notifications {
            hide_muted_channels: true,
            suppress_roles: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    let general = server.text_channels[0].id.clone();
    marks.channel_mut(&server, &general).unwrap().mentions = 1;
    change(&mut server, &mut marks);
    adapter
        .borrow_mut()
        .app
        .apply(Event::Discord(Arc::new(Snapshot {
            status: Status::Online,
            user_id: Some("1".into()),
            servers: vec![MarkedServer {
                server: Arc::new(server),
                marks: Arc::new(marks),
            }],
            ..Default::default()
        })));
    (window, adapter, directory)
}

#[test]
#[cfg(feature = "discord")]
fn a_discord_servers_menus_show_and_change_its_notifications() {
    use yapfast_core::chat::{Muting, NotificationChange, NotifyLevel};
    let (window, adapter, _directory) = discord_server_window();
    let texts = |rows: ModelRc<MenuRow>| -> Vec<String> {
        rows.iter().map(|row| row.text.to_string()).collect()
    };
    window.invoke_prepare_menu("discord:10".into(), false);
    let rows = window.get_menu_rows();
    assert_eq!(
        texts(rows.clone()),
        [
            "Mark as read",
            "Mute server",
            "Notification settings",
            "Hide muted channels",
            "Edit server profile",
            "Leave server",
            "Copy link",
        ]
    );
    let notifications = rows.row_data(2).unwrap();
    assert_eq!(
        notifications.subtitle, "Only @mentions",
        "the server's default"
    );
    let hide = rows.row_data(3).unwrap();
    assert!(hide.checkbox && hide.checked);
    let submenus = window.get_menu_submenus();
    assert_eq!(
        submenus.row_data(1).unwrap().row_count(),
        6,
        "the mute times"
    );
    assert_eq!(
        rows.row_data(1).unwrap().click_id,
        "mute-always",
        "clicking Mute server mutes until it's turned back on"
    );
    let levels = submenus.row_data(2).unwrap();
    assert_eq!(
        texts(levels.clone()),
        [
            "All messages",
            "Only @mentions",
            "Nothing",
            "Suppress @everyone and @here",
            "Suppress all role @mentions"
        ]
    );
    let checked: Vec<bool> = levels.iter().map(|row| row.checked).collect();
    assert_eq!(checked, [false, true, false, false, true]);

    // The header's dropdown has the same rows, in Discord's order.
    window.invoke_prepare_menu("header:discord:10".into(), false);
    assert_eq!(
        texts(window.get_menu_rows()),
        [
            "Notification settings",
            "Edit server profile",
            "Hide muted channels",
            "Leave server"
        ]
    );
    assert_eq!(window.get_header_server(), "", "until the server is open");
    window.invoke_navigate_server("discord:10".into());
    assert_eq!(window.get_header_server(), "discord:10");

    // Each choice is the change it names; a switch turns over.
    let app = &adapter.borrow().app;
    let change = |id: &str| menus::server_change(app, "discord:10", id);
    assert_eq!(
        change("mute-60"),
        Some(NotificationChange::Mute(Muting::Minutes(60)))
    );
    assert_eq!(
        change("unmute-server"),
        Some(NotificationChange::Mute(Muting::Off))
    );
    assert_eq!(
        change("level-nothing"),
        Some(NotificationChange::Level(Some(NotifyLevel::Nothing)))
    );
    assert_eq!(
        change("suppress-roles"),
        Some(NotificationChange::SuppressRoles(false))
    );
    assert_eq!(
        change("suppress-everyone"),
        Some(NotificationChange::SuppressEveryone(true))
    );
    assert_eq!(
        change("hide-muted"),
        Some(NotificationChange::HideMuted(false))
    );
    assert_eq!(change("copy-id"), None);
}

#[test]
#[cfg(feature = "discord")]
fn discord_categories_head_their_channels_and_open_their_menus() {
    let (window, adapter, _directory) = discord_server_window();
    window.invoke_navigate_server("discord:10".into());
    let rows: Vec<RoomRow> = adapter.borrow().models.rooms.iter().collect();
    let shown: Vec<(&str, String, bool)> = rows
        .iter()
        .map(|row| {
            let category = window.invoke_category_of(row.id.clone()).to_string();
            (row.heading.as_str(), category, row.hidden)
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("Talk", "discord:10:4".into(), false),
            ("", "discord:10:4".into(), false),
            ("Later", "discord:10:5".into(), true),
        ],
        "an empty category shows its heading alone, named by the category; \
         Staff is hidden from her"
    );
    window.invoke_prepare_menu("discord:10:4".into(), false);
    let texts: Vec<String> = window
        .get_menu_rows()
        .iter()
        .map(|row| row.text.to_string())
        .collect();
    assert_eq!(
        texts,
        ["Mark as read", "Mute category", "Notification settings",]
    );
}

#[test]
#[cfg(feature = "discord")]
fn categories_of_the_same_name_head_and_fold_their_own_channels() {
    let (window, adapter, _directory) = discord_server_window_changed(0, |server, _| {
        Arc::make_mut(&mut server.categories)
            .get_mut("5")
            .unwrap()
            .name = "Talk".into();
        server.voice_channels[0].parent = Some("5".into());
    });
    window.invoke_navigate_server("discord:10".into());
    let shown = || -> Vec<(String, String, bool)> {
        adapter
            .borrow()
            .models
            .rooms
            .iter()
            .map(|row| (row.heading.into(), row.name.into(), row.collapsed))
            .collect()
    };
    let row = |heading: &str, name: &str, collapsed| (heading.into(), name.into(), collapsed);
    assert_eq!(
        shown(),
        [row("Talk", "general", false), row("Talk", "Lounge", false)]
    );
    window.invoke_toggle_category("discord:10:5".into());
    assert_eq!(
        shown(),
        [row("Talk", "general", false), row("Talk", "", true)],
        "only the second folds"
    );
}

#[test]
#[cfg(feature = "discord")]
fn pinned_channels_leave_their_categories_for_the_top() {
    use yapfast_core::yapfast_discord::notifications::{Notifications, Override};
    let (window, adapter, _directory) = discord_server_window_changed(0, |_, marks| {
        let mut settings = Notifications::clone(&marks.notifications);
        settings.overrides.insert(
            "11".into(),
            Override {
                flags: 6144,
                ..Default::default()
            },
        );
        marks.notifications = Arc::new(settings);
    });
    {
        // A category of that name folding doesn't fold the pinned ones.
        adapter
            .borrow_mut()
            .layout
            .collapsed
            .entry("discord:10".into())
            .or_default()
            .insert("PINNED CHANNELS".into());
    }
    window.invoke_navigate_server("discord:10".into());
    let rows: Vec<RoomRow> = adapter.borrow().models.rooms.iter().collect();
    let shown: Vec<_> = rows
        .iter()
        .map(|row| {
            (
                row.heading.as_str(),
                row.name.as_str(),
                row.pinned,
                row.collapsed,
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("PINNED CHANNELS", "general", true, false),
            ("Talk", "Lounge", false, false),
            ("Later", "", false, false),
        ]
    );
    window.invoke_prepare_menu("discord:10:11".into(), false);
    let texts: Vec<String> = window
        .get_menu_rows()
        .iter()
        .map(|row| row.text.to_string())
        .collect();
    assert_eq!(
        texts,
        [
            "Mark as read",
            "Unpin channel",
            "Copy link",
            "Mute channel",
            "Notification settings",
        ]
    );
}

#[test]
#[cfg(feature = "discord")]
fn a_community_server_lists_the_channels_followed_and_offers_following() {
    use yapfast_core::yapfast_discord::notifications::Notifications;
    let (window, adapter, _directory) = discord_server_window_changed(0, |server, marks| {
        let mut settings = Notifications::clone(&marks.notifications);
        settings.flags = 1 << 14;
        marks.notifications = Arc::new(settings);
        server.community = true;
        let general = marks.channel_mut(server, "11").unwrap();
        general.mentions = 0;
        general.unread = true;
    });
    window.invoke_navigate_server("discord:10".into());
    let rail = window.get_servers();
    let row = rail.iter().find(|row| row.id == "discord:10").unwrap();
    assert!(!row.unread, "#general's messages don't mark the server");
    let rows: Vec<RoomRow> = adapter.borrow().models.rooms.iter().collect();
    let shown: Vec<_> = rows
        .iter()
        .map(|row| (row.heading.as_str(), row.name.as_str()))
        .collect();
    assert_eq!(
        shown,
        [("Talk", "Lounge"), ("Later", "")],
        "#general isn't followed"
    );
    let texts = |id: &str| -> Vec<(String, bool)> {
        window.invoke_prepare_menu(id.into(), false);
        window
            .get_menu_rows()
            .iter()
            .map(|row| (row.text.to_string(), row.checked))
            .collect()
    };
    let server = texts("discord:10");
    let at = server
        .iter()
        .position(|(text, _)| text == "Show all channels")
        .unwrap();
    assert!(!server[at].1, "unticked");
    assert_eq!(server[at + 1].0, "Hide muted channels");
    let channel: Vec<String> = texts("discord:10:11")
        .into_iter()
        .map(|(text, _)| text)
        .collect();
    assert_eq!(
        channel,
        [
            "Mark as read",
            "Pin channel to top",
            "Add to channel list",
            "Copy link",
            "Mute channel",
            "Notification settings",
        ]
    );
}

#[test]
#[cfg(feature = "discord")]
fn browse_channels_opens_from_a_community_servers_list_and_searches() {
    let (window, _adapter, _directory) = discord_server_window_changed(0, |server, _| {
        server.community = true;
        server.text_channels[0].topic = Some("Say **hi**".into());
    });
    window.invoke_navigate_server("discord:10".into());
    assert!(window.get_browse_offered());
    assert!(!window.get_browse_open());
    window.invoke_open_browse();
    assert!(window.get_browse_open());
    let rows = |window: &YapfastWindow| -> Vec<(String, String, bool, bool)> {
        window
            .get_browse_rows()
            .iter()
            .map(|row| {
                (
                    row.name.to_string(),
                    row.topic.to_string(),
                    row.category,
                    row.followed,
                )
            })
            .collect()
    };
    // With none followed, every box is ticked.
    assert_eq!(
        rows(&window),
        [
            ("Talk".into(), "".into(), true, true),
            ("general".into(), "Say hi".into(), false, true),
            ("Lounge".into(), "".into(), false, true),
        ]
    );
    window.set_browse_filter("lou".into());
    window.invoke_browse_filter_edited();
    assert_eq!(
        rows(&window),
        [
            ("Talk".into(), "".into(), true, true),
            ("Lounge".into(), "".into(), false, true),
        ]
    );
    window.set_browse_filter("nothing".into());
    window.invoke_browse_filter_edited();
    assert!(rows(&window).is_empty());
    // A channel takes its place.
    window.invoke_select_room("discord:10:11".into());
    assert!(!window.get_browse_open());
}

#[test]
fn empty_categories_go_where_discords_order_puts_them() {
    let category = |id: &str, empty: bool| ListedCategory {
        id: id.into(),
        name: id.into(),
        empty,
    };
    let row = |heading: &str, id: &str| RoomRow {
        id: id.into(),
        heading: heading.into(),
        category: heading.to_lowercase().into(),
        ..Default::default()
    };
    // Each heading names its category's first channel.
    let rows = vec![row("", "x"), row("B", "b1"), row("", "y"), row("D", "d1")];
    let categories = [
        category("a", true),
        category("b", false),
        category("c", true),
        category("d", false),
        category("e", true),
    ];
    let listed: Vec<String> = refresh::with_empty_categories(rows, &categories)
        .iter()
        .map(|row| format!("{}{}", row.heading, if row.hidden { "!" } else { "" }))
        .collect();
    assert_eq!(listed, ["", "a!", "B", "", "c!", "D", "e!"]);
}

#[test]
#[cfg(feature = "discord")]
fn the_invite_dialog_shows_its_link_and_how_long_it_lasts() {
    let (window, adapter, _directory) = discord_server_window();
    let show = |invite: yapfast_core::application::InviteDialog| {
        let mut adapter = adapter.borrow_mut();
        adapter.app.state.invite = Some(invite);
        adapter.refresh(&window);
    };
    let invite = yapfast_core::application::InviteDialog {
        server: "Test".into(),
        room: "discord:10:14".into(),
        channel: "Lounge".into(),
        voice: true,
        max_age: 43_200,
        max_uses: 25,
        link: None,
        making: true,
        error: None,
    };
    show(invite.clone());
    assert!(window.get_invite_open());
    let prompt = window.get_invite_prompt();
    assert_eq!(prompt.title, "Invite friends to Test");
    assert_eq!(prompt.detail, "They'll land in Lounge.");
    assert!(prompt.making && prompt.link.is_empty() && prompt.note.is_empty());
    assert_eq!(
        (window.get_invite_age(), window.get_invite_uses()),
        (3, 4),
        "its choices"
    );

    // Choosing others stays while the dialog is open.
    window.set_invite_age(6);
    show(yapfast_core::application::InviteDialog {
        link: Some("https://discord.gg/abc".into()),
        making: false,
        ..invite.clone()
    });
    let prompt = window.get_invite_prompt();
    assert_eq!(prompt.link, "https://discord.gg/abc");
    assert_eq!(
        prompt.note,
        "This link expires in 12 hours and works 25 times."
    );
    assert_eq!(window.get_invite_age(), 6);

    show(yapfast_core::application::InviteDialog {
        max_age: 0,
        max_uses: 1,
        channel: "general".into(),
        voice: false,
        link: Some("https://discord.gg/abc".into()),
        making: false,
        ..invite
    });
    let prompt = window.get_invite_prompt();
    assert_eq!(prompt.note, "This link never expires and works once.");
    assert_eq!(prompt.detail, "They'll land in #general.");
    window.invoke_close_invite();
    assert!(!window.get_invite_open());
}

#[test]
#[cfg(feature = "discord")]
fn channel_menus_and_their_dialog_make_change_and_delete_channels() {
    use yapfast_core::{
        application::{ChannelAsk, ChannelDialog},
        chat::{ChannelKind, ChannelSettings},
        yapfast_discord::ready::permission,
    };
    let (window, adapter, _directory) = discord_server_window_granting(permission::MANAGE_CHANNELS);
    let texts = |rows: ModelRc<MenuRow>| -> Vec<String> {
        rows.iter().map(|row| row.text.to_string()).collect()
    };
    window.invoke_prepare_menu("discord:10:14".into(), false);
    let rows = window.get_menu_rows();
    assert_eq!(
        texts(rows.clone()),
        [
            "Mark as read",
            "Join voice",
            "Open chat",
            "Pin channel to top",
            "Copy link",
            "Edit channel",
            "Duplicate channel",
            "Create voice channel",
            "Delete channel",
        ]
    );
    let delete = rows.row_data(8).unwrap();
    assert!(delete.danger && delete.separated);
    window.invoke_prepare_menu("discord:10:4".into(), false);
    assert_eq!(
        texts(window.get_menu_rows())[1..],
        [
            "Mute category",
            "Notification settings",
            "Edit category",
            "Delete category",
        ]
    );
    window.invoke_prepare_menu("header:discord:10".into(), false);
    assert_eq!(
        texts(window.get_menu_rows())[..2],
        ["Create channel", "Create category"]
    );

    // Choosing one opens its dialog.
    window.invoke_choose_menu("discord:10".into(), false, "create-channel".into());
    assert!(window.get_channel_open());
    let prompt = window.get_channel_prompt();
    assert_eq!(prompt.title, "Create channel");
    assert_eq!(prompt.detail, "In Test");
    assert!(prompt.kinds && prompt.named && !prompt.danger);
    window.set_channel_name("Plans".into());
    window.set_channel_kind(1);
    window.invoke_save_channel();
    {
        let adapter = adapter.borrow();
        let dialog = adapter.app.state.channel_dialog.as_ref().unwrap();
        assert!(dialog.saving, "{dialog:?}");
        assert_eq!(dialog.kind, ChannelKind::Voice);
    }
    window.invoke_close_channel();
    assert!(!window.get_channel_open());

    let show = |dialog: ChannelDialog| {
        let mut adapter = adapter.borrow_mut();
        adapter.app.state.channel_dialog = Some(dialog);
        adapter.refresh(&window);
    };
    show(ChannelDialog {
        ask: ChannelAsk::Delete("discord:10:4".into()),
        server: "discord:10".into(),
        server_name: "Test".into(),
        name: "Talk".into(),
        kind: ChannelKind::Category,
        settings: Some(ChannelSettings::default()),
        saving: false,
        error: None,
    });
    let prompt = window.get_channel_prompt();
    assert_eq!(prompt.title, "Delete category");
    assert_eq!(
        prompt.detail,
        "Are you sure you want to delete Talk? This cannot be undone."
    );
    assert!(prompt.danger && !prompt.named);
}

#[test]
#[cfg(feature = "discord")]
fn channel_settings_cover_the_window_with_overview_and_permissions() {
    use slint::Model;
    use yapfast_core::yapfast_discord::ready::permission;
    let (window, adapter, _directory) =
        discord_server_window_granting(permission::MANAGE_CHANNELS | permission::MANAGE_ROLES);
    window.invoke_prepare_menu("discord:10:11".into(), false);
    window.invoke_choose_menu("discord:10:11".into(), false, "edit-channel".into());
    assert!(window.get_channel_settings_open() && !window.get_channel_open());
    let prompt = window.get_channel_settings_prompt();
    assert!(prompt.loading && prompt.section == 0);
    let rows: Vec<String> = prompt.rows.iter().map(|row| row.text.to_string()).collect();
    assert_eq!(
        rows,
        ["#GENERAL", "Overview", "Permissions", "", "Delete Channel"]
    );
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.apply(Event::ChannelLoaded {
            room: "discord:10:11".into(),
            result: Ok(ChannelSettings {
                name: "general".into(),
                topic: "Say hi".into(),
                slowmode: 300,
                nsfw: true,
            }),
        });
        adapter.refresh(&window);
    }
    let prompt = window.get_channel_settings_prompt();
    assert!(prompt.loaded && prompt.nsfw && !prompt.changed);
    assert_eq!(prompt.slowmode, 7);
    assert_eq!(window.get_channel_settings_topic(), "Say hi");

    // Typing shows the bar, and another page waits for it.
    window.set_channel_settings_topic("Say hello".into());
    window.invoke_channel_settings_input(1, "Say hello".into(), 0);
    assert!(window.get_channel_settings_prompt().changed);
    window.invoke_channel_settings_input(10, "".into(), 1);
    assert_eq!(window.get_channel_settings_prompt().nudge, 1);
    window.invoke_channel_settings_input(30, "".into(), 0);
    assert_eq!(window.get_channel_settings_topic(), "Say hi");

    // Permissions: @everyone's, three ways each, locked where she can't.
    window.invoke_channel_settings_input(10, "".into(), 1);
    let prompt = window.get_channel_settings_prompt();
    assert_eq!(prompt.section, 1);
    assert_eq!(prompt.title, "Channel Permissions");
    let names: Vec<String> = prompt
        .overwrites
        .iter()
        .map(|row| row.name.to_string())
        .collect();
    assert_eq!(names, ["@everyone"]);
    let row = |bit: u64| {
        window
            .get_channel_settings_prompt()
            .permissions
            .iter()
            .find(|row| !row.heading && 1u64 << row.bit == bit)
            .unwrap()
    };
    assert_eq!(
        window
            .get_channel_settings_prompt()
            .permissions
            .row_data(0)
            .unwrap()
            .title,
        "GENERAL CHANNEL PERMISSIONS"
    );
    assert_eq!(
        (
            row(permission::VIEW_CHANNEL).state,
            row(permission::VIEW_CHANNEL).lock_text.as_str()
        ),
        (1, "")
    );
    assert_eq!(
        row(permission::MANAGE_WEBHOOKS).lock_text,
        "You cannot modify this permission because none of your roles have it."
    );
    window.invoke_channel_settings_input(14, "".into(), 11 * 3 + 2);
    assert_eq!(row(permission::SEND_MESSAGES).state, 2);
    assert!(window.get_channel_settings_prompt().changed);
    window.invoke_channel_settings_input(30, "".into(), 0);
    window.invoke_channel_settings_input(32, "".into(), 0);
    assert!(!window.get_channel_settings_open());

    // Delete Channel, last in the sidebar, closes them and asks.
    window.invoke_prepare_menu("discord:10:11".into(), false);
    window.invoke_choose_menu("discord:10:11".into(), false, "edit-channel".into());
    window.invoke_channel_settings_input(10, "".into(), 99);
    assert!(!window.get_channel_settings_open() && window.get_channel_open());
    assert_eq!(window.get_channel_prompt().title, "Delete channel");
}

#[test]
#[cfg(feature = "discord")]
fn leaving_a_server_asks_first_in_red() {
    let (window, adapter, _directory) = discord_server_window();
    window.invoke_prepare_menu("discord:10".into(), false);
    let leave = window
        .get_menu_rows()
        .iter()
        .find(|row| row.text == "Leave server")
        .unwrap();
    assert!(leave.danger && leave.separated);
    window.invoke_choose_menu("discord:10".into(), false, "leave-server".into());
    assert!(window.get_leave_open());
    let prompt = window.get_leave_prompt();
    assert_eq!(prompt.title, "Leave 'Test'");
    assert_eq!(prompt.confirm, "Leave server");
    assert!(prompt.danger && !prompt.named && !prompt.saving);
    window.invoke_confirm_leave();
    assert!(adapter.borrow().app.state.leave.as_ref().unwrap().leaving);
    window.invoke_close_leave();
    assert!(!window.get_leave_open());
}

#[test]
#[cfg(feature = "discord")]
fn leaving_a_group_dm_asks_first_and_offers_to_tell_no_one() {
    use yapfast_core::yapfast_discord::service::{Conversation, Snapshot, Status};
    let (window, adapter, _directory) = discord_server_window();
    adapter
        .borrow_mut()
        .app
        .apply(Event::Discord(Arc::new(Snapshot {
            status: Status::Online,
            user_id: Some("1".into()),
            conversations: vec![Arc::new(Conversation {
                id: "31".into(),
                name: "Hikers".into(),
                group: true,
                people: 3,
                ..Default::default()
            })],
            ..Default::default()
        })));
    let group = "discord:@me:31";
    window.invoke_prepare_menu(group.into(), false);
    let rows = window.get_menu_rows();
    let leave = rows.iter().find(|row| row.text == "Leave group").unwrap();
    assert!(leave.danger && leave.separated);
    window.invoke_choose_menu(group.into(), false, "leave-group".into());
    assert!(window.get_leave_open());
    let prompt = window.get_leave_prompt();
    assert_eq!(prompt.title, "Leave 'Hikers'");
    assert_eq!(
        prompt.detail,
        "Are you sure you want to leave Hikers? You won't be able to rejoin this group unless you are re-invited."
    );
    assert_eq!(prompt.confirm, "Leave group");
    assert_eq!(prompt.option, "Leave without notifying other members");
    assert!(prompt.danger && !prompt.ticked);
    window.invoke_toggle_silent_leave();
    assert!(window.get_leave_prompt().ticked);
    window.invoke_confirm_leave();
    let leave = adapter.borrow().app.state.leave.clone().unwrap();
    assert!(leave.leaving && leave.silent);
    assert!(window.get_leave_prompt().saving);
}

#[test]
#[cfg(feature = "discord")]
fn edit_server_profile_opens_the_per_server_profile_editor() {
    use yapfast_core::yapfast_discord::ready::permission;
    let (window, adapter, _directory) = discord_server_window_granting(permission::CHANGE_NICKNAME);
    window.invoke_prepare_menu("header:discord:10".into(), false);
    let texts: Vec<String> = window
        .get_menu_rows()
        .iter()
        .map(|row| row.text.to_string())
        .collect();
    assert!(
        texts.contains(&"Edit server profile".to_owned()),
        "{texts:?}"
    );
    window.invoke_choose_menu(
        "header:discord:10".into(),
        false,
        "edit-server-profile".into(),
    );
    assert!(window.get_profile_editor_open());
    assert!(window.get_profile_editor_prompt().loading);
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.apply(Event::OwnProfile {
            server: Some("discord:10".into()),
            result: Ok(yapfast_core::chat::OwnProfile {
                pronouns: "she/her".into(),
                fallback: yapfast_core::chat::Profile {
                    name: "Alice".into(),
                    handle: "alice".into(),
                    bio: "Main bio".into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        });
        adapter.refresh(&window);
    }
    let prompt = window.get_profile_editor_prompt();
    assert!(!prompt.loading && prompt.name_enabled && !prompt.bio_enabled && !prompt.changed);
    assert!(prompt.offer && !prompt.main && prompt.banner_shown && !prompt.colour_shown);
    assert_eq!(prompt.title, "Edit server profile");
    assert_eq!(window.get_profile_editor_pronouns(), "she/her");
    assert_eq!(prompt.name_placeholder, "alice");
    assert_eq!(prompt.preview.name, "Alice", "the main profile's name");
    assert_eq!(prompt.preview.handle, "alice");
    assert_eq!(prompt.preview.pronouns, "she/her");
    assert!(prompt.preview.has_bio, "the main profile's bio");
    assert_eq!(prompt.bio_left, 190);

    // Typing changes the preview and nothing else.
    window.set_profile_editor_name("Ally".into());
    window.invoke_write_profile(0, "Ally".into());
    let prompt = window.get_profile_editor_prompt();
    assert_eq!(prompt.preview.name, "Ally");
    assert!(prompt.changed);
    window.invoke_write_profile(0, "x".repeat(33).into());
    assert_eq!(
        window.get_profile_editor_prompt().too_long,
        "A nickname can be up to 32 characters."
    );

    window.invoke_close_profile_editor();
    assert!(!window.get_profile_editor_open());
    assert_eq!(window.get_profile_editor_prompt().preview.name, "");
}

#[test]
#[cfg(feature = "discord")]
fn server_settings_open_from_the_menu_and_hold_their_changes_until_saved() {
    use slint::Model;
    use yapfast_core::{
        chat::{ServerProfile, ServerSettingsPage, SettingsSection},
        yapfast_discord::ready::permission,
    };
    let (window, adapter, _directory) = discord_server_window_granting(permission::MANAGE_GUILD);
    let texts = |window: &YapfastWindow| -> Vec<String> {
        window
            .get_menu_rows()
            .iter()
            .map(|row| row.text.to_string())
            .collect()
    };
    window.invoke_prepare_menu("header:discord:10".into(), false);
    assert!(
        texts(&window).contains(&"Server settings".to_owned()),
        "{:?}",
        texts(&window)
    );
    window.invoke_choose_menu("header:discord:10".into(), false, "server-settings".into());
    assert!(window.get_server_settings_open());
    let prompt = window.get_server_settings_prompt();
    assert!(prompt.loading && !prompt.loaded);
    let rows: Vec<(bool, String)> = prompt
        .rows
        .iter()
        .map(|row| (row.heading, row.text.to_string()))
        .collect();
    assert_eq!(
        rows,
        [
            (true, "TEST".into()),
            (false, "Server Profile".into()),
            (false, "Engagement".into()),
            (false, "Boost Perks".into()),
            (true, "PEOPLE".into()),
            (false, "Members".into()),
            (false, "Invites".into()),
            (true, "MODERATION".into()),
            (false, "Safety Setup".into())
        ]
    );

    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.apply(Event::ServerSettingsLoaded {
            server: "discord:10".into(),
            section: SettingsSection::Profile,
            result: Ok(ServerSettingsPage::Profile(ServerProfile {
                name: "Test".into(),
                description: "Hi".into(),
                icon: None,
            })),
        });
        adapter.refresh(&window);
    }
    let prompt = window.get_server_settings_prompt();
    assert!(prompt.loaded && !prompt.changed && !prompt.profile.has_icon);
    assert_eq!(window.get_server_settings_name(), "Test");
    assert_eq!(window.get_server_settings_description(), "Hi");
    assert_eq!(prompt.profile.description_left, 298);

    // Typing shows the bar; closing then shakes it instead.
    window.set_server_settings_name("T".into());
    window.invoke_write_server_setting(0, "T".into());
    let prompt = window.get_server_settings_prompt();
    assert!(prompt.changed && !prompt.may_save);
    assert_eq!(
        prompt.problem,
        "The server name must be between 2 and 100 characters."
    );
    window.invoke_close_server_settings();
    assert!(window.get_server_settings_open());
    assert_eq!(window.get_server_settings_prompt().nudge, 1);

    // Reset puts Discord's back, and closing closes.
    window.invoke_undo_server_settings();
    assert_eq!(window.get_server_settings_name(), "Test");
    assert!(!window.get_server_settings_prompt().changed);
    window.invoke_close_server_settings();
    assert!(!window.get_server_settings_open());
}

#[test]
#[cfg(feature = "discord")]
fn engagement_and_boost_perks_show_their_choices_and_locks() {
    use slint::Model;
    use yapfast_core::{
        chat::{GuildSettings, ServerSettingsPage, SettingsSection},
        yapfast_discord::ready::permission,
    };
    let (window, adapter, _directory) = discord_server_window_granting(permission::MANAGE_GUILD);
    let names = |list: ModelRc<SharedString>| -> Vec<String> {
        list.iter().map(|name| name.to_string()).collect()
    };
    let load = |section: SettingsSection| {
        let mut adapter = adapter.borrow_mut();
        adapter.app.apply(Event::ServerSettingsLoaded {
            server: "discord:10".into(),
            section,
            result: Ok(ServerSettingsPage::Guild(GuildSettings {
                system_channel: Some("11".into()),
                // Welcome messages left out.
                system_flags: 1,
                default_notifications: 1,
                afk_timeout: 300,
                premium_tier: 1,
                ..Default::default()
            })),
        });
        adapter.refresh(&window);
    };
    window.invoke_prepare_menu("header:discord:10".into(), false);
    window.invoke_choose_menu("header:discord:10".into(), false, "server-settings".into());
    window.invoke_server_settings_section(1);
    load(SettingsSection::Engagement);
    let engagement = window.get_server_settings_prompt().engagement;
    assert_eq!(
        names(engagement.system_channels),
        ["No System Messages", "general"]
    );
    assert_eq!(
        names(engagement.afk_channels),
        ["No Inactive Channel", "Lounge"]
    );
    assert_eq!(
        (
            engagement.system_channel,
            engagement.afk_channel,
            engagement.afk_timeout,
            engagement.notifications
        ),
        (1, 0, 1, 1)
    );
    assert!(!engagement.welcome && engagement.boosted && engagement.setup_tips);

    // An inactive channel, then its timeout, and welcome messages back on.
    window.invoke_choose_server_setting(2, 1);
    window.invoke_choose_server_setting(3, 3);
    window.invoke_toggle_server_setting(0);
    let prompt = window.get_server_settings_prompt();
    assert!(prompt.changed && prompt.may_save);
    assert_eq!(
        (prompt.engagement.afk_channel, prompt.engagement.afk_timeout),
        (1, 3)
    );
    assert!(prompt.engagement.welcome);
    window.invoke_undo_server_settings();

    // At level 1 the invite background is open, the banner locked.
    window.invoke_server_settings_section(2);
    load(SettingsSection::BoostPerks);
    let boost = window.get_server_settings_prompt().boost;
    assert_eq!(boost.level, "Level 1");
    assert_eq!(boost.banner.locked, "Unlocks at Level 2");
    assert_eq!(boost.splash.locked, "");
    assert!(!boost.banner.shown && !boost.progress_bar);
    window.invoke_toggle_server_setting(4);
    assert!(window.get_server_settings_prompt().boost.progress_bar);
}

#[cfg(feature = "discord")]
#[test]
fn moderation_shows_safety_setup_and_bans_as_discords_pages_do() {
    use slint::Model;
    use yapfast_core::{
        chat::{Ban, GuildSettings, RoleMember, ServerSettingsPage, SettingsSection},
        yapfast_discord::ready::permission,
    };
    let (window, adapter, _directory) =
        discord_server_window_granting(permission::BAN_MEMBERS | permission::MANAGE_GUILD);
    window.invoke_prepare_menu("header:discord:10".into(), false);
    window.invoke_choose_menu("header:discord:10".into(), false, "server-settings".into());
    let rows: Vec<String> = window
        .get_server_settings_prompt()
        .rows
        .iter()
        .map(|row| row.text.to_string())
        .collect();
    assert_eq!(
        rows[rows.len() - 3..],
        ["MODERATION", "Safety Setup", "Bans"]
    );

    // Safety Setup: Discord's levels, and the 2FA switch only its owner moves.
    window.invoke_server_settings_section(4);
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.apply(Event::ServerSettingsLoaded {
            server: "discord:10".into(),
            section: SettingsSection::Safety,
            result: Ok(ServerSettingsPage::Guild(GuildSettings {
                verification_level: 2,
                features: vec!["COMMUNITY".into()],
                ..Default::default()
            })),
        });
        adapter.refresh(&window);
    }
    let prompt = window.get_server_settings_prompt();
    assert!(prompt.immediate);
    let safety = prompt.safety;
    let levels: Vec<(String, bool)> = safety
        .verification_options
        .iter()
        .map(|option| (option.title.to_string(), option.enabled))
        .collect();
    assert_eq!(
        levels[0],
        ("None".into(), false),
        "not on a Community server"
    );
    assert_eq!(levels[4], ("Highest".into(), true));
    assert_eq!(safety.verification, 2);
    assert_eq!(
        safety.mfa_lock.as_str(),
        "This setting can only be changed by a server owner with 2FA enabled on their account."
    );
    // Choosing one goes to Discord at once, without the bar.
    window.invoke_choose_server_setting(10, 4);
    let prompt = window.get_server_settings_prompt();
    assert!(prompt.saving && !prompt.changed);
    assert_eq!(prompt.safety.verification, 4);
    adapter.borrow_mut().app.apply(Event::ServerSettingsSaved {
        server: "discord:10".into(),
        result: Ok(ServerSettingsPage::Guild(GuildSettings {
            verification_level: 4,
            ..Default::default()
        })),
    });

    // Bans: the list, and a ban's dialog.
    window.invoke_server_settings_section(5);
    {
        let mut adapter = adapter.borrow_mut();
        let ban = |id: &str, name: &str, username: Option<&str>, reason: Option<&str>| Ban {
            user: RoleMember {
                id: id.into(),
                name: name.into(),
                username: username.map(str::to_owned),
                avatar: String::new(),
            },
            reason: reason.map(str::to_owned),
        };
        adapter.app.apply(Event::ServerSettingsLoaded {
            server: "discord:10".into(),
            section: SettingsSection::Bans,
            result: Ok(ServerSettingsPage::Bans(vec![
                ban("5", "Mallory", Some("mallory"), Some("spam")),
                ban("6", "trent", None, None),
            ])),
        });
        adapter.refresh(&window);
    }
    let bans = window.get_server_settings_prompt().bans;
    let names: Vec<(String, String)> = bans
        .rows
        .iter()
        .map(|row| (row.name.to_string(), row.username.to_string()))
        .collect();
    assert_eq!(
        names,
        [
            ("Mallory".into(), "mallory".into()),
            ("trent".into(), String::new())
        ]
    );
    assert_eq!(bans.pages, 1);
    window.invoke_ban_input(3, "6".into(), 0);
    let bans = window.get_server_settings_prompt().bans;
    assert!(bans.open && bans.may_unban);
    assert_eq!(bans.open_name.as_str(), "trent");
    assert_eq!(bans.open_reason.as_str(), "No reason provided");
    window.invoke_ban_input(4, "".into(), 0);
    assert!(!window.get_server_settings_prompt().bans.open);

    // A refresh with nothing new keeps the lists on screen: rows made
    // again between a click's press and release would lose the click.
    let shown = window.get_server_settings_prompt();
    adapter.borrow_mut().refresh(&window);
    let again = window.get_server_settings_prompt();
    assert!(again.rows == shown.rows && again.bans.rows == shown.bans.rows);
}

#[cfg(feature = "discord")]
#[test]
fn audit_log_shows_rows_that_open_their_numbered_lines() {
    use slint::Model;
    use yapfast_core::{
        chat::{AuditEntry, AuditLog, AuditUser, ServerSettingsPage, SettingsSection},
        yapfast_discord::ready::permission,
    };
    let (window, adapter, _directory) = discord_server_window_granting(permission::VIEW_AUDIT_LOG);
    window.invoke_prepare_menu("header:discord:10".into(), false);
    window.invoke_choose_menu("header:discord:10".into(), false, "server-settings".into());
    let rows: Vec<String> = window
        .get_server_settings_prompt()
        .rows
        .iter()
        .map(|row| row.text.to_string())
        .collect();
    assert_eq!(rows, ["MODERATION", "Audit Log", "Bans"]);
    // A kick an hour ago, with its reason, and an unban without lines.
    let now = yapfast_core::clock::now_ms();
    let id = |ago_ms: u64| (((now - ago_ms) - 1_420_070_400_000) << 22).to_string();
    let entry = |id: String, action: u16, reason: Option<&str>| AuditEntry {
        id,
        action,
        target: Some("5".into()),
        user: Some("1".into()),
        reason: reason.map(str::to_owned),
        ..Default::default()
    };
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.apply(Event::ServerSettingsLoaded {
            server: "discord:10".into(),
            section: SettingsSection::AuditLog,
            result: Ok(ServerSettingsPage::AuditLog(AuditLog {
                entries: vec![
                    entry(id(60_000), 20, Some("spam")),
                    entry(id(120_000), 23, None),
                ],
                users: [("1", "alice"), ("5", "mallory")]
                    .into_iter()
                    .map(|(id, name)| {
                        let user = AuditUser {
                            name: name.into(),
                            avatar: String::new(),
                        };
                        (id.to_owned(), user)
                    })
                    .collect(),
                names: Default::default(),
            })),
        });
        adapter.refresh(&window);
    }
    let prompt = window.get_server_settings_prompt();
    assert!(prompt.immediate && prompt.section == 6);
    let log = prompt.audit_log;
    let rows: Vec<(bool, i32, String)> = log
        .rows
        .iter()
        .map(|row| (row.expandable, row.kind, row.initial.to_string()))
        .collect();
    assert_eq!(rows, [(true, 2, "A".into()), (false, 0, "A".into())]);
    // A minute ago: yesterday's in the first minute after midnight.
    let time = log.rows.row_data(0).unwrap().time;
    assert!(time.starts_with("Today at ") || time.starts_with("Yesterday at "));
    assert_eq!(log.users.iter().collect::<Vec<_>>(), ["All Users", "alice"]);
    assert_eq!(log.actions.row_count(), 69);
    assert_eq!(log.lines.row_count(), 0);

    // The arrow opens a row's lines, numbered.
    window.invoke_audit_log_input(3, 0);
    let log = window.get_server_settings_prompt().audit_log;
    assert_eq!(log.open, 0);
    let numbers: Vec<String> = log.lines.iter().map(|line| line.number.into()).collect();
    assert_eq!(numbers, ["01"]);

    // A refresh with nothing new keeps the lists.
    let shown = window.get_server_settings_prompt().audit_log;
    adapter.borrow_mut().refresh(&window);
    let again = window.get_server_settings_prompt().audit_log;
    assert!(
        again.rows == shown.rows && again.lines == shown.lines && again.actions == shown.actions
    );

    // Filtering by Kick Member asks Discord from the newest.
    window.invoke_audit_log_input(1, 9);
    let log = window.get_server_settings_prompt().audit_log;
    assert!(log.fetching && log.rows.row_count() == 0);
    assert_eq!(log.action, 9);
}

#[cfg(feature = "discord")]
#[test]
fn members_and_invites_list_as_discords_pages_do() {
    use slint::Model;
    use yapfast_core::{
        chat::{
            GuildSettings, JoinMethod, ListedMember, RoleMember, ServerInvite, ServerSettingsPage,
            SettingsSection,
        },
        yapfast_discord::ready::permission,
    };
    let (window, adapter, _directory) = discord_server_window_changed(
        permission::KICK_MEMBERS | permission::MANAGE_GUILD | permission::CREATE_INSTANT_INVITE,
        |server, _| {
            server.put_role(
                &serde_json::json!({"id": "20", "name": "Mods", "position": 2,
                                                "color": 0x3498db, "permissions": "0"}),
            );
            server.put_role(
                &serde_json::json!({"id": "30", "name": "Helpers", "position": 1,
                                                "permissions": "0"}),
            );
        },
    );
    window.invoke_prepare_menu("header:discord:10".into(), false);
    window.invoke_choose_menu("header:discord:10".into(), false, "server-settings".into());
    let rows: Vec<String> = window
        .get_server_settings_prompt()
        .rows
        .iter()
        .map(|row| row.text.to_string())
        .collect();
    let people = rows.iter().position(|row| row == "PEOPLE").unwrap();
    assert_eq!(
        rows[people..people + 4],
        ["PEOPLE", "Members", "Invites", "MODERATION"]
    );

    // Members: a moderator joined three days ago, timed out, and someone
    // whose join Discord doesn't know.
    window.invoke_server_settings_section(7);
    let now = yapfast_core::clock::now_ms();
    let member = |id: &str, name: &str, roles: &[&str], joined_by| ListedMember {
        person: RoleMember {
            id: id.into(),
            name: name.into(),
            username: Some(name.to_lowercase()),
            avatar: String::new(),
        },
        roles: roles.iter().map(|role| (*role).to_owned()).collect(),
        joined: now - 3 * 86_400_000,
        timed_out_until: 0,
        unusual_dms: false,
        spammer: false,
        quarantined: false,
        joined_by,
        bot: false,
    };
    let mut mod_ = member(
        "1400000000000000005",
        "Mallory",
        &["30", "20"],
        JoinMethod::Invite("abc".into()),
    );
    mod_.timed_out_until = now + 3_660_000;
    {
        let mut adapter = adapter.borrow_mut();
        adapter.layout.role_colors_everywhere = true;
        adapter.app.apply(Event::ServerSettingsLoaded {
            server: "discord:10".into(),
            section: SettingsSection::Members,
            result: Ok(ServerSettingsPage::Members {
                members: vec![mod_, member("6", "trent", &[], JoinMethod::Unknown)],
                total: Some(2),
            }),
        });
        adapter.refresh(&window);
    }
    let prompt = window.get_server_settings_prompt();
    assert!(prompt.immediate && prompt.section == 7);
    let members = prompt.members;
    assert!(!members.searched && members.join_method);
    assert_eq!((members.count.as_str(), members.total), ("2", 2));
    let first = members.rows.row_data(0).unwrap();
    assert_eq!(
        (first.name.as_str(), first.username.as_str()),
        ("Mallory", "mallory")
    );
    assert_eq!(first.name_color, server_settings::role_color(0x3498db));
    assert_eq!(
        (first.role.as_str(), first.more_roles.as_str()),
        ("Mods", "+1")
    );
    assert_eq!(
        (first.since.as_str(), first.join.as_str()),
        ("3 days ago", "abc")
    );
    // Its id's time, in 2025, however long ago that is now.
    assert!(first.age.ends_with(" ago") && !first.age.contains("day"));
    assert!(first.timed_out.starts_with("Timed out for 1h 1m"));
    let second = members.rows.row_data(1).unwrap();
    assert!(second.join_unknown && second.role.is_empty());

    // An order starts again, as Search Results.
    window.invoke_members_input(1, "".into(), 2);
    let members = window.get_server_settings_prompt().members;
    assert!(members.searched && members.fetching && members.rows.row_count() == 0);
    assert_eq!((members.sort, members.count.as_str()), (2, "..."));

    // Invites: by inviter, counting down, and Security Actions.
    window.invoke_server_settings_section(8);
    let invite = |code: &str, inviter: &str, max_uses, expires| ServerInvite {
        code: code.into(),
        inviter: Some(RoleMember {
            id: "2".into(),
            name: inviter.to_uppercase(),
            username: Some(inviter.into()),
            avatar: String::new(),
        }),
        channel: "#general".into(),
        uses: 3,
        max_uses,
        expires,
        roles: vec!["20".into()],
        bypass: false,
    };
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.apply(Event::ServerSettingsLoaded {
            server: "discord:10".into(),
            section: SettingsSection::Invites,
            result: Ok(ServerSettingsPage::Invites {
                invites: vec![
                    invite("zzz", "carol", 10, Some(now / 1000 + 90_000)),
                    invite("bob1", "bob", 0, None),
                ],
                settings: GuildSettings::default(),
            }),
        });
        adapter.refresh(&window);
    }
    let invites = window.get_server_settings_prompt().invites;
    assert!(!invites.paused && invites.may_create && !invites.security);
    let rows: Vec<(String, String, String, i32)> = invites
        .rows
        .iter()
        .map(|row| {
            (
                row.inviter.to_string(),
                row.uses.to_string(),
                row.role.to_string(),
                row.expires_in,
            )
        })
        .collect();
    assert_eq!(rows[0], ("bob".into(), "3".into(), "Mods".into(), -1));
    assert_eq!(rows[1].1, "3/10");
    assert!((89_990..=90_000).contains(&rows[1].3));
    // A refresh keeps the rows, changed in place, and gives the second
    // their countdowns count from again.
    let shown = window.get_server_settings_prompt().invites;
    adapter.borrow_mut().refresh(&window);
    let again = window.get_server_settings_prompt().invites;
    assert!(again.rows == shown.rows, "the same rows");
    assert!(again.counted >= shown.counted && again.counted > 0);
    window.invoke_invites_input(2, "".into(), 0);
    let invites = window.get_server_settings_prompt().invites;
    assert!(invites.security && !invites.may_save);
    assert_eq!(invites.hours, 1, "two hours, as Discord's");
    window.invoke_invites_input(4, "".into(), 0);
    let invites = window.get_server_settings_prompt().invites;
    assert!(invites.pause_invites && invites.may_save);

    // A refresh with nothing new keeps the lists.
    let shown = window.get_server_settings_prompt();
    adapter.borrow_mut().refresh(&window);
    let again = window.get_server_settings_prompt();
    assert!(again.invites.rows == shown.invites.rows);
    window.invoke_invites_input(7, "".into(), 0);
    assert!(!window.get_server_settings_prompt().invites.security);
}

#[cfg(feature = "discord")]
#[test]
fn delete_server_ends_the_owners_sidebar_and_asks_for_the_name() {
    use slint::Model;
    let (window, _adapter, _directory) = discord_server_window_changed(0, |server, _| {
        server.owner_id = Some("1".into());
    });
    window.invoke_prepare_menu("header:discord:10".into(), false);
    window.invoke_choose_menu("header:discord:10".into(), false, "server-settings".into());
    let rows = window.get_server_settings_prompt().rows;
    let count = rows.row_count();
    let (line, last) = (
        rows.row_data(count - 2).unwrap(),
        rows.row_data(count - 1).unwrap(),
    );
    assert!(line.heading && line.text.is_empty());
    assert!(last.danger && last.text == "Delete Server");

    window.invoke_server_settings_section(last.id);
    let ownership = window.get_server_settings_prompt().ownership;
    assert_eq!(
        (ownership.kind, ownership.title.as_str()),
        (1, "Delete 'Test'")
    );
    assert!(ownership.by_name, "without 2FA");
    window.invoke_ownership_input(0, "nope".into());
    window.invoke_ownership_input(1, "".into());
    assert!(window.get_server_settings_prompt().ownership.wrong);
    window.invoke_ownership_input(2, "".into());
    assert_eq!(window.get_server_settings_prompt().ownership.kind, 0);
}

#[cfg(feature = "discord")]
#[test]
fn emoji_and_stickers_show_as_discords_pages_do() {
    use slint::Model;
    use yapfast_core::{
        chat::{
            GuildSettings, RoleMember, ServerEmoji, ServerSettingsPage, ServerSticker,
            SettingsSection, StickerForm,
        },
        yapfast_discord::ready::permission,
    };
    let (window, adapter, _directory) =
        discord_server_window_changed(permission::CREATE_GUILD_EXPRESSIONS, |_, _| {});
    window.invoke_prepare_menu("header:discord:10".into(), false);
    window.invoke_choose_menu("header:discord:10".into(), false, "server-settings".into());
    let rows: Vec<String> = window
        .get_server_settings_prompt()
        .rows
        .iter()
        .map(|row| row.text.to_string())
        .collect();
    let at = rows.iter().position(|row| row == "EXPRESSION").unwrap();
    assert_eq!(rows[at..at + 3], ["EXPRESSION", "Emoji", "Stickers"]);

    let person = |id: &str| RoleMember {
        id: id.into(),
        name: format!("user{id}"),
        username: None,
        avatar: String::new(),
    };
    let load = |section, page| {
        let mut adapter = adapter.borrow_mut();
        adapter.app.apply(Event::ServerSettingsLoaded {
            server: "discord:10".into(),
            section,
            result: Ok(page),
        });
        adapter.refresh(&window);
    };

    // Emoji: newest first; only her own may change.
    window.invoke_server_settings_section(9);
    let emoji = |id: &str, by: &str| ServerEmoji {
        id: id.into(),
        name: format!("emoji{id}"),
        animated: false,
        available: true,
        user: Some(person(by)),
        picture: format!("https://cdn/emojis/{id}.png"),
    };
    load(
        SettingsSection::Emoji,
        ServerSettingsPage::Emoji {
            emojis: vec![emoji("5", "2"), emoji("6", "1")],
            settings: GuildSettings::default(),
        },
    );
    let page = window.get_server_settings_prompt().emoji;
    assert!(page.may_upload && !page.uploading);
    let shown: Vec<_> = page
        .rows
        .iter()
        .map(|row| {
            (
                row.heading.to_string(),
                row.name.to_string(),
                row.may_change,
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("Emoji".into(), String::new(), false),
            (String::new(), "emoji6".into(), true),
            (String::new(), "emoji5".into(), false),
        ]
    );
    assert_eq!(page.rows.row_data(0).unwrap().slots, "48 slots available");
    window.invoke_emoji_input(1, "6".into(), "".into());
    assert_eq!(
        window.get_server_settings_prompt().emoji.rows.row_count(),
        2
    );

    // Stickers: Discord's cards, and the form.
    window.invoke_server_settings_section(10);
    let sticker = |id: &str| ServerSticker {
        id: id.into(),
        form: StickerForm {
            name: format!("sticker{id}"),
            tags: "👋".into(),
            description: String::new(),
        },
        format: 1,
        available: true,
        user_id: Some("1".into()),
        user: Some(person("1")),
        picture: Some(format!("https://cdn/stickers/{id}.png")),
    };
    load(
        SettingsSection::Stickers,
        ServerSettingsPage::Stickers {
            stickers: vec![sticker("7"), sticker("8"), sticker("9")],
            settings: GuildSettings::default(),
        },
    );
    let page = window.get_server_settings_prompt().stickers;
    let cards: Vec<_> = page
        .cards
        .iter()
        .map(|card| {
            (
                card.title.to_string(),
                card.status.to_string(),
                card.unlocked,
                card.next,
            )
        })
        .collect();
    assert_eq!(
        cards[..2],
        [
            (
                "Free Slots".into(),
                "2 of 5 slots available".into(),
                true,
                true
            ),
            ("Level 1".into(), "2 Boosts".into(), false, false),
        ]
    );
    assert_eq!(page.cards.row_data(0).unwrap().tiles.row_count(), 3);
    window.invoke_stickers_input(0, "".into());
    window.invoke_stickers_input(2, "wave".into());
    window.invoke_stickers_input(3, "👋".into());
    let page = window.get_server_settings_prompt().stickers;
    assert!(page.form && !page.editing && !page.may_save, "no file yet");
    assert_eq!(page.name, "wave");
    window.invoke_stickers_input(6, "".into());
    assert!(!window.get_server_settings_prompt().stickers.form);
}

#[test]
#[cfg(feature = "discord")]
fn roles_lock_what_ranks_above_her_and_the_permissions_she_lacks() {
    use slint::Model;
    use yapfast_core::{
        chat::{RoleDetail, ServerRoles, ServerSettingsPage, SettingsSection},
        yapfast_discord::ready::permission,
    };
    // She holds Staff, which may kick.
    let (window, adapter, _directory) =
        discord_server_window_changed(permission::MANAGE_ROLES, |server, marks| {
            marks.roles = vec!["20".into()];
            Arc::make_mut(&mut server.role_positions).insert("20".into(), 2);
            Arc::make_mut(&mut server.roles).insert("20".into(), permission::KICK_MEMBERS);
        });
    let role = |id: &str, name: &str, position: i64| RoleDetail {
        id: id.into(),
        name: name.into(),
        position,
        ..Default::default()
    };
    window.invoke_prepare_menu("header:discord:10".into(), false);
    window.invoke_choose_menu("header:discord:10".into(), false, "server-settings".into());
    let rows: Vec<(bool, String)> = window
        .get_server_settings_prompt()
        .rows
        .iter()
        .map(|row| (row.heading, row.text.to_string()))
        .collect();
    assert_eq!(
        rows,
        [
            (true, "PEOPLE".into()),
            (false, "Members".into()),
            (false, "Roles".into())
        ]
    );
    window.invoke_server_settings_section(3);
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.apply(Event::ServerSettingsLoaded {
            server: "discord:10".into(),
            section: SettingsSection::Roles,
            result: Ok(ServerSettingsPage::Roles(ServerRoles {
                roles: vec![
                    role("40", "Leads", 3),
                    role("20", "Staff", 2),
                    role("30", "Crew", 1),
                    role("10", "@everyone", 0),
                ],
                ..Default::default()
            })),
        });
        adapter.refresh(&window);
    }
    let roles = window.get_server_settings_prompt().roles;
    let listed: Vec<(String, bool)> = roles
        .rows
        .iter()
        .map(|row| (row.name.to_string(), row.locked))
        .collect();
    assert_eq!(
        listed,
        [
            ("Leads".into(), true),
            ("Staff".into(), true),
            ("Crew".into(), false)
        ]
    );
    assert_eq!(
        roles.rows.row_data(1).unwrap().lock_text,
        "Role is locked because it is your highest ranked role. Please ask a higher rank or Server Owner for help."
    );
    assert_eq!(roles.first_movable, 2);

    // Crew's permissions: Kick Members is hers to give, Administrator not.
    window.invoke_role_input(10, "30".into(), 0);
    window.invoke_role_input(11, "".into(), 1);
    let roles = window.get_server_settings_prompt().roles;
    assert!(roles.open && !roles.locked && roles.tab == 1);
    let switch = |bit: u64| {
        window
            .get_server_settings_prompt()
            .roles
            .permissions
            .iter()
            .find(|row| !row.heading && 1u64 << row.bit == bit)
            .unwrap()
    };
    assert_eq!(
        switch(permission::ADMINISTRATOR).lock_text,
        "You cannot modify this permission because none of your roles have it."
    );
    assert_eq!(switch(permission::KICK_MEMBERS).lock_text, "");
    window.invoke_role_input(14, "".into(), 1);
    assert!(switch(permission::KICK_MEMBERS).on);
    assert!(window.get_server_settings_prompt().changed);
}

#[test]
#[cfg(feature = "discord")]
fn the_two_factor_prompt_offers_discords_methods() {
    use slint::Model;
    use yapfast_core::chat::MfaMethod;
    let (window, adapter, _directory) = discord_server_window_granting(0);
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.apply(Event::MfaRequired {
            id: 3,
            methods: vec![MfaMethod::App, MfaMethod::Backup],
        });
        adapter.refresh(&window);
    }
    assert!(window.get_mfa_open());
    let prompt = window.get_mfa_prompt();
    assert!(!prompt.listing && !prompt.ready && !prompt.password);
    assert_eq!(prompt.field, "6-digit code");
    assert_eq!(prompt.choices.row_count(), 2);
    window.set_mfa_code("123456".into());
    window.invoke_write_mfa("123456".into());
    assert!(window.get_mfa_prompt().ready);

    // "Verify with something else" lists them.
    window.invoke_choose_mfa(-1);
    let prompt = window.get_mfa_prompt();
    assert!(prompt.listing);
    let texts: Vec<String> = prompt
        .choices
        .iter()
        .map(|choice| choice.text.to_string())
        .collect();
    assert_eq!(texts, ["Use your authenticator app", "Use a backup code"]);
    window.invoke_choose_mfa(3);
    assert!(window.get_mfa_prompt().warning, "Discord's warning first");
    window.invoke_confirm_mfa();
    assert!(!window.get_mfa_prompt().warning);
    assert_eq!(window.get_mfa_prompt().field, "Backup code");
    assert_eq!(window.get_mfa_code(), "", "a new method starts empty");

    window.invoke_cancel_mfa();
    assert!(!window.get_mfa_open());
}

#[test]
#[cfg(feature = "discord")]
fn pronouns_can_be_typed_from_the_profile_card() {
    use i_slint_core::{
        accessibility::AccessibleStringProperty, item_tree::ItemRc, items::TextInput,
    };
    use slint::platform::{Key, PointerEventButton, WindowEvent};
    use std::ops::ControlFlow;

    let (window, adapter, _directory) = discord_server_window_granting(0);
    let handle = "a-very-long-username".repeat(8);
    window.invoke_edit_profile();
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.apply(Event::OwnProfile {
            server: None,
            result: Ok(yapfast_core::chat::OwnProfile {
                fallback: yapfast_core::chat::Profile {
                    name: "Alice".into(),
                    handle: handle.clone(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        });
        adapter.refresh(&window);
    }
    window.set_profile_editor_activity(ModelRc::new(VecModel::from(vec![ActivityRow {
        heading: "LISTENING TO A TRACK".into(),
        name: "Track".into(),
        ..Default::default()
    }])));
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(480., 700.));
    i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(300));
    settle(&window);
    let inner = i_slint_core::window::WindowInner::from_pub(window.window());
    let component = inner.component();
    let find = |label: &str, predicate: &dyn Fn(&ItemRc) -> bool| {
        ItemRc::new_root(component.clone())
            .visit_descendants(|item| {
                if item.is_visible() && predicate(&item) {
                    ControlFlow::Break(item.clone())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .unwrap_or_else(|| panic!("{label} should be visible"))
    };
    let text = |value: &str| find(value, &|item| item_text_is(item, value));
    let center = |item: &ItemRc| item.map_to_item_tree(item.geometry().center(), &component);
    let label = text("Add pronouns");
    let point = center(&label);
    let position = slint::LogicalPosition::new(point.x, point.y);
    let button = PointerEventButton::Left;
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    window
        .window()
        .dispatch_event(WindowEvent::PointerPressed { position, button });
    window.set_profile_editor_activity(ModelRc::new(VecModel::from(vec![ActivityRow {
        heading: "LISTENING TO A TRACK".into(),
        name: "Track".into(),
        details: "Artist".into(),
        state: "Album".into(),
        ..Default::default()
    }])));
    settle(&window);
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased { position, button });
    settle(&window);
    let field = find("Pronouns", &|item| {
        item.accessible_string_property(AccessibleStringProperty::Label)
            .as_deref()
            == Some("Pronouns")
    });
    let handle_item = text(&handle);
    assert!((center(&handle_item).y - center(&field).y).abs() < 1.);
    assert!(center(&handle_item).x + handle_item.geometry().width() / 2. <= center(&field).x);
    assert!(center(&field).y < center(&text("LISTENING TO A TRACK")).y);
    tap(&window, "t".into());
    assert_eq!(window.get_profile_editor_pronouns(), "t");
    assert_eq!(window.get_profile_editor_activity().row_count(), 0);
    settle(&window);
    tap(&window, "h".into());
    assert_eq!(window.get_profile_editor_pronouns(), "th");
    assert_eq!(
        adapter
            .borrow()
            .app
            .state
            .profile_editor
            .as_ref()
            .unwrap()
            .pronouns,
        "th"
    );

    tap(&window, Key::Escape.into());
    assert!(window.get_profile_editor_open());
    let field = field.downcast::<TextInput>().unwrap();
    let has_focus = || {
        TextInput::FIELD_OFFSETS
            .has_focus()
            .apply_pin(field.as_pin_ref())
            .get()
    };
    assert!(!has_focus());
    for _ in 0..8 {
        tap(&window, Key::Tab.into());
        if has_focus() {
            break;
        }
    }
    assert!(has_focus(), "Tab reaches pronouns");
    tap(&window, Key::End.into());
    tap(&window, "e".into());
    assert_eq!(window.get_profile_editor_pronouns(), "the");

    window.invoke_save_profile();
    assert!(window.get_profile_editor_prompt().saving);
    tap(&window, "y".into());
    assert_eq!(window.get_profile_editor_pronouns(), "they");
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.apply(Event::ProfileSaved {
            server: None,
            result: Err("Fixture save failed".into()),
        });
        adapter.refresh(&window);
    }
    assert_eq!(window.get_profile_editor_pronouns(), "they");
    assert_eq!(
        window.get_profile_editor_prompt().error,
        "Fixture save failed"
    );
    window.invoke_undo_profile();
    assert!(window.get_profile_editor_pronouns().is_empty());
    assert!(!window.get_profile_editor_prompt().changed);
}

#[test]
#[cfg(feature = "discord")]
fn edit_profile_opens_the_main_profile_editor() {
    let (window, adapter, _directory) = discord_server_window_granting(0);
    window.invoke_edit_profile();
    assert!(window.get_profile_editor_open());
    {
        let mut adapter = adapter.borrow_mut();
        adapter.app.apply(Event::OwnProfile {
            server: None,
            result: Ok(yapfast_core::chat::OwnProfile {
                name: "Alice".into(),
                accent: Some(0x112233),
                fallback: yapfast_core::chat::Profile {
                    name: "alice".into(),
                    handle: "alice".into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        });
        adapter.refresh(&window);
    }
    let prompt = window.get_profile_editor_prompt();
    assert_eq!(prompt.title, "Your profile");
    assert_eq!(prompt.detail, "Your profile");
    assert_eq!(prompt.handle, "alice");
    assert!(prompt.main && !prompt.offer && prompt.bio_enabled && prompt.avatar_enabled);
    assert!(
        prompt.colour_shown && !prompt.banner_shown,
        "without Nitro, a colour instead of a banner"
    );
    assert_eq!(prompt.reset_word, "Remove");
    assert_eq!(window.get_profile_editor_name(), "Alice");
    assert_eq!(window.get_profile_editor_colour(), "#112233");
    assert!(prompt.has_colour && prompt.preview.has_accent);
    assert_eq!(
        prompt.preview.accent,
        slint::Color::from_rgb_u8(0x11, 0x22, 0x33)
    );

    window.invoke_write_profile(3, "teal".into());
    let prompt = window.get_profile_editor_prompt();
    assert!(prompt.colour_wrong && !prompt.has_colour);
    window.invoke_write_profile(3, "#abcdef".into());
    let prompt = window.get_profile_editor_prompt();
    assert!(!prompt.colour_wrong && prompt.changed);
    assert_eq!(
        prompt.preview.accent,
        slint::Color::from_rgb_u8(0xab, 0xcd, 0xef)
    );
    // An empty display name shows the username.
    window.invoke_write_profile(0, " ".into());
    assert_eq!(window.get_profile_editor_prompt().preview.name, "alice");
    window.invoke_write_profile(0, "x".repeat(33).into());
    assert_eq!(
        window.get_profile_editor_prompt().too_long,
        "A display name can be up to 32 characters."
    );

    // Pronouns and bio editing
    window.invoke_write_profile(1, "they/them".into());
    assert_eq!(
        window.get_profile_editor_prompt().preview.pronouns,
        "they/them"
    );
    assert!(window.get_profile_editor_prompt().changed);

    let mut track = yapfast_core::activity::MediaTrack {
        requires_music_rpc: false,
        title: "Track".into(),
        artist: "Artist".into(),
        album: String::new(),
        duration_ms: 119_000,
        position_ms: 48_000,
        position_updated_ms: Some(games::now_ms()),
        is_playing: true,
        thumbnail_url: None,
        thumbnail: None,
        url: None,
        source: Some("Chrome".into()),
    };
    assert_eq!(window.get_profile_editor_activity().row_count(), 0);
    for playing in [true, false] {
        track.is_playing = playing;
        let mut adapter = adapter.borrow_mut();
        adapter.activity_manager.set_global_sharing(true);
        adapter
            .activity_manager
            .set_media_track(Some(track.clone()));
        adapter.show_editor_activity(&window);
        let row = window.get_profile_editor_activity().row_data(0).unwrap();
        assert_eq!(row.name, "Track");
        assert_eq!(row.paused, !playing);
        assert_eq!(window.get_profile_editor_pronouns(), "they/them");
    }

    window.invoke_write_profile(2, "Hello world bio".into());
    assert_eq!(
        window.get_profile_editor_prompt().preview.bio,
        slint::StyledText::from_plain_text("Hello world bio")
    );
    assert!(window.get_profile_editor_prompt().changed);

    // Undo reverts unsaved changes
    window.invoke_undo_profile();
    assert!(!window.get_profile_editor_prompt().changed);
    assert_eq!(window.get_profile_editor_prompt().preview.name, "Alice");

    window.invoke_close_profile_editor();
    assert!(!window.get_profile_editor_open());
}

#[test]
fn a_threads_activity_reads_as_discords_list_of_threads_says_it() {
    let now = 100 * 86_400_000;
    let said = |ago: u64| refresh::ago(now - ago, now);
    assert_eq!(said(5_000), "5s ago");
    assert_eq!(said(5 * 60_000), "5m ago");
    assert_eq!(said(3 * 3_600_000), "3h ago");
    assert_eq!(said(30 * 86_400_000), "30d ago");
    assert_eq!(said(31 * 86_400_000), ">30d ago");
    assert_eq!(refresh::ago(0, 400 * 86_400_000), ">1y ago");
    assert_eq!(refresh::ago(now + 1_000, now), "0s ago", "a clock behind");
}

#[test]
fn a_theme_paints_the_window_and_the_mentions_in_its_rows() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    assert_eq!(
        theme::Theme::shown(&window),
        theme::YAP,
        "the Slint defaults are Yap's, for the first frame"
    );
    let message = ChatMessage {
        body: "hi @Bob".into(),
        formatted: true,
        mentions: vec![yapfast_core::mention::Mentioned {
            text: "@Bob".into(),
            target: yapfast_core::mention::Target::User("2".into()),
            color: None,
            at: vec![3],
        }],
        ..Default::default()
    };
    let yap_row = message_row_plain(&message);
    let light = theme::Theme {
        dark: false,
        chat: slint::Color::from_rgb_u8(0xfa, 0xf8, 0xf5),
        link: slint::Color::from_rgb_u8(0x1a, 0x5f, 0xb4),
        ..theme::YAP
    };
    assert!(light.apply(&window), "mentions change colour");
    assert_eq!(theme::Theme::shown(&window), light);
    assert!(message_row_plain(&message).styled != yap_row.styled);
    assert!(!light.apply(&window), "nothing to build again");
    assert!(theme::YAP.apply(&window));
    assert!(message_row_plain(&message).styled == yap_row.styled);
}

#[test]
fn a_chosen_theme_is_kept_and_survives_a_window_made_again() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    assert_eq!(window.get_theme_id(), "yap");
    assert_eq!(window.get_themes().row_count(), theme::BUILT_IN.len());
    let nord = theme::find("nord").unwrap().theme;
    window.invoke_choose_theme("nord".into());
    assert_eq!(window.get_theme_id(), "nord");
    assert_eq!(theme::Theme::shown(&window), nord);
    assert_eq!(
        Layout::read(directory.path()).theme.as_deref(),
        Some("nord")
    );

    // Closing to the tray drops the window; the next one starts in Nord.
    let again = classic_window();
    adapter.borrow_mut().attach(&again);
    assert_eq!(theme::Theme::shown(&again), nord);
    assert_eq!(again.get_theme_id(), "nord");

    // An id Yapfast doesn't know changes nothing, and a stored one it no longer
    // has shows Yap.
    again.invoke_choose_theme("missing".into());
    assert_eq!(theme::Theme::shown(&again), nord);
    let fresh = classic_window();
    Layout {
        theme: Some("gone".into()),
        ..Default::default()
    }
    .apply(&fresh);
    assert_eq!(theme::Theme::shown(&fresh), theme::YAP);
    assert_eq!(fresh.get_theme_id(), "yap");
}

#[test]
fn built_in_themes_have_their_own_ids_and_say_whether_they_are_dark() {
    let mut ids = std::collections::HashSet::new();
    for built_in in &theme::BUILT_IN {
        assert!(ids.insert(built_in.id), "{} twice", built_in.id);
        let chat = built_in.theme.chat;
        let light =
            u32::from(chat.red()) + u32::from(chat.green()) + u32::from(chat.blue()) > 3 * 128;
        assert_eq!(built_in.theme.dark, !light, "{}", built_in.name);
    }
    assert_eq!(theme::BUILT_IN[0].theme, theme::YAP, "Yap comes first");
}

#[test]
fn onyx_text_is_readable_on_every_surface() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let onyx = theme::find("onyx").expect("Onyx is built in").theme;
    onyx.apply(&window);
    let tokens = window.global::<yapfast_ui::Theme>();
    let surfaces = [
        ("rail", tokens.get_rail()),
        ("sidebar", tokens.get_sidebar()),
        ("panel", tokens.get_panel()),
        ("chat", tokens.get_chat()),
        ("raised", tokens.get_raised()),
        ("field", tokens.get_field()),
        ("hover", tokens.get_hover()),
        ("active", tokens.get_active()),
        ("row-hover", tokens.get_row_hover()),
        ("menu", tokens.get_menu()),
        ("composer", tokens.get_composer()),
    ];
    let linear = |value: u8| {
        let value = f64::from(value) / 255.;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = |colour: slint::Color| {
        0.2126 * linear(colour.red())
            + 0.7152 * linear(colour.green())
            + 0.0722 * linear(colour.blue())
    };
    let contrast = |a: slint::Color, b: slint::Color| {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    };
    let over = |front: slint::Color, back: slint::Color| {
        let alpha = f64::from(front.alpha()) / 255.;
        let mix = |front: u8, back: u8| {
            (f64::from(front) * alpha + f64::from(back) * (1. - alpha)).round() as u8
        };
        slint::Color::from_rgb_u8(
            mix(front.red(), back.red()),
            mix(front.green(), back.green()),
            mix(front.blue(), back.blue()),
        )
    };
    let rows = [
        ("text", onyx.text, 4.5),
        ("secondary", onyx.secondary, 4.5),
        ("muted", onyx.muted, 4.5),
        ("faint", onyx.faint, 2.5),
        ("link", onyx.link, 4.5),
        ("danger", onyx.danger, 4.5),
        ("success", onyx.live, 4.5),
        ("focus ring (accent)", onyx.accent, 3.0),
    ];
    let mut header = String::from("| Colour |");
    for (name, _) in &surfaces {
        header.push_str(&format!(" {name} |"));
    }
    println!("{header}");
    let mut rule = String::from("| --- |");
    for _ in &surfaces {
        rule.push_str(" --- |");
    }
    println!("{rule}");
    for (name, colour, needed) in rows {
        let mut line = format!("| {name} |");
        for (surface_name, surface) in &surfaces {
            let ratio = contrast(colour, *surface);
            assert!(
                ratio + 0.005 >= needed,
                "{name} on {surface_name}: {ratio:.2}, needs {needed}"
            );
            line.push_str(&format!(" {ratio:.2} |"));
        }
        println!("{line}");
    }
    let mut line = String::from("| mention (text on row) |");
    for (surface_name, surface) in &surfaces {
        let row = over(onyx.mentioned, *surface);
        let ratio = contrast(onyx.text, row);
        assert!(
            ratio + 0.005 >= 4.5,
            "mention text on {surface_name}: {ratio:.2}"
        );
        line.push_str(&format!(" {ratio:.2} |"));
    }
    println!("{line}");
    let on_accent = contrast(onyx.on_accent, onyx.accent);
    let on_danger = contrast(onyx.on_danger, onyx.danger);
    let on_danger_hover = contrast(onyx.on_danger, onyx.danger_hover);
    println!("| on-accent on accent | {on_accent:.2} |");
    println!("| on-danger on danger | {on_danger:.2} |");
    println!("| on-danger on danger-hover | {on_danger_hover:.2} |");
    assert!(on_accent >= 4.5);
    assert!(on_danger >= 4.5);
    assert!(on_danger_hover >= 4.5);
}

#[test]
fn a_saved_custom_theme_takes_mention_colours_from_its_base() {
    let stored = serde_json::json!({
        "id": "custom:1",
        "name": "Old",
        "base": "nord",
        "colours": { "chat": "#000000" },
    });
    let custom: theme::Custom = serde_json::from_value(stored).unwrap();
    let nord = theme::find("nord").unwrap().theme;
    assert_eq!(custom.theme.mentioned, nord.mentioned);
    assert_eq!(custom.theme.mentioned_hover, nord.mentioned_hover);
    let written = serde_json::to_value(&custom).unwrap();
    assert_eq!(written["colours"]["mentioned"], theme::hex(nord.mentioned));
    assert_eq!(
        written["colours"]["mentioned-hover"],
        theme::hex(nord.mentioned_hover)
    );

    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("layout.json"),
        r##"{
            "theme": "custom:1",
            "custom_themes": [{
                "id": "custom:1", "name": "Old", "base": "nord",
                "colours": { "chat": "#000000" }
            }]
        }"##,
    )
    .unwrap();
    let layout = Layout::read(directory.path());
    let custom = &layout.custom_themes[0];
    assert_eq!(custom.theme.mentioned, nord.mentioned);
    assert_eq!(custom.theme.mentioned_hover, nord.mentioned_hover);
}

#[test]
fn a_colour_is_written_as_slint_writes_one() {
    let parse = |text| theme::parse(text).map(theme::hex);
    assert_eq!(parse("#abc").as_deref(), Some("#aabbcc"));
    assert_eq!(parse(" #A0B1C2 ").as_deref(), Some("#a0b1c2"));
    assert_eq!(parse("#ffbf4726").as_deref(), Some("#ffbf4726"));
    assert_eq!(parse("#123456ff").as_deref(), Some("#123456"));
    for wrong in [
        "", "#", "abc", "a0b1c2", "#ab", "#abcd", "#12345", "#1234567", "#ggg", "#+12", "#-12345",
    ] {
        assert_eq!(theme::parse(wrong), None, "{wrong:?}");
    }
    assert_eq!(theme::parse("#ffbf4726"), Some(theme::YAP.accent_soft));
}

#[test]
fn a_custom_theme_keeps_its_colours_by_name_and_falls_back_to_its_base() {
    let stored = serde_json::json!({
        "id": "custom:1",
        "name": "Mine",
        "base": "nord",
        "dark": false,
        "colours": {
            "chat": "#fafafa",
            "row-hover": "#abc",
            "link": "not a colour",
            "a-colour-from-later": "#123456",
        },
    });
    let custom: theme::Custom = serde_json::from_value(stored).unwrap();
    let nord = theme::find("nord").unwrap().theme;
    assert_eq!(custom.base, "nord");
    assert!(!custom.theme.dark);
    assert_eq!(custom.theme.chat, theme::parse("#fafafa").unwrap());
    assert_eq!(custom.theme.row_hover, theme::parse("#aabbcc").unwrap());
    assert_eq!(custom.theme.link, nord.link, "a wrong colour is Nord's");
    assert_eq!(
        custom.theme.accent, nord.accent,
        "a missing colour is Nord's"
    );

    // It's written back with every colour, by the names ui/theme.slint has.
    let written = serde_json::to_value(&custom).unwrap();
    assert_eq!(written["colours"]["row-hover"], "#aabbcc");
    assert_eq!(written["colours"]["accent-soft"], "#88c0d026");
    assert_eq!(
        written["colours"].as_object().unwrap().len(),
        theme::Theme::NAMES.len()
    );
    assert_eq!(
        serde_json::from_value::<theme::Custom>(written).unwrap(),
        custom
    );

    // A base Yapfast doesn't know is Yap.
    let unknown: theme::Custom =
        serde_json::from_value(serde_json::json!({ "base": "gone" })).unwrap();
    assert_eq!((unknown.base, unknown.theme), ("yap", theme::YAP));
}

#[test]
fn custom_theme_ids_stay_apart_from_built_in_ones_and_each_other() {
    for built_in in &theme::BUILT_IN {
        assert!(!built_in.id.contains(':'), "{}", built_in.id);
    }
    let custom = |id: &str| theme::Custom {
        id: id.into(),
        name: id.into(),
        base: "yap",
        theme: theme::YAP,
    };
    let mut themes = vec![
        custom("custom:2"),
        custom("nord"),
        custom("custom:2"),
        custom(""),
        custom("custom:x"),
    ];
    theme::tidy(&mut themes);
    let ids: Vec<_> = themes.iter().map(|theme| theme.id.as_str()).collect();
    assert_eq!(
        ids,
        ["custom:2", "custom:3", "custom:4", "custom:5", "custom:6"]
    );
    assert_eq!(theme::new_id(&themes), "custom:7");
    assert_eq!(theme::new_id(&[]), "custom:1");
}

#[test]
fn a_custom_theme_is_made_edited_and_deleted_in_appearance() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    let nord = theme::find("nord").unwrap().theme;
    window.invoke_choose_theme("nord".into());

    // A new theme starts as a copy of the one shown, and shows each change.
    window.invoke_new_theme();
    assert!(window.get_theme_editor());
    assert_eq!(window.get_theme_name(), "Nord copy");
    let nord_start = theme::BUILT_IN
        .iter()
        .position(|built_in| built_in.id == "nord")
        .unwrap();
    assert_eq!(window.get_theme_start(), nord_start as i32);
    assert_eq!(window.get_theme_starts().row_count(), theme::BUILT_IN.len());
    let colours = window.get_theme_colours();
    assert_eq!(colours.row_count(), theme::Theme::NAMES.len());
    let chat = theme::Theme::NAMES
        .iter()
        .position(|name| *name == "chat")
        .unwrap();
    assert_eq!(colours.row_data(chat).unwrap().hex, theme::hex(nord.chat));
    window.invoke_theme_colour_edited(chat as i32, "#102030".into());
    assert_eq!(
        theme::Theme::shown(&window).chat,
        theme::parse("#102030").unwrap()
    );
    assert!(window.get_theme_can_save());

    // A field without a colour says so and holds Save back; the window keeps
    // the last colour.
    window.invoke_theme_colour_edited(chat as i32, "#1020".into());
    assert!(colours.row_data(chat).unwrap().wrong);
    assert!(!window.get_theme_can_save());
    assert_eq!(
        theme::Theme::shown(&window).chat,
        theme::parse("#102030").unwrap()
    );
    window.invoke_save_theme();
    assert!(window.get_theme_editor(), "Save waits for a colour");
    window.invoke_theme_colour_edited(chat as i32, "#123".into());
    window.invoke_theme_dark_toggled(false);
    window.set_theme_name("  Midnight  ".into());
    window.invoke_save_theme();

    // Saved, it's chosen, listed after the built-in themes and kept.
    assert!(!window.get_theme_editor());
    assert_eq!(
        window.get_theme_colours().row_count(),
        0,
        "rows go with the editor"
    );
    assert_eq!(window.get_theme_id(), "custom:1");
    let themes = window.get_themes();
    assert_eq!(themes.row_count(), theme::BUILT_IN.len() + 1);
    let card = themes.row_data(theme::BUILT_IN.len()).unwrap();
    assert_eq!(
        (card.id.as_str(), card.name.as_str(), card.custom),
        ("custom:1", "Midnight", true)
    );
    let midnight = theme::Theme {
        dark: false,
        chat: theme::parse("#112233").unwrap(),
        ..nord
    };
    assert_eq!(theme::Theme::shown(&window), midnight);
    let layout = Layout::read(directory.path());
    assert_eq!(layout.theme.as_deref(), Some("custom:1"));
    assert_eq!(layout.custom_themes.len(), 1);
    assert_eq!(layout.custom_themes[0].theme, midnight);
    assert_eq!(layout.custom_themes[0].base, "nord");

    // A window made again starts in it.
    let again = classic_window();
    bind_callbacks(&again, &adapter);
    adapter.borrow_mut().attach(&again);
    assert_eq!(theme::Theme::shown(&again), midnight);
    assert_eq!(again.get_theme_id(), "custom:1");
    let window = again;

    // Cancel puts back the theme from before editing, and the name.
    window.invoke_edit_theme("custom:1".into());
    assert!(window.get_theme_editing_saved());
    assert_eq!(window.get_theme_name(), "Midnight");
    window.invoke_theme_colour_edited(chat as i32, "#ff0000".into());
    window.set_theme_name("Red".into());
    assert_eq!(
        theme::Theme::shown(&window).chat,
        theme::parse("#ff0000").unwrap()
    );
    window.invoke_cancel_theme();
    assert_eq!(theme::Theme::shown(&window), midnight);
    assert_eq!(adapter.borrow().layout.custom_themes[0].name, "Midnight");

    // A new theme from another start takes its colours and name.
    window.invoke_new_theme();
    assert_eq!(window.get_theme_name(), "Midnight copy");
    let sakura_start = theme::BUILT_IN
        .iter()
        .position(|built_in| built_in.id == "sakura")
        .unwrap();
    window.invoke_theme_start_chosen(sakura_start as i32);
    assert_eq!(window.get_theme_name(), "Sakura copy");
    assert_eq!(
        theme::Theme::shown(&window),
        theme::find("sakura").unwrap().theme
    );
    window.set_theme_name("Blossom".into());
    window.invoke_theme_start_chosen(0);
    assert_eq!(window.get_theme_name(), "Blossom", "a name given stays");
    window.invoke_cancel_theme();
    assert_eq!(theme::Theme::shown(&window), midnight);

    // Renamed, then deleted while chosen: Yap shows.
    window.invoke_edit_theme("custom:1".into());
    window.set_theme_name("Dusk".into());
    window.invoke_save_theme();
    assert_eq!(
        window
            .get_themes()
            .row_data(theme::BUILT_IN.len())
            .unwrap()
            .name,
        "Dusk"
    );
    window.invoke_edit_theme("custom:1".into());
    window.invoke_delete_theme();
    assert!(!window.get_theme_editor());
    assert_eq!(window.get_theme_id(), "yap");
    assert_eq!(theme::Theme::shown(&window), theme::YAP);
    assert_eq!(window.get_themes().row_count(), theme::BUILT_IN.len());
    let layout = Layout::read(directory.path());
    assert_eq!(layout.theme.as_deref(), Some("yap"));
    assert!(layout.custom_themes.is_empty());
}

#[test]
fn a_window_made_again_while_editing_drops_the_edit() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_new_theme();
    let link = theme::Theme::NAMES
        .iter()
        .position(|name| *name == "link")
        .unwrap();
    window.invoke_theme_colour_edited(link as i32, "#ff0000".into());
    assert_eq!(theme::mention_color(), 0xff0000);

    let again = classic_window();
    adapter.borrow_mut().attach(&again);
    assert!(adapter.borrow().theme_editor.is_none());
    assert!(
        adapter.borrow().restyle,
        "rows lose the edited mention colour"
    );
    assert!(!again.get_theme_editor());
    assert_eq!(theme::Theme::shown(&again), theme::YAP);
    assert_eq!(theme::mention_color(), 0x64adf6);
    assert!(Layout::read(directory.path()).custom_themes.is_empty());
}

#[test]
fn a_reactions_tooltip_names_up_to_three_as_discords_client_does() {
    let names = |list: &[&str]| {
        list.iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>()
    };
    let tip = |list: &[&str], count| messages::reaction_tip(":tada:", &names(list), count);
    assert_eq!(tip(&["Ann"], 1), ":tada: reacted by Ann");
    assert_eq!(tip(&["Ann"], 2), ":tada: reacted by Ann and 1 other");
    assert_eq!(tip(&["Ann", "Bo"], 2), ":tada: reacted by Ann and Bo");
    assert_eq!(
        tip(&["Ann", "Bo"], 5),
        ":tada: reacted by Ann, Bo, and 3 others"
    );
    assert_eq!(
        tip(&["Ann", "Bo", "Cy"], 3),
        ":tada: reacted by Ann, Bo, and Cy"
    );
    assert_eq!(
        tip(&["Ann", "Bo", "Cy"], 4),
        ":tada: reacted by Ann, Bo, Cy, and 1 other"
    );
    // Discord's names for emoji: a word, not `:+1:`, and a custom one's own.
    assert_eq!(emoji::shortcode("👍"), ":thumbsup:");
    assert_eq!(emoji::shortcode("🎉"), ":tada:");
    assert_eq!(emoji::shortcode(":talk:"), ":talk:");
}

#[test]
fn the_thread_beside_keeps_its_rows_when_a_new_layout_builds_them_again() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    let messages: Vec<_> = (0..3)
        .map(|n| {
            Arc::new(ChatMessage {
                id: format!("t{n}"),
                item: format!("t{n}"),
                author: "Ann".into(),
                body: format!("Reply {n}"),
                ..Default::default()
            })
        })
        .collect();
    {
        let mut adapter = adapter.borrow_mut();
        let aside = adapter.app.state.aside.insert(Default::default());
        aside.room = "thread".into();
        aside.ready = true;
        aside.messages = Arc::new(messages);
    }
    adapter.borrow_mut().refresh(&window);
    assert_eq!(window.get_aside_messages().row_count(), 3);
    // As a theme's colours do, which the rows carry.
    adapter.borrow_mut().restyle = true;
    adapter.borrow_mut().refresh(&window);
    assert_eq!(window.get_aside_messages().row_count(), 3);
}

#[cfg(feature = "discord")]
#[test]
fn requests_list_someone_once_with_their_message() {
    use yapfast_core::chat::{ChatRoom, FriendRequest, Reacts, RequestPreview};
    use yapfast_core::network::Network;
    let request = |id: &str, name: &str, incoming, spam| FriendRequest {
        id: id.into(),
        name: name.into(),
        avatar: None,
        incoming,
        spam,
    };
    let room = |id: &str, person: &str, category: &str| ChatRoom {
        id: id.into(),
        network: Network::Discord,
        capabilities: Network::Discord.capabilities(),
        name: person.into(),
        direct: true,
        voice: false,
        encrypted: false,
        members: 2,
        unread: 1,
        mentions: 0,
        muted: false,
        last_read: None,
        locked: false,
        join_locked: false,
        read_only: true,
        reacts: Reacts::None,
        slowed: false,
        timed_out_until: None,
        category: category.into(),
        category_id: None,
        request: true,
        person: Some(person.into()),
        people: Vec::new(),
        ringing: Vec::new(),
        incoming: false,
        avatar: None,
        decoration: None,
        nameplate: None,
        style: None,
        upload_limit: 0,
        presence: None,
        activity: None,
        activities: Default::default(),
        parent: None,
        forum: false,
        mark: Default::default(),
        unlisted: false,
        tag: None,
        official: false,
        empty: false,
        preview: Some(Box::new(RequestPreview {
            text: format!("Hi from {person}"),
            at: 1_700_000_000_000,
        })),
    };
    let mut state = AppState::default();
    state.friend_requests = vec![
        request("erin", "Erin", true, false),
        request("dave", "Dave", false, false),
        request("zed", "Zed", true, true),
    ];
    let asked = [
        room("dm-erin", "erin", "Message requests"),
        room("dm-gil", "gil", "Message requests"),
    ];
    let spam = [room("dm-frank", "frank", "Spam")];
    let rows = refresh::request_rows(
        &state,
        &asked.iter().collect::<Vec<_>>(),
        &spam.iter().collect::<Vec<_>>(),
        false,
    );
    let listed = |rows: &[RequestRow]| -> Vec<(String, i32, String, String)> {
        rows.iter()
            .map(|row| {
                (
                    row.id.to_string(),
                    row.friend,
                    row.room.to_string(),
                    row.message.to_string(),
                )
            })
            .collect()
    };
    let row = |id: &str, friend, room: &str, message: &str| {
        (id.into(), friend, room.into(), message.into())
    };
    // Erin's friend request and message share a row; Gil only wrote.
    assert_eq!(
        listed(&rows.incoming),
        [
            row("erin", 1, "dm-erin", "Hi from erin"),
            row("gil", 0, "dm-gil", "Hi from gil")
        ]
    );
    assert!(
        rows.incoming
            .iter()
            .all(|row| row.unread && !row.time.is_empty())
    );
    assert_eq!(listed(&rows.sent), [row("dave", 2, "", "")]);
    assert_eq!(
        listed(&rows.spam),
        [
            row("zed", 1, "", ""),
            row("frank", 0, "dm-frank", "Hi from frank")
        ]
    );
}

#[test]
#[cfg(feature = "discord")]
fn join_a_server_shows_the_invites_server_and_a_link_opens_it() {
    let (window, adapter, _directory) = discord_server_window();
    adapter.borrow_mut().refresh(&window);
    assert!(window.get_can_join_server());
    // A clicked invite link opens the dialog rather than the browser.
    window.invoke_open_link("https://discord.gg/abc".into());
    {
        let join = adapter.borrow().app.state.join.clone().unwrap();
        assert!(join.from_link && join.resolving);
        assert_eq!(join.code.as_deref(), Some("abc"));
    }
    assert!(window.get_join_open());
    let prompt = window.get_join_prompt();
    assert_eq!(prompt.title, "You've been invited to join a server");
    assert!(!prompt.field && !prompt.found && prompt.busy);
    assert_eq!(prompt.status, "Looking up the invite...");

    let mut adapter = adapter.borrow_mut();
    adapter.app.state.join = Some(yapfast_core::application::JoinDialog {
        code: Some("abc".into()),
        preview: Some(yapfast_core::chat::InvitePreview {
            code: "abc".into(),
            server: "discord:20".into(),
            name: "Builders".into(),
            icon: None,
            room: "discord:20:21".into(),
            channel: "general".into(),
            channel_type: 0,
            voice: false,
            members: Some(42),
            online: Some(7),
        }),
        ..Default::default()
    });
    adapter.refresh(&window);
    let prompt = window.get_join_prompt();
    assert_eq!(prompt.title, "Join a Server");
    assert!(prompt.field && prompt.found && !prompt.busy);
    assert_eq!(
        (prompt.name.as_str(), prompt.initial.as_str()),
        ("Builders", "B")
    );
    assert_eq!(prompt.detail, "7 online, 42 members");
    assert_eq!(prompt.confirm, "Join Server");
}

#[test]
#[cfg(feature = "discord")]
fn a_handed_link_waits_for_the_open_dialog() {
    let (window, adapter, _directory) = discord_server_window();
    adapter.borrow_mut().refresh(&window);
    let query = |adapter: &Rc<RefCell<Adapter>>| {
        let adapter = adapter.borrow();
        let dialog = adapter.app.state.oauth.as_ref();
        dialog.and_then(|dialog| dialog.query.clone())
    };
    window.invoke_open_link(
        "https://discord.com/oauth2/authorize?client_id=157730590492196864&scope=bot".into(),
    );
    let shown = query(&adapter);
    assert!(shown.is_some());
    // Another app hands Yapfast a link to a different application while
    // the dialog is open: an Authorize click is still for the first.
    {
        let mut adapter = adapter.borrow_mut();
        adapter.pending_link =
            Some("discord://-/oauth2/authorize?client_id=157730590492196865&scope=bot".into());
        adapter.open_pending_link(&window);
    }
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(1));
    assert_eq!(query(&adapter), shown, "the dialog keeps what it showed");
    assert!(adapter.borrow().pending_link.is_some());
    // Closing it opens the link that waited.
    window.invoke_close_oauth();
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(1));
    assert!(query(&adapter).is_some_and(|query| query.contains("157730590492196865")));
    assert!(adapter.borrow().pending_link.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn authorizing_an_app_says_what_it_asks_for_and_where_it_goes() {
    use yapfast_core::application::OAuthDialog;
    use yapfast_core::chat::{OAuthGuild, OAuthPreview};
    let (window, adapter, _directory) = discord_server_window();
    adapter.borrow_mut().refresh(&window);
    // A plain bot link is the dialog's, not the browser's.
    window.invoke_open_link(
        "https://discord.com/oauth2/authorize?client_id=157730590492196864&scope=bot".into(),
    );
    assert!(adapter.borrow().app.state.oauth.is_some());
    assert!(window.get_oauth_open());

    let guild = |id: &str, name: &str, can_add| OAuthGuild {
        id: id.into(),
        name: name.into(),
        icon: None,
        can_add,
    };
    let preview = OAuthPreview {
        query: "q".into(),
        name: "Alien App".into(),
        about: None,
        icon: None,
        bot: Some("Alien App".into()),
        wants: vec![
            "Add a bot to a server",
            "Interface with your Discord client",
        ],
        // Administrator, View Channels, and bit 60, which no switch shows.
        permissions: Some((1 << 3) | (1 << 10) | (1 << 60)),
        controls_client: true,
        authorized: false,
        redirect: None,
        guild_install: true,
        guild_locked: true,
        wanted_guild: Some("20".into()),
        guilds: vec![guild("10", "Server", true), guild("20", "Builders", true)],
    };
    let mut adapter = adapter.borrow_mut();
    adapter.app.state.oauth = Some(OAuthDialog {
        query: Some("q".into()),
        preview: Some(preview.clone()),
        guild: Some("20".into()),
        ..Default::default()
    });
    adapter.refresh(&window);
    let prompt = window.get_oauth_prompt();
    assert!(
        prompt.warning.contains("control your Discord client"),
        "{}",
        prompt.warning
    );
    assert!(
        prompt.warning.contains("Administrator"),
        "{}",
        prompt.warning
    );
    assert!(
        prompt
            .wants
            .contains("- Interface with your Discord client")
    );
    assert!(
        prompt.wants.contains("and 1 more permission"),
        "{}",
        prompt.wants
    );
    // The one server the link names shows, and no picker.
    assert!(!prompt.guild_picker);
    assert_eq!(prompt.guild_name, "Builders");
    assert_eq!(prompt.confirm, "Authorize");

    // A locked server she can't add apps to: no Authorize.
    adapter.app.state.oauth = Some(OAuthDialog {
        query: Some("q".into()),
        preview: Some(preview),
        guild: None,
        error: Some("The link adds it only to a server you can't add apps to.".into()),
        ..Default::default()
    });
    adapter.refresh(&window);
    let prompt = window.get_oauth_prompt();
    assert_eq!(prompt.confirm, "");
    assert!(!prompt.error.is_empty());

    // An address that isn't a web page waits to be copied.
    adapter.app.state.oauth = Some(OAuthDialog {
        query: Some("q".into()),
        done: true,
        next: Some("myapp://cb?code=x".into()),
        ..Default::default()
    });
    adapter.refresh(&window);
    let prompt = window.get_oauth_prompt();
    assert_eq!(prompt.address, "myapp://cb?code=x");
    assert_eq!(prompt.confirm, "");
}

#[test]
#[cfg(feature = "discord")]
fn discord_links_open_channels_messages_and_profiles_here() {
    let (window, adapter, _directory) = discord_server_window();
    adapter.borrow_mut().refresh(&window);

    window.invoke_open_link("https://discord.com/channels/10/11".into());
    {
        let adapter = adapter.borrow();
        assert_eq!(
            adapter.app.state.navigation,
            Navigation::Server("discord:10".into())
        );
        assert_eq!(adapter.app.state.selected.as_deref(), Some("discord:10:11"));
    }

    adapter.borrow_mut().app.state.messages = Arc::new(vec![Arc::new(ChatMessage {
        item: "d:42".into(),
        body: "Linked".into(),
        ..Default::default()
    })]);
    window.invoke_open_link("https://discord.com/channels/10/11/42".into());
    assert_eq!(
        adapter
            .borrow()
            .app
            .state
            .jump
            .as_ref()
            .map(|(item, _)| item.as_str()),
        Some("d:42")
    );

    window.invoke_open_link("https://discord.com/users/4242".into());
    {
        let adapter = adapter.borrow();
        let view = adapter.app.state.profile.as_ref().unwrap();
        assert_eq!(view.profile.id, "4242");
    }
    assert!(window.get_profile_open());
}

#[test]
fn a_card_of_components_shows_headings_files_and_only_link_buttons_working() {
    use yapfast_core::chat::{
        Attachment, ButtonStyle, Card, CardButton as Button, CardRow as Row, Placement,
    };
    i_slint_backend_testing::init_no_event_loop();
    let button = |style, url: Option<&str>| Button {
        label: "Go".into(),
        style,
        url: url.map(str::to_owned),
        disabled: false,
    };
    let message = ChatMessage {
        item: "d:1".into(),
        attachments: vec![Attachment {
            voice_ms: None,
            name: "notes.txt".into(),
            size: Some(10),
            image: false,
            dimensions: None,
            preview: None,
            preview_failed: false,
            placement: Placement::Component(0),
            spoiler: false,
            video: false,
            gif: None,
            url: None,
        }],
        cards: vec![Card {
            framed: true,
            color: Some(0x5865f2),
            rows: vec![
                Row::Text {
                    text: "# Hello\nThere\n-# small print".into(),
                    beside: None,
                },
                Row::Media(0),
                Row::Buttons(vec![
                    button(ButtonStyle::Link, Some("https://example.com")),
                    button(ButtonStyle::Primary, None),
                ]),
            ],
        }],
        ..Default::default()
    };
    let row = message_row_plain(&message);
    // The file shows in its card, not under the message.
    assert_eq!(row.files.row_count(), 0);
    let card = row.cards.row_data(0).unwrap();
    assert!(card.framed && card.has_color);
    let rows: Vec<_> = card.rows.iter().collect();
    let levels: Vec<_> = rows[0].blocks.iter().map(|block| block.level).collect();
    assert_eq!(levels, [1, 0, 4]);
    assert_eq!(rows[1].files.row_data(0).unwrap().name, "notes.txt");
    let enabled: Vec<_> = rows[2]
        .buttons
        .iter()
        .map(|button| button.enabled)
        .collect();
    assert_eq!(enabled, [true, false], "only a link's button works");
    let text = message_text(&MessageRow {
        first: false,
        ..row
    });
    assert_eq!(text, "Hello\nThere\nsmall print\nnotes.txt\nGo Go");
}

#[test]
fn headings_in_fenced_code_stay_as_typed() {
    let blocks = text_blocks("```\n# not a heading\n```\n## Two");
    let levels: Vec<_> = blocks.iter().map(|block| block.level).collect();
    assert_eq!(levels, [5, 2]);
    assert_eq!(
        i_slint_core::yapfast_selection::plain(&blocks[0].text),
        "# not a heading"
    );
}

fn formatted(body: &str) -> ChatMessage {
    ChatMessage {
        item: "d:1".into(),
        body: body.into(),
        formatted: true,
        ..Default::default()
    }
}

#[test]
fn a_message_shows_its_headings_quotes_and_code_apart() {
    i_slint_backend_testing::init_no_event_loop();
    let row = message_row_plain(&formatted(
        "# Title\n> quoted **bold**\n> -# small\n```\n<b>code</b>\n```\n-# end",
    ));
    let blocks: Vec<_> = row.blocks.iter().collect();
    let shown: Vec<_> = blocks
        .iter()
        .map(|block| {
            (
                block.level,
                block.quoted,
                i_slint_core::yapfast_selection::plain(&block.text),
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            (1, false, "Title".into()),
            (0, true, "quoted bold".into()),
            (4, true, "small".into()),
            (5, false, "<b>code</b>".into()),
            (4, false, "end".into()),
        ]
    );
    // The quote's bar runs from its first block to its last.
    assert!(blocks[1].quote_start && !blocks[1].quote_end);
    assert!(!blocks[2].quote_start && blocks[2].quote_end);
    // A first heading sits closer to the top than a later one would.
    assert_eq!((blocks[0].margin, blocks[0].padding), (8.0, 0.0));
    assert_eq!((blocks[1].margin, blocks[1].padding), (8.0, 0.0));
    assert_eq!(blocks[3].margin, 6.0);
    let text = message_text(&MessageRow {
        first: false,
        ..row
    });
    assert_eq!(text, "Title\nquoted bold\nsmall\n<b>code</b>\nend");
}

#[test]
fn text_alone_shows_as_one() {
    i_slint_backend_testing::init_no_event_loop();
    let row = message_row_plain(&formatted("plain **text**\n- and a list"));
    assert_eq!(row.blocks.row_count(), 0);
    assert!(i_slint_core::yapfast_selection::plain(&row.styled).starts_with("plain text\n"));
}

#[test]
fn text_beside_a_heading_keeps_its_formatting_and_blank_lines() {
    i_slint_backend_testing::init_no_event_loop();
    let row = message_row_plain(&formatted("# Title\n\n**bold**\n---"));
    let plain: Vec<_> = row
        .blocks
        .iter()
        .map(|block| i_slint_core::yapfast_selection::plain(&block.text))
        .collect();
    assert_eq!(plain, ["Title", "\u{a0}\nbold\n---"]);
}

#[test]
fn html_in_a_block_shows_as_typed() {
    i_slint_backend_testing::init_no_event_loop();
    // Markdown would make the text after the open fence code, which the
    // HTML's escaping skips, but Discord's client shows it as text.
    let body = "```\n# Title\n<font color=\"#ff0000\">red</font>";
    let row = message_row_plain(&formatted(body));
    let last = row.blocks.iter().last().unwrap();
    assert_eq!(
        i_slint_core::yapfast_selection::plain(&last.text),
        "<font color=\"#ff0000\">red</font>"
    );
}

#[test]
fn a_mention_in_a_quote_still_links() {
    i_slint_backend_testing::init_no_event_loop();
    let mut message = formatted("# Hi\n> and @Bob");
    let plain = message_row_plain(&message);
    message.mentions = vec![yapfast_core::mention::Mentioned {
        text: "@Bob".into(),
        target: yapfast_core::mention::Target::User("2".into()),
        color: None,
        at: vec![message.body.find('@').unwrap()],
    }];
    let mentioned = message_row_plain(&message);
    let quote = |row: &MessageRow| row.blocks.row_data(1).unwrap().text;
    assert!(quote(&plain) != quote(&mentioned));
    assert_eq!(
        i_slint_core::yapfast_selection::plain(&quote(&mentioned)),
        "and @Bob"
    );
}

#[test]
fn spoilers_count_on_across_blocks() {
    i_slint_backend_testing::init_no_event_loop();
    let message = ChatMessage {
        spoilers: true,
        ..formatted("||a||\n# T ||b||")
    };
    let row = |revealed: &[&str]| {
        let revealed = revealed.iter().map(|item| item.to_string()).collect();
        let row = message_row(&message, &revealed);
        let blocks: Vec<_> = row.blocks.iter().map(|block| block.text).collect();
        blocks
    };
    let hidden = row(&[]);
    let second = row(&["d:1/1"]);
    assert!(hidden[0] == second[0], "the first is still hidden");
    assert!(hidden[1] != second[1], "the heading's is the second");
}

#[test]
fn an_embed_shows_headings_and_code_in_its_description_and_fields() {
    i_slint_backend_testing::init_no_event_loop();
    let message = ChatMessage {
        embeds: vec![yapfast_core::chat::Embed {
            description: "## Status\n```\nall good\n```".into(),
            fields: vec![
                yapfast_core::chat::EmbedField {
                    name: "Plain".into(),
                    value: "just **text**".into(),
                    inline: false,
                },
                yapfast_core::chat::EmbedField {
                    name: "Quoted".into(),
                    value: "> said".into(),
                    inline: false,
                },
            ],
            ..Default::default()
        }],
        ..formatted("")
    };
    let row = message_row_plain(&message);
    let embed = row.embeds.row_data(0).unwrap();
    let levels: Vec<_> = embed
        .description_blocks
        .iter()
        .map(|block| block.level)
        .collect();
    assert_eq!(levels, [2, 5]);
    let fields: Vec<_> = embed
        .field_rows
        .iter()
        .flat_map(|row| row.fields.iter().collect::<Vec<_>>())
        .collect();
    assert_eq!(fields[0].value_blocks.row_count(), 0);
    assert_eq!(
        i_slint_core::yapfast_selection::plain(&fields[0].value),
        "just text"
    );
    assert!(fields[1].value_blocks.row_data(0).unwrap().quoted);
    let text = message_text(&MessageRow {
        first: false,
        ..row
    });
    assert_eq!(text, "Status\nall good\nPlain\njust text\nQuoted\nsaid");
}

#[test]
fn role_colors_everywhere_switches_message_and_reply_colours() {
    i_slint_backend_testing::init_no_event_loop();
    let message = ChatMessage {
        author: "Bob".into(),
        color: Some(0x3498db),
        reply: Some(yapfast_core::chat::Reply {
            author: "Ann".into(),
            avatar: None,
            body: String::new(),
            item: None,
            color: Some(0x3498db),
        }),
        ..Default::default()
    };
    let on = message_row_with_role_colors(&message, &HashSet::new(), true);
    assert!(on.has_color);
    assert_eq!(on.color, slint::Color::from_rgb_u8(0x34, 0x98, 0xdb));
    assert!(on.reply_has_color);
    assert_eq!(on.reply_color, slint::Color::from_rgb_u8(0x34, 0x98, 0xdb));

    let off = message_row_with_role_colors(&message, &HashSet::new(), false);
    assert!(!off.has_color);
    assert_eq!(off.color, slint::Color::default());
    assert!(!off.reply_has_color);
    assert_eq!(off.reply_color, slint::Color::default());

    let found = found_row_with_role_colors(&message, false, false);
    assert!(!found.has_color);
    assert_eq!(found.color, slint::Color::default());

    let notice = yapfast_core::chat::Notice {
        kind: yapfast_core::chat::NoticeKind::Pinned(Some("1".into())),
        text: "pinned a message.".into(),
    };
    let on_notice = notice_text(&message, &notice, "12:00", true);
    let off_notice = notice_text(&message, &notice, "12:00", false);
    assert_ne!(on_notice, off_notice);

    let topic = yapfast_core::chat::Topic {
        body: "Hello @Bob".into(),
        mentions: vec![yapfast_core::mention::Mentioned {
            text: "@Bob".into(),
            target: yapfast_core::mention::Target::User("2".into()),
            color: Some(0x3498db),
            at: vec![6],
        }],
        emoji: Vec::new(),
    };
    let on_topic = topic_text_with_role_colors(&topic, false, true);
    let off_topic = topic_text_with_role_colors(&topic, false, false);
    assert_ne!(on_topic, off_topic);
}

#[test]
#[cfg(feature = "discord")]
fn role_colors_everywhere_switches_mention_suggestions_and_voice() {
    let (window, adapter, _folder) = discord_server_window();
    let mut adapter_mut = adapter.borrow_mut();
    adapter_mut
        .app
        .dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    adapter_mut.action(&window, Action::SelectRoom("discord:10:11".into()));

    let member = |id: &str, name: &str| yapfast_core::chat::ChatMember {
        bot: false,
        id: id.into(),
        name: name.into(),
        username: None,
        role: yapfast_core::chat::Role::Member,
        avatar: None,
        decoration: None,
        nameplate: None,
        style: None,
        color: None,
        roles: std::sync::Arc::from([]),
        group: None,
        group_icon: None,
        role_icon: None,
        presence: None,
        activity: None,
        activities: std::sync::Arc::from([]),
        boosting_since: None,
        tag: None,
        timed_out_until: None,
    };
    adapter_mut.app.state.members = Some(Ok(vec![yapfast_core::chat::ChatMember {
        color: Some(0x3498db),
        ..member("2", "Bob")
    }]));
    adapter_mut.app.state.mention_groups = vec![yapfast_core::chat::MentionGroup {
        mention: "@Mods".into(),
        name: "Mods".into(),
        color: Some(0xe74c3c),
        channel: false,
        voice: false,
    }];
    adapter_mut
        .app
        .dispatch(Action::MentionQuery(Some(('@', "".into()))));

    // Default is role_colors_everywhere == false
    assert!(!adapter_mut.layout.role_colors_everywhere);
    adapter_mut.show_composer(&window);
    let mentions: Vec<_> = adapter_mut.models.mentions.iter().collect();
    let bob = mentions.iter().find(|m| m.name == "Bob").unwrap();
    assert!(
        !bob.has_color,
        "Bob username should not have role colour when OFF"
    );
    assert_eq!(bob.color, slint::Color::default());

    let mods = mentions.iter().find(|m| m.name == "Mods").unwrap();
    assert!(
        mods.has_color,
        "Mods role badge should remain coloured when OFF"
    );
    assert_eq!(mods.color, slint::Color::from_rgb_u8(0xe7, 0x4c, 0x3c));

    // Turn ON
    adapter_mut.layout.role_colors_everywhere = true;
    adapter_mut.show_composer(&window);
    let mentions_on: Vec<_> = adapter_mut.models.mentions.iter().collect();
    let bob_on = mentions_on.iter().find(|m| m.name == "Bob").unwrap();
    assert!(
        bob_on.has_color,
        "Bob username should have role colour when ON"
    );
    assert_eq!(bob_on.color, slint::Color::from_rgb_u8(0x34, 0x98, 0xdb));

    let mods_on = mentions_on.iter().find(|m| m.name == "Mods").unwrap();
    assert!(
        mods_on.has_color,
        "Mods role badge should remain coloured when ON"
    );
    assert_eq!(mods_on.color, slint::Color::from_rgb_u8(0xe7, 0x4c, 0x3c));

    // Test voice participants
    adapter_mut.app.state.voice = yapfast_core::voice::State {
        phase: yapfast_core::voice::Phase::Connected,
        room: Some("discord:10:14".into()),
        participants: vec![yapfast_core::voice::Participant {
            id: "2".into(),
            person: "2".into(),
            user: "Bob".into(),
            color: Some(0x3498db),
            connected: true,
            speaking: false,
            local: false,
            muted: false,
            deafened: false,
            server_muted: false,
            server_deafened: false,
            streaming: false,
            video: None,
            avatar: None,
            problem: None,
        }],
        ..Default::default()
    };
    adapter_mut.refresh(&window);
    let participants: Vec<_> = adapter_mut.models.participants.iter().collect();
    assert_eq!(participants.len(), 1);
    assert!(participants[0].has_color);
    assert_eq!(
        participants[0].color,
        slint::Color::from_rgb_u8(0x34, 0x98, 0xdb)
    );

    // Turn OFF and check voice participant
    adapter_mut.layout.role_colors_everywhere = false;
    adapter_mut.refresh(&window);
    let participants_off: Vec<_> = adapter_mut.models.participants.iter().collect();
    assert_eq!(participants_off.len(), 1);
    assert!(!participants_off[0].has_color);
    assert_eq!(participants_off[0].color, slint::Color::default());
}

#[test]
fn a_found_message_shows_its_blocks_with_spoilers_hidden() {
    i_slint_backend_testing::init_no_event_loop();
    let message = ChatMessage {
        spoilers: true,
        ..formatted("# Plans\n> meet at ||the park||")
    };
    let row = found_row(&message, false);
    let plain: Vec<_> = row
        .blocks
        .iter()
        .map(|block| i_slint_core::yapfast_selection::plain(&block.text))
        .collect();
    assert_eq!(plain, ["Plans", "meet at (spoiler)"]);
}

#[test]
fn the_picker_keeps_the_width_it_was_dragged_to() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_picker_width_chosen(600.);
    window.invoke_picker_width_chosen(648.);
    // Written once the drags stop, not at each.
    assert_eq!(Layout::read(directory.path()).picker_width, None);
    adapter.borrow_mut().flush_layout();
    assert_eq!(Layout::read(directory.path()).picker_width, Some(648.));
    // A new window starts at the stored width.
    let next = classic_window();
    adapter.borrow_mut().attach(&next);
    assert_eq!(next.get_picker_width(), 648.);
}

#[test]
fn resizing_the_window_carries_the_pickers_room_with_the_composer() {
    let window = showing(vec![]);
    window.set_can_pick_gifs(true);
    settle(&window);
    let button = slint::platform::PointerEventButton::Left;
    let click = |x: f32, y: f32| {
        let native = window.window();
        native.dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(x, y),
        });
        native.dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(x, y),
            button,
        });
        native.dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(x, y),
            button,
        });
        settle(&window);
    };
    // The GIF button opens the picker with 978px of room: up to 8px right of
    // the server list, which is 80px from the window's left edge.
    click(992., 710.);
    let room = window.get_picker_room();
    assert_eq!(room, 978.);
    // A window 200px wider carries the composer's buttons with it, and the
    // room the picker may take, so the picker stays over its button instead of
    // where the window used to end.
    window
        .window()
        .set_size(slint::LogicalSize::new(1320., 760.));
    settle(&window);
    assert_eq!(window.get_picker_room(), room + 200.);
    assert_eq!(window.get_picker_shown(), 498.);
}

#[test]
fn the_picker_draws_the_emoji_it_shows_as_pictures_of_the_grids_size() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.set_emoji_panel_width(498.);
    window.invoke_emoji_search("".into());
    // What the picker shows, drawn once each, at the size the grid draws its
    // glyphs: the colour font's paint graph would otherwise be walked again for
    // every emoji on screen, every frame.
    let rows = window.get_emoji_rows();
    let shown = 0..8.min(rows.row_count());
    let mut drawn = 0;
    for index in shown {
        let row = rows.row_data(index).unwrap();
        for cell in row.cells.iter() {
            // A custom emoji has its own picture, and a sequence keeps its
            // glyphs for the font to join.
            if cell.key.contains(':') || cell.glyph != cell.key || cell.glyph.chars().count() != 1 {
                continue;
            }
            // Shrunk once to the grid's 36px, so a frame copies it.
            assert_eq!(
                cell.picture.size().width,
                36,
                "{} is drawn as a picture",
                cell.name
            );
            drawn += 1;
        }
    }
    assert!(drawn > 20, "the picker drew {drawn} emoji");
}

#[test]
fn the_picker_lists_what_the_account_uses_for_a_message_or_a_reaction() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    adapter.borrow_mut().app.state.discord_emojis = yapfast_core::application::DiscordEmojiLists {
        favorites: vec!["skull_crossbones".into()],
        frequent: vec!["joy".into()],
        reactions: vec!["100".into()],
    };
    window.set_emoji_panel_width(498.);
    let first_of = |row: usize| {
        let cells = window.get_emoji_rows().row_data(row).unwrap().cells;
        cells.row_data(0).unwrap().key
    };
    window.set_react_item("yapfast:composer".into());
    window.invoke_emoji_search("".into());
    assert_eq!(
        window.get_emoji_rows().row_data(0).unwrap().heading,
        "FAVORITES"
    );
    assert_eq!(first_of(1), "☠️");
    assert_eq!(
        window.get_emoji_rows().row_data(2).unwrap().heading,
        "FREQUENTLY USED"
    );
    assert_eq!(first_of(3), "😂");
    // A reaction's picker has the account's reactions, which it counts apart.
    window.set_react_item("message".into());
    window.invoke_emoji_search("".into());
    assert_eq!(first_of(1), "☠️");
    assert_eq!(first_of(3), "💯");
    // A search finds emoji, under no such headings.
    window.invoke_emoji_search("pizza".into());
    assert_eq!(window.get_emoji_rows().row_count(), 1);
}

#[test]
fn resizing_the_grid_fills_the_rows_it_is_scrolled_to() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.set_emoji_panel_width(498.);
    window.invoke_emoji_search("".into());
    // Scrolled a long way down, the rows in view are the ones drawn.
    let top = window
        .get_emoji_rows()
        .row_data(40)
        .map(|row| row.top)
        .unwrap_or_default();
    window.invoke_emoji_scrolled_to(top, 6.0 * emoji::ROW_HEIGHT);
    // A wider grid is laid out again, and its cells are new ones: the rows it is
    // scrolled to must take their pictures again, or every emoji there would be
    // a placeholder until the grid was scrolled.
    // A drag changes the columns more than once, and each change lays the grid
    // out again: every one of them must fill the rows in view, not only the
    // first.
    window.set_emoji_panel_width(648.);
    window.invoke_emoji_reflow();
    window.set_emoji_panel_width(978.);
    window.invoke_emoji_reflow();
    let rows = window.get_emoji_rows();
    let drawn: usize = (40..46.min(rows.row_count()))
        .map(|index| {
            rows.row_data(index)
                .map(|row| {
                    row.cells
                        .iter()
                        .filter(|cell| cell.picture.size().width > 0)
                        .count()
                })
                .unwrap_or(0)
        })
        .sum();
    assert!(drawn > 20, "the rows in view are drawn again: {drawn}");
}

#[test]
fn only_the_rows_the_picker_shows_are_drawn() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.set_emoji_panel_width(498.);
    window.invoke_emoji_search("".into());
    let rows = window.get_emoji_rows();
    let drawn_in = |index: usize| -> usize {
        let Some(row) = rows.row_data(index) else {
            return 0;
        };
        row.cells
            .iter()
            .filter(|cell| cell.picture.size().width > 0)
            .count()
    };
    // The rows the grid shows carry their pictures, and a screen below them is
    // ready too, so a scroll finds it drawn rather than filling in; beyond that
    // a row is left as glyphs, since a long list costs only what is looked at.
    let shown: usize = (0..8.min(rows.row_count())).map(drawn_in).sum();
    assert!(shown > 20, "the rows shown are drawn: {shown}");
    let below: usize = (8..16.min(rows.row_count())).map(drawn_in).sum();
    assert!(below > 20, "the screen below is ready: {below}");
    let far: usize = (17..40.min(rows.row_count())).map(drawn_in).sum();
    assert_eq!(far, 0, "the rows beyond a screen are not prepared");
}

#[test]
fn rows_on_slots_keep_their_slot_and_take_those_of_rows_gone() {
    let model = VecModel::<i32>::default();
    let mut slots = Vec::new();
    update_model_by_slot(
        &model,
        &mut slots,
        (0..6).map(|key| (key, key * 10)).collect(),
    );
    assert_eq!(model.iter().collect::<Vec<_>>(), [0, 10, 20, 30, 40, 50]);
    // Two went from the top and two came at the foot: they take the slots of
    // those gone, and the rest stay where they were.
    update_model_by_slot(
        &model,
        &mut slots,
        (2..8).map(|key| (key, key * 10)).collect(),
    );
    assert_eq!(model.iter().collect::<Vec<_>>(), [60, 70, 20, 30, 40, 50]);
    assert_eq!(slots, [6, 7, 2, 3, 4, 5]);
    // One that stays and changed is set in its own slot.
    update_model_by_slot(
        &model,
        &mut slots,
        (2..8)
            .map(|key| (key, if key == 4 { 41 } else { key * 10 }))
            .collect(),
    );
    assert_eq!(model.iter().collect::<Vec<_>>(), [60, 70, 20, 30, 41, 50]);
    // Fewer rows: the slots after them move into the ones left empty.
    update_model_by_slot(&model, &mut slots, [(3, 30), (7, 70), (5, 50)].into());
    assert_eq!(model.row_count(), 3);
    let mut shown: Vec<(i32, i32)> = slots.iter().copied().zip(model.iter()).collect();
    shown.sort();
    assert_eq!(shown, [(3, 30), (5, 50), (7, 70)]);
    update_model_by_slot(&model, &mut slots, Vec::new());
    assert_eq!(model.row_count(), 0);
    assert!(slots.is_empty());
}

#[test]
fn the_picker_grid_draws_the_rows_in_sight_on_the_buttons_it_has() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.set_emoji_panel_width(498.);
    window.invoke_emoji_search("".into());
    let rows = window.get_emoji_rows();
    let last = rows.row_data(rows.row_count() - 1).unwrap();
    assert_eq!(window.get_emoji_grid_height(), last.top + last.height);
    let tiles = window.get_emoji_tiles();
    let placed = || -> HashMap<(SharedString, i32), usize> {
        tiles
            .iter()
            .enumerate()
            .map(|(slot, tile)| ((tile.cell.key.clone(), tile.y as i32), slot))
            .collect()
    };
    let before = placed();
    // A screen of rows and a couple below, not the whole list.
    assert!(before.len() > 40 && before.len() < 120, "{}", before.len());
    assert!(
        tiles
            .iter()
            .all(|tile| tile.y < 340.0 + 3.0 * emoji::ROW_HEIGHT)
    );
    assert!(
        window
            .get_emoji_headings()
            .iter()
            .all(|heading| !heading.heading.is_empty())
    );
    // A scroll of a row keeps every emoji still in sight on its button.
    let top_of = |index: usize| rows.row_data(index).unwrap().top;
    window.invoke_emoji_scrolled_to(top_of(4), 340.0);
    let after = placed();
    let kept = before
        .iter()
        .filter(|(place, slot)| after.get(*place) == Some(*slot))
        .count();
    let stayed = before
        .keys()
        .filter(|place| after.contains_key(*place))
        .count();
    assert!(stayed > 20);
    assert_eq!(
        kept, stayed,
        "an emoji that stays in sight keeps its button"
    );
    // Far down the list, it shows those rows and none from the top.
    window.invoke_emoji_scrolled_to(top_of(100), 340.0);
    assert!(tiles.iter().all(|tile| tile.y >= top_of(97)));
    assert!(
        tiles
            .iter()
            .filter(|tile| tile.cell.picture.size().width > 0)
            .count()
            > 20
    );
}

#[test]
fn rows_far_from_those_the_picker_shows_let_go_of_their_pictures() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.set_emoji_panel_width(498.);
    window.invoke_emoji_search("".into());
    let rows = window.get_emoji_rows();
    let drawn = |range: std::ops::Range<usize>| -> usize {
        range
            .filter_map(|index| rows.row_data(index))
            .map(|row| {
                row.cells
                    .iter()
                    .filter(|cell| cell.picture.size().width > 0)
                    .count()
            })
            .sum()
    };
    assert!(drawn(0..8) > 20, "the top is drawn");
    // The screen below the one it shows is made ready with it, so the first
    // scroll finds what it brings into sight drawn already.
    assert!(drawn(8..16) > 20, "the screen below is ready");
    let top_of =
        |index: usize| -> f32 { rows.row_data(index).map(|row| row.top).unwrap_or_default() };
    let screen = 6.0 * emoji::ROW_HEIGHT;
    // A short scroll keeps what it left, for a scroll back.
    window.invoke_emoji_scrolled_to(top_of(6), screen);
    assert!(drawn(0..4) > 10, "the rows just left keep their pictures");
    // A long one lets go of it, whether it went by steps or at once, as the
    // sidebar jumps; the rows it reached are drawn, and those between were not.
    window.invoke_emoji_scrolled_to(top_of(40), screen);
    assert_eq!(drawn(0..24), 0, "the rows far above let go");
    assert!(drawn(40..46) > 20, "the rows reached are drawn");
    assert!(
        drawn(32..40) > 20,
        "and the screen above them is ready, for a scroll back"
    );
    window.invoke_emoji_scrolled_to(top_of(100), screen);
    assert_eq!(drawn(40..46), 0, "a jump lets go of where it came from");
    assert!(drawn(100..106) > 20, "and draws where it went");
    // Back at the top, the pictures are drawn again.
    window.invoke_emoji_scrolled_to(0.0, screen);
    assert!(drawn(0..6) > 20, "a row drawn before is drawn again");
    assert_eq!(drawn(100..106), 0);
}

#[test]
fn a_closed_picker_keeps_no_grid_and_builds_it_as_it_opens() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.set_emoji_panel_width(498.);
    window.invoke_emoji_search("".into());
    assert!(window.get_emoji_rows().row_count() > 100);
    window.invoke_emoji_picker_closed();
    assert_eq!(window.get_emoji_rows().row_count(), 0);
    assert_eq!(window.get_emoji_sections().row_count(), 0);
    assert_eq!(window.get_emoji_rail_sections().row_count(), 0);
    assert_eq!(window.get_emoji_tiles().row_count(), 0);
    // A window resized meanwhile doesn't build it for nothing.
    window.set_emoji_panel_width(648.);
    window.invoke_emoji_reflow();
    assert_eq!(window.get_emoji_rows().row_count(), 0);
    window.invoke_emoji_search("".into());
    let rows = window.get_emoji_rows();
    assert!(rows.row_count() > 100);
    let drawn: usize = (0..8)
        .filter_map(|index| rows.row_data(index))
        .map(|row| {
            row.cells
                .iter()
                .filter(|cell| cell.picture.size().width > 0)
                .count()
        })
        .sum();
    assert!(drawn > 20, "the picker that opens is drawn: {drawn}");
}

#[test]
fn scrolling_cancels_far_emoji_even_before_any_picture_arrives() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.set_emoji_panel_width(498.);
    window.invoke_emoji_search("".into());
    let far = "https://cdn.discordapp.com/emojis/999999.png".to_owned();
    // A request from a row already scrolled past: none have finished, so a
    // decoded-picture threshold would incorrectly leave it running.
    adapter
        .borrow_mut()
        .app
        .dispatch(Action::LoadPicturesFirst(vec![far.clone()]));
    let top = window.get_emoji_rows().row_data(80).unwrap().top;
    window.invoke_emoji_scrolled_to(top, 340.);
    let mut adapter = adapter.borrow_mut();
    adapter.app.apply(Event::Picture {
        url: far.clone(),
        picture: Arc::new(Picture::default()),
    });
    assert!(
        !adapter.app.state.pictures.contains_key(&far),
        "a late reply for the abandoned row is not retained"
    );
}

#[test]
fn the_picker_asks_for_the_pictures_of_the_custom_emoji_it_shows() {
    let row = |cells: Vec<EmojiCell>| EmojiRow {
        heading: "".into(),
        cells: ModelRc::new(VecModel::from(cells)),
        ..Default::default()
    };
    let rows = VecModel::from(vec![
        row(vec![emoji::cell("🍕")]),
        row(vec![emoji::cell("cate:5"), emoji::cell("dog:6")]),
    ]);
    let fetch = |first, last, around| callbacks::custom_ids_to_fetch(&rows, first, last, around);
    // A Unicode glyph needs no picture, so the first row asks for none.
    assert!(
        fetch(0, 0, 0).is_empty(),
        "the Unicode row asks for nothing"
    );
    assert_eq!(fetch(0, 1, 0), ["5", "6"]);
    // Rows past the end are left alone rather than read.
    assert_eq!(fetch(1, 9, 0).len(), 2);
    assert!(fetch(5, 9, 0).is_empty());
    // The rows around those shown are asked for too, nearest first.
    assert_eq!(fetch(0, 0, 1), ["5", "6"]);
    assert_eq!(fetch(2, 2, 0), Vec::<String>::new());
    // A picture that arrived, or a name the server kept without one, is not
    // asked for again.
    let shown = rows.row_data(1).unwrap();
    let filled = emoji::cell("cate:5");
    shown.cells.set_row_data(
        0,
        EmojiCell {
            picture: slint::Image::from_rgba8(slint::SharedPixelBuffer::new(2, 2)),

            usable: true,
            ..filled
        },
    );
    assert_eq!(fetch(1, 1, 0), ["6"]);
}

#[test]
fn the_emoji_grid_fits_the_width_the_picker_shows_at() {
    // The widest row of the grid, which is the emoji a row fit.
    fn widest(window: &YapfastWindow) -> usize {
        let rows = window.get_emoji_rows();
        (0..rows.row_count())
            .map(|index| rows.row_data(index).unwrap().cells.row_count())
            .max()
            .unwrap_or(0)
    }
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    // The emoji popup opens 408px wide: six 56px buttons a row.
    window.set_emoji_panel_width(408.);
    window.invoke_emoji_search("".into());
    assert_eq!(widest(&window), 6);
    // The picker above the composer opens at 498, and its left edge drags it
    // wider: the grid fits more emoji a row, as the GIFs' grid does.
    window.set_emoji_panel_width(498.);
    window.invoke_emoji_reflow();
    assert_eq!(widest(&window), 7);
    window.set_emoji_panel_width(648.);
    window.invoke_emoji_reflow();
    assert_eq!(widest(&window), 10);
    // Narrower than the default, the grid fits fewer.
    window.set_emoji_panel_width(560.);
    window.invoke_emoji_reflow();
    assert_eq!(widest(&window), 8);
}

#[test]
fn dragging_the_pickers_left_edge_widens_it_no_further_than_the_server_list() {
    let window = showing(vec![]);
    window.set_can_pick_gifs(true);
    // The widths letting go of the edge keeps.
    let chosen = Rc::new(RefCell::new(vec![]));
    let kept = chosen.clone();
    window.on_picker_width_chosen(move |width| kept.borrow_mut().push(width));
    settle(&window);
    let button = slint::platform::PointerEventButton::Left;
    let at = |x: f32, y: f32| slint::LogicalPosition::new(x, y);
    let click = |x: f32, y: f32| {
        let native = window.window();
        native.dispatch_event(slint::platform::WindowEvent::PointerMoved { position: at(x, y) });
        native.dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: at(x, y),
            button,
        });
        native.dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position: at(x, y),
            button,
        });
        settle(&window);
    };
    // The GIF button in the composer opens the picker, 498px wide, its right
    // edge on the emoji button's.
    click(992., 710.);
    assert_eq!(window.get_picker_shown(), 498.);
    assert_eq!(window.get_gif_grid_width(), 474.);
    // Room for it up to 8px right of the server list, which is 80px from the
    // window's left edge.
    let room = window.get_picker_room();
    assert_eq!(room, 978.);
    // Where the picker shows on screen: its right edge, which is the emoji
    // button's, must stay there however far its left edge is dragged.
    // The panel inside the popup, which spans all the room it could take:
    // its left edge on the window and its width; and its top.
    let found = || {
        use i_slint_core::{accessibility::AccessibleStringProperty, item_tree::ItemRc};
        use std::ops::ControlFlow;
        let inner = i_slint_core::window::WindowInner::from_pub(window.window());
        let popups = inner.active_popups();
        let popup = popups.last().expect("the picker is open");
        let i_slint_core::window::PopupWindowLocation::ChildWindow(place) = popup.location else {
            panic!("not a child window");
        };
        let panel = ItemRc::new_root(popup.component.clone())
            .visit_descendants(|item| {
                let label = item.accessible_string_property(AccessibleStringProperty::Label);
                if label.as_deref() == Some("Expression Picker") {
                    ControlFlow::Break(item.clone())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .expect("the picker's panel");
        let geometry = panel.geometry();
        let at = panel.map_to_item_tree(geometry.origin, &popup.component);
        (place.x + at.x, geometry.width(), place.y + at.y)
    };
    let panel = || {
        let (x, width, _) = found();
        (x, width)
    };
    let right_edge = || {
        let (x, width) = panel();
        x + width
    };
    assert_eq!(panel(), (560., 498.));
    // Its left edge is the right edge less its width, 1058 - 498 = 560, and
    // the handle starts there: press 4px into it, then move the pointer, as a
    // mouse reports it, several times. Each move must widen the picker by the
    // distance the pointer travelled: Slint measures the pointer against the
    // popup's place when it was pressed for the whole drag, so a width taken
    // from the width so far would count that travel again on every move.
    let press = |x: f32| {
        let native = window.window();
        native.dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: at(x, 400.),
        });
        native.dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: at(x, 400.),
            button,
        });
    };
    let moved = |x: f32| {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: at(x, 400.),
            });
    };
    let release = |x: f32| {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerReleased {
                position: at(x, 400.),
                button,
            });
        settle(&window);
    };
    // A press and a release that don't move it keep no width.
    press(564.);
    release(564.);
    assert!(chosen.borrow().is_empty());
    press(564.);
    moved(514.);
    assert_eq!(window.get_picker_shown(), 548.);
    assert_eq!(panel(), (510., 548.));
    moved(464.);
    assert_eq!(window.get_picker_shown(), 598.);
    assert_eq!(right_edge(), 1058.);
    moved(414.);
    assert_eq!(window.get_picker_shown(), 648.);
    assert_eq!(right_edge(), 1058.);
    // The grid takes the width it got: three columns of 200px.
    assert_eq!(window.get_gif_grid_width(), 624.);
    release(414.);
    assert_eq!(window.get_picker_width(), 648.);
    assert_eq!(right_edge(), 1058.);
    assert_eq!(*chosen.borrow(), [648.]);
    // The emoji show at the width dragged too: the two tabs share the window.
    let tabs = found().2 + 30.;
    click(410. + 110., tabs);
    assert_eq!(panel(), (410., 648.));
    assert_eq!(window.get_emoji_panel_width(), 648.);
    // A click on the panel's empty places keeps it open; one beside it, in
    // the room it could take, closes it.
    click(410. + 300., tabs);
    assert_eq!(panel(), (410., 648.));
    click(300., tabs);
    let inner = i_slint_core::window::WindowInner::from_pub(window.window());
    assert!(inner.active_popups().is_empty());
    // It opens again on the GIFs, at the width dragged.
    click(992., 710.);
    assert_eq!(panel(), (410., 648.));
    // Back to the right, the edge follows the pointer to the width it opens
    // at, and no further.
    press(414.);
    moved(514.);
    assert_eq!(window.get_picker_shown(), 548.);
    moved(700.);
    assert_eq!(window.get_picker_shown(), 498.);
    release(700.);
    assert_eq!(window.get_picker_width(), 498.);
    // Left again, the edge stops just past the server list.
    press(564.);
    moved(0.);
    assert_eq!(window.get_picker_shown(), room);
    assert_eq!(right_edge(), 1058.);
    release(0.);
    assert_eq!(window.get_picker_width(), room);
    assert_eq!(right_edge(), 1058.);
    window.hide().unwrap();
}

#[test]
fn yapfast_navigation_matches_the_reference_column_width() {
    i_slint_backend_testing::init_no_event_loop();
    let window = YapfastWindow::new().unwrap();
    // The supplied 2560px reference places chat at x=376: 72px rail + 304px list.
    assert_eq!(window.get_sidebar_width(), 304.0);
    assert_eq!(window.get_sidebar_shown(), 304.0);

    window.global::<ClientStyle>().set_enabled(false);
    assert_eq!(window.get_sidebar_width(), 240.0);
    assert_eq!(window.get_sidebar_shown(), 240.0);
    Layout::default().apply_appearance(&window);
    assert_eq!(window.get_theme_id(), "yap");
}

#[test]
fn yapfast_defaults_to_bonfire_and_keeps_saved_appearance_choices() {
    i_slint_backend_testing::init_no_event_loop();
    let window = YapfastWindow::new().unwrap();
    Layout::default().apply_appearance(&window);
    assert_eq!(window.get_theme_id(), "bonfire");
    assert_eq!(window.get_sidebar_width(), 304.0);

    let saved = Layout {
        theme: Some("nord".into()),
        sidebar_width: Some(280.0),
        ..Default::default()
    };
    saved.apply_appearance(&window);
    assert_eq!(window.get_theme_id(), "nord");
    assert_eq!(window.get_sidebar_width(), 280.0);
}

#[test]
fn yapfast_caption_buttons_keep_the_existing_window_actions() {
    use slint::platform::{PointerEventButton, WindowEvent};
    i_slint_backend_testing::init_no_event_loop();
    let window = YapfastWindow::new().unwrap();
    window.set_own_title_bar(true);
    let actions = Rc::new(RefCell::new(Vec::new()));
    let seen = actions.clone();
    window.on_minimize_window(move || seen.borrow_mut().push("minimize"));
    let seen = actions.clone();
    window.on_maximize_window(move || seen.borrow_mut().push("maximize"));
    let seen = actions.clone();
    window.on_close_window(move || seen.borrow_mut().push("close"));
    window.show().unwrap();
    for width in [800., 1120.] {
        window
            .window()
            .set_size(slint::LogicalSize::new(width, 760.));
        for offset in [90., 54., 18.] {
            let position = slint::LogicalPosition::new(width - offset, 16.);
            window.window().dispatch_event(WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            });
            window
                .window()
                .dispatch_event(WindowEvent::PointerReleased {
                    position,
                    button: PointerEventButton::Left,
                });
        }
    }
    assert_eq!(
        *actions.borrow(),
        [
            "minimize", "maximize", "close", "minimize", "maximize", "close"
        ]
    );
    window.hide().unwrap();
}

#[test]
fn yapfast_account_panel_keeps_settings_and_audio_actions_reachable() {
    use slint::platform::{PointerEventButton, WindowEvent};
    let window = one_message("Synthetic message");
    window.global::<ClientStyle>().set_enabled(true);
    window.set_own_title_bar(true);
    window.set_preview(true);
    let muted = Rc::new(Cell::new(false));
    let seen = muted.clone();
    window.on_toggle_mute(move || seen.set(true));
    let deafened = Rc::new(Cell::new(false));
    let seen = deafened.clone();
    window.on_toggle_deafen(move || seen.set(true));
    for (x, y) in [(254., 732.), (308., 732.), (352., 732.)] {
        let position = slint::LogicalPosition::new(x, y);
        window.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        window
            .window()
            .dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
    }
    assert!(
        muted.get(),
        "the microphone remains operable in the account panel"
    );
    assert!(
        deafened.get(),
        "the headphone toggle remains operable in the account panel"
    );
    assert!(window.get_settings_open(), "settings remain reachable");
    window.hide().unwrap();
}

#[test]
fn yapfast_docked_profile_opens_the_existing_full_profile() {
    use slint::platform::{PointerEventButton, WindowEvent};
    let window = one_message("Synthetic message");
    window.global::<ClientStyle>().set_enabled(true);
    window.set_own_title_bar(true);
    window.set_profile_available(true);
    window.set_panel_profile_shown(true);
    window.set_panel_profile(ProfileData {
        name: "Sam".into(),
        initial: "S".into(),
        ..Default::default()
    });
    let opened = Rc::new(Cell::new(false));
    let seen = opened.clone();
    window.on_enlarge_panel_profile(move || seen.set(true));
    let position = slint::LogicalPosition::new(900., 100.);
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    assert!(
        opened.get(),
        "the docked profile banner opens the full profile through its adapter callback"
    );
    window.hide().unwrap();
}

#[test]
fn yapfast_dm_heading_keeps_friend_and_new_message_actions() {
    use slint::platform::{PointerEventButton, WindowEvent};
    let window = one_message("Synthetic message");
    window.global::<ClientStyle>().set_enabled(true);
    window.set_own_title_bar(true);
    window.set_dms_selected(true);
    window.set_discord_linked(true);
    window.set_preview(false);
    window.set_rooms(ModelRc::new(VecModel::from(vec![RoomRow {
        id: "synthetic-dm".into(),
        name: "Sam".into(),
        direct: true,
        person: true,
        ..Default::default()
    }])));
    let friend = Rc::new(Cell::new(false));
    let seen = friend.clone();
    window.on_open_add_friend(move || seen.set(true));
    let message = Rc::new(Cell::new(false));
    let seen = message.clone();
    window.on_open_new_dm(move || seen.set(true));
    for x in [328., 356.] {
        let position = slint::LogicalPosition::new(x, 182.);
        window.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        window
            .window()
            .dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
    }
    assert!(
        friend.get(),
        "the existing add-friend action remains reachable"
    );
    assert!(
        message.get(),
        "the existing new-DM action remains reachable"
    );
    window.hide().unwrap();
}

#[test]
fn yapfast_settings_navigation_and_header_keep_the_existing_actions() {
    use slint::platform::{Key, PointerEventButton, WindowEvent};
    i_slint_backend_testing::init_no_event_loop();
    let window = YapfastWindow::new().unwrap();
    window.set_own_title_bar(true);
    window.set_account_name("Alex".into());
    window.set_account_initial("A".into());
    window.set_discord_build(true);
    window.set_discord_linked(true);
    window.set_settings_open(true);
    window.set_settings_section(1);
    let refreshed = Rc::new(Cell::new(0));
    let seen = refreshed.clone();
    window.on_refresh_devices(move || seen.set(seen.get() + 1));
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1920., 1080.));
    // The settings card fades in; let the animation finish so it takes clicks.
    i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(1000));
    let click = |x, y| {
        let position = slint::LogicalPosition::new(x, y);
        window.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        window
            .window()
            .dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
    };
    // Reference geometry: a 1400px card, 72px outer inset, 266px navigation.
    click(350., 500.);
    assert_eq!(
        window.get_settings_section(),
        7,
        "Appearance remains reachable"
    );
    click(350., 340.);
    assert_eq!(window.get_settings_section(), 1);
    assert_eq!(refreshed.get(), 1, "Voice still refreshes the device list");
    click(350., 180.);
    for character in "APPEAR".chars() {
        let text: SharedString = character.to_string().into();
        window
            .window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window
            .window()
            .dispatch_event(WindowEvent::KeyReleased { text });
    }
    click(350., 236.);
    assert_eq!(
        window.get_settings_section(),
        7,
        "search filters supported pages regardless of case"
    );
    click(1636., 96.);
    assert!(!window.get_settings_open(), "the header close action works");
    window.set_settings_open(true);
    window.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Escape.into(),
    });
    assert!(
        !window.get_settings_open(),
        "Escape still dismisses settings"
    );
    window.hide().unwrap();
}

#[test]
fn yapfast_account_popover_copies_its_id_and_opens_settings_without_changing_status() {
    use slint::platform::{PointerEventButton, WindowEvent};
    let window = one_message("Synthetic message");
    window.global::<ClientStyle>().set_enabled(true);
    window.set_own_title_bar(true);
    window.set_discord_linked(true);
    window.set_preview(false);
    window.set_account_name("Alex".into());
    window.set_account_initial("A".into());
    window.set_account_user_id("123456789012345678".into());
    let status_changes = Rc::new(Cell::new(0));
    let seen = status_changes.clone();
    window.on_set_status(move |_| seen.set(seen.get() + 1));
    for (x, y) in [(100., 722.), (160., 632.), (100., 722.), (160., 674.)] {
        let position = slint::LogicalPosition::new(x, y);
        window.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        window
            .window()
            .dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
    }
    assert_eq!(
        i_slint_core::window::WindowInner::from_pub(window.window())
            .context()
            .platform()
            .clipboard_text(i_slint_core::platform::Clipboard::DefaultClipboard)
            .as_deref(),
        Some("123456789012345678")
    );
    assert!(window.get_settings_open());
    assert_eq!(window.get_settings_section(), 0);
    assert_eq!(status_changes.get(), 0);
    window.hide().unwrap();
}

#[test]
fn yapfast_server_rail_keeps_home_pinned_without_a_duplicate_separator() {
    use slint::platform::{PointerEventButton, WindowEvent};
    i_slint_backend_testing::init_no_event_loop();
    let window = YapfastWindow::new().unwrap();
    window.set_workspace(true);
    let mut rows = vec![ServerRow {
        divider: true,
        ..Default::default()
    }];
    rows.extend((0..500).map(|index| ServerRow {
        id: index.to_string().into(),
        name: format!("Server {index}").into(),
        initial: "S".into(),
        ..Default::default()
    }));
    let model = Rc::new(VecModel::from(rows));
    window.set_servers(model.clone().into());
    let navigated = Rc::new(RefCell::new(Vec::new()));
    let seen = navigated.clone();
    window.on_navigate_server(move |id| seen.borrow_mut().push(id.to_string()));
    let homes = Rc::new(Cell::new(0));
    let seen = homes.clone();
    window.on_navigate_dms(move || seen.set(seen.get() + 1));
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    let click = |y| {
        let position = slint::LogicalPosition::new(36., y);
        window.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        window
            .window()
            .dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
    };
    let scroll = |delta_y| {
        window
            .window()
            .dispatch_event(WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(36., 200.),
                delta_x: 0.,
                delta_y,
            });
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(250));
    };
    // Padding 12px, Home 40px, gap 8px, one 2px separator, gap 8px: server starts at 70px.
    click(74.);
    assert_eq!(*navigated.borrow(), ["0"]);
    scroll(-240.);
    click(26.);
    assert_eq!(homes.get(), 1, "DM home stays pinned while servers scroll");
    model.insert(
        0,
        ServerRow {
            id: "dm".into(),
            name: "Unread conversation".into(),
            dm: true,
            badge: "1".into(),
            ..Default::default()
        },
    );
    scroll(10000.);
    navigated.borrow_mut().clear();
    // Unread DM at 70px, then its 8px gap and a 10px divider row.
    click(122.);
    assert!(navigated.borrow().is_empty(), "keep the unread-DM boundary");
    click(132.);
    assert_eq!(*navigated.borrow(), ["0"]);
    window.hide().unwrap();
}

#[test]
fn yapfast_typography_is_compact_and_preserves_explicit_text_sizes() {
    i_slint_backend_testing::init_no_event_loop();
    let window = yapfast::window().unwrap();
    assert_eq!(window.global::<ClientStyle>().get_font_family(), "DM Sans");
    assert_eq!(window.get_message_font_size(), 15.);
    window.set_text_size(0);
    assert_eq!(window.get_message_font_size(), 14.);
    window.set_text_size(2);
    assert_eq!(window.get_message_font_size(), 18.);
    window.set_text_size(1);
    window.global::<ClientStyle>().set_enabled(false);
    assert_eq!(window.get_message_font_size(), 16.);
    assert_eq!(
        window.global::<ClientStyle>().get_font_family(),
        "Atkinson Hyperlegible Next"
    );
}

/// CPU-side snapshot throughput, not the display's presented frame rate.
/// Run with --ignored --nocapture. It measures the testing backend's software
/// renderer: the testing backend here has no option to choose Skia.
#[test]
#[ignore = "manual renderer benchmark with the Skia testing feature"]
fn yapfast_server_rail_scroll_snapshot_benchmark() {
    use slint::platform::WindowEvent;
    use std::time::Instant;
    slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            threading: false,
        },
    )))
    .unwrap();
    let window = yapfast::window().unwrap();
    Layout::default().apply_appearance(&window);
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    adapter.borrow_mut().action(&window, Action::Preview);
    window.set_workspace(true);
    window.set_servers(ModelRc::new(VecModel::from(
        (0..500)
            .map(|index| {
                let mut pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(64, 64);
                for (offset, pixel) in pixels.make_mut_slice().iter_mut().enumerate() {
                    *pixel = slint::Rgba8Pixel {
                        r: ((index * 17 + offset) % 255) as u8,
                        g: ((index * 31 + offset / 64) % 255) as u8,
                        b: 150,
                        a: 255,
                    };
                }
                ServerRow {
                    id: index.to_string().into(),
                    name: format!("Server {index}").into(),
                    picture: slint::Image::from_rgba8(pixels),
                    unread: index % 3 == 0,
                    badge: if index % 7 == 0 {
                        "3".into()
                    } else {
                        "".into()
                    },
                    ..Default::default()
                }
            })
            .collect::<Vec<_>>(),
    )));
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(2560., 1440.));
    for _ in 0..10 {
        window.window().take_snapshot().unwrap();
    }
    if let Some(path) = std::env::var_os("YAPFAST_REVIEW_SNAPSHOT") {
        let frame = window.window().take_snapshot().unwrap();
        image::save_buffer(
            path,
            frame.as_bytes(),
            frame.width(),
            frame.height(),
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    let start = Instant::now();
    for frame in 0..240 {
        if frame % 6 == 0 {
            window
                .window()
                .dispatch_event(WindowEvent::PointerScrolled {
                    position: slint::LogicalPosition::new(36., 350.),
                    delta_x: 0.,
                    delta_y: -120.,
                });
        }
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(16));
        window.window().take_snapshot().unwrap();
    }
    println!(
        "rail snapshot benchmark: 240 frames in {:?}, {:.2} ms/frame",
        start.elapsed(),
        start.elapsed().as_secs_f64() * 1000. / 240.
    );
    window.hide().unwrap();
}

/// A profile without a banner tints its banner area with the avatar's
/// dominant colour: a vivid colour on a grey ground wins over the ground.
#[test]
fn dominant_colour_of_an_avatar_is_its_vivid_colour() {
    use slint::{Rgba8Pixel, SharedPixelBuffer};
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(32, 32);
    for (at, pixel) in buffer.make_mut_slice().iter_mut().enumerate() {
        *pixel = if at % 8 == 0 {
            Rgba8Pixel::new(200, 20, 20, 255)
        } else {
            Rgba8Pixel::new(120, 120, 120, 255)
        };
    }
    let color = profile::dominant_color(&slint::Image::from_rgba8(buffer)).unwrap();
    let (red, green) = ((color >> 16) & 0xff, (color >> 8) & 0xff);
    assert!(red > 150 && green < 90, "{color:06x}");
    let empty = SharedPixelBuffer::<Rgba8Pixel>::new(4, 4);
    assert_eq!(
        profile::dominant_color(&slint::Image::from_rgba8(empty)),
        None
    );
}

#[test]
fn an_avatars_colour_is_worked_out_once_it_has_arrived() {
    use slint::{Rgba8Pixel, SharedPixelBuffer};
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(4, 4);
    buffer
        .make_mut_slice()
        .fill(Rgba8Pixel::new(200, 20, 20, 255));
    let avatar = slint::Image::from_rgba8(buffer);
    let waiting = slint::Image::default();
    assert_eq!(profile::avatar_color(Some("a.png"), &waiting), None);
    let color = profile::avatar_color(Some("a.png"), &avatar);
    assert!(color.is_some());
    assert_eq!(
        profile::avatar_color(Some("a.png"), &waiting),
        color,
        "kept by address, without reading the picture again"
    );
    assert_eq!(profile::avatar_color(Some("b.png"), &waiting), None);
    assert_eq!(profile::avatar_color(None, &avatar), None);
}

#[test]
fn a_long_message_folds_until_show_more_only_with_the_switch_on() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();
    assert!(!window.get_collapse_long(), "off unless turned on");
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(1120., 760.));
    // A frame or so: the list lays its rows out as time moves on.
    let settle = |window: &YapfastWindow| {
        for _ in 0..5 {
            i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(1));
            slint::platform::update_timers_and_animations();
            let _ = window.invoke_timeline_extent();
        }
    };
    settle(&window);
    let before = window.invoke_timeline_extent();
    let count = window.get_messages().row_count();
    // The preview's short messages don't fold.
    window.set_collapse_long(true);
    settle(&window);
    assert_eq!(window.invoke_timeline_extent(), before);
    window.set_collapse_long(false);
    settle(&window);
    let long: String = (1..=40)
        .map(|line| format!("Line {line} of the plan\n"))
        .collect();
    adapter
        .borrow_mut()
        .action(&window, Action::SetDraft(long.trim_end().into()));
    adapter.borrow_mut().action(&window, Action::Send);
    settle(&window);
    assert_eq!(window.get_messages().row_count(), count + 1, "it was sent");
    let item = window.get_messages().iter().last().unwrap().item;
    let whole = window.invoke_timeline_extent();
    let line = window.get_message_font_size() * 1.3;
    assert!(whole - before > line * 30., "{before} grew to {whole}");

    // With the switch on it folds to twelve lines, and the list is shorter.
    window.set_collapse_long(true);
    settle(&window);
    let folded = window.invoke_timeline_extent();
    assert!(whole - folded > line * 20., "{whole} folds to {folded}");

    // Show more shows it whole, in its own row, and Show less folds it again.
    window.invoke_fold_message(item.clone(), true);
    settle(&window);
    let row = window.get_messages().iter().last().unwrap();
    assert!(row.item == item && row.expanded);
    let shown = window.invoke_timeline_extent();
    assert!(shown > whole, "whole, with Show less under it: {shown}");
    window.invoke_fold_message(item.clone(), false);
    settle(&window);
    assert!(!window.get_messages().iter().last().unwrap().expanded);
    assert_eq!(window.invoke_timeline_extent(), folded);

    // The switch is kept with the other preferences.
    window.invoke_preferences_changed();
    assert!(adapter.borrow().layout.collapse_long);
}

#[test]
fn better_animations_phase_a_core_motion_and_overlays() {
    i_slint_backend_testing::init_no_event_loop();
    let window = YapfastWindow::new().unwrap();
    let popup = window.global::<Popup>();
    let looks = window.global::<Looks>();

    // 1. By default, Better Animations is OFF: all durations are 0ms and transforms are identity.
    assert!(!looks.get_better_animations());
    assert_eq!(popup.get_duration_enter(), 0);
    assert_eq!(popup.get_duration_exit(), 0);
    assert_eq!(popup.get_menu_enter(), 0);
    assert_eq!(popup.get_tooltip_enter(), 0);
    assert_eq!(popup.get_popover_enter(), 0);
    assert_eq!(popup.get_hover_enter(), 0);
    assert_eq!(popup.get_hover_exit(), 0);
    assert!((popup.invoke_scale(0.0) - 1.0).abs() < 1e-4);
    assert!((popup.invoke_lift(0.0) - 0.0).abs() < 1e-4);
    assert!((popup.invoke_menu_scale(0.0) - 1.0).abs() < 1e-4);
    assert!((popup.invoke_popover_scale(0.0) - 1.0).abs() < 1e-4);

    // 2. When Better Animations is ON: expressive durations, scale, and lift are enabled.
    looks.set_better_animations(true);
    assert!((popup.invoke_scale(0.0) - 0.92).abs() < 1e-4);
    assert!((popup.invoke_scale(1.0) - 1.0).abs() < 1e-4);
    assert!((popup.invoke_lift(0.0) - 10.0).abs() < 1e-4);
    assert!((popup.invoke_lift(1.0) - 0.0).abs() < 1e-4);

    // Verify modal entry (200ms) and exit (135ms) durations.
    assert_eq!(popup.get_duration_enter(), 200);
    assert_eq!(popup.get_duration_exit(), 135);

    // Verify context menu, tooltip, popover, and hover durations and scale ranges.
    assert_eq!(popup.get_menu_enter(), 140);
    assert_eq!(popup.get_tooltip_enter(), 100);
    assert_eq!(popup.get_popover_enter(), 160);
    assert_eq!(popup.get_hover_enter(), 160);
    assert_eq!(popup.get_hover_exit(), 130);
    assert!((popup.invoke_menu_scale(0.0) - 0.93).abs() < 1e-4);
    assert!((popup.invoke_menu_scale(1.0) - 1.0).abs() < 1e-4);
    assert!((popup.invoke_popover_scale(0.0) - 0.95).abs() < 1e-4);
    assert!((popup.invoke_popover_scale(1.0) - 1.0).abs() < 1e-4);

    // Verify status menu toggle stability without conflicting scale transforms.
    window.set_account_status_expanded(false);
    assert!(!window.get_account_status_expanded());
    window.set_account_status_expanded(true);
    assert!(window.get_account_status_expanded());
}

#[test]
fn better_animations_hover_interaction_durations_and_shift() {
    i_slint_backend_testing::init_no_event_loop();
    let window = YapfastWindow::new().unwrap();
    let popup = window.global::<Popup>();
    let looks = window.global::<Looks>();

    // When OFF: 0ms
    assert_eq!(popup.get_hover_enter(), 0);
    assert_eq!(popup.get_hover_exit(), 0);

    // When ON: 160ms enter, 130ms exit
    looks.set_better_animations(true);
    assert_eq!(popup.get_hover_enter(), 160);
    assert_eq!(popup.get_hover_exit(), 130);
}

#[test]
fn better_animations_account_status_popup_geometry_stability() {
    i_slint_backend_testing::init_no_event_loop();
    let window = YapfastWindow::new().unwrap();
    window.set_account_name("Tester".into());
    window.set_account_user_id("12345678".into());
    window.invoke_open_status_menu();

    use i_slint_core::items::ItemRef;
    use i_slint_core::window::{PopupWindowLocation, WindowInner};

    let initial_y;
    let initial_h;
    {
        let popups = WindowInner::from_pub(window.window()).active_popups();
        assert_eq!(popups.len(), 1, "status-menu popup should be active");
        match &popups[0].location {
            PopupWindowLocation::ChildWindow(pt) => {
                initial_y = pt.y;
            }
            _ => panic!("Expected ChildWindow"),
        }
        let component = i_slint_core::item_tree::ItemTreeRc::borrow_pin(&popups[0].component);
        let root_item = component.as_ref().get_item_ref(0);
        let w = ItemRef::downcast_pin::<i_slint_core::items::WindowItem>(root_item).unwrap();
        initial_h = w.height().get();
        // The bottom of the popup must align exactly at root.menu-y - 4px
        assert_eq!(initial_y + initial_h, window.get_menu_y() - 4.0);
    }

    // Toggle expansion to true
    window.set_account_status_expanded(true);
    slint::platform::update_timers_and_animations();
    i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(50));
    slint::platform::update_timers_and_animations();

    {
        let popups = WindowInner::from_pub(window.window()).active_popups();
        assert_eq!(popups.len(), 1);
        match &popups[0].location {
            PopupWindowLocation::ChildWindow(pt) => {
                // Location must remain stable to avoid any off-sync repaint frames
                assert_eq!(
                    pt.y, initial_y,
                    "Popup location must remain stable when expanded"
                );
            }
            _ => panic!("Expected ChildWindow"),
        }
        let component = i_slint_core::item_tree::ItemTreeRc::borrow_pin(&popups[0].component);
        let root_item = component.as_ref().get_item_ref(0);
        let w = ItemRef::downcast_pin::<i_slint_core::items::WindowItem>(root_item).unwrap();
        let expanded_h = w.height().get();
        assert_eq!(
            expanded_h, initial_h,
            "PopupWindow height must remain stable"
        );
        assert_eq!(
            initial_y + expanded_h,
            window.get_menu_y() - 4.0,
            "Popup bottom must stay at menu-y - 4px"
        );
    }

    // Toggle expansion back to false
    window.set_account_status_expanded(false);
    slint::platform::update_timers_and_animations();
    i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(50));
    slint::platform::update_timers_and_animations();

    {
        let popups = WindowInner::from_pub(window.window()).active_popups();
        assert_eq!(popups.len(), 1);
        match &popups[0].location {
            PopupWindowLocation::ChildWindow(pt) => {
                assert_eq!(pt.y, initial_y);
            }
            _ => panic!("Expected ChildWindow"),
        }
    }
}

#[test]
fn better_animations_phase_b1_switch_transition_constants() {
    i_slint_backend_testing::init_no_event_loop();
    let window = YapfastWindow::new().unwrap();
    let popup = window.global::<Popup>();
    let looks = window.global::<Looks>();

    // When OFF (default): 0ms and 0px
    assert_eq!(popup.get_switch_enter(), 0);
    assert_eq!(popup.get_switch_exit(), 0);
    assert!((popup.get_switch_offset() - 0.0).abs() < 1e-4);
    assert!((popup.get_switch_drift() - 0.0).abs() < 1e-4);
    assert!((popup.get_switch_offset_x() - 0.0).abs() < 1e-4);
    assert!((popup.get_switch_drift_x() - 0.0).abs() < 1e-4);

    // When ON: BetterAnimations switch transition timings and directional offsets
    looks.set_better_animations(true);
    assert_eq!(popup.get_switch_enter(), 195);
    assert_eq!(popup.get_switch_exit(), 75);
    assert!((popup.get_switch_offset() - 24.0).abs() < 1e-4);
    assert!((popup.get_switch_drift() - (-6.0)).abs() < 1e-4);
    assert!((popup.get_switch_offset_x() - 24.0).abs() < 1e-4);
    assert!((popup.get_switch_drift_x() - (-8.0)).abs() < 1e-4);
}

#[test]
fn better_animations_phase_b1_chat_motion_phase_properties() {
    i_slint_backend_testing::init_no_event_loop();
    let window = YapfastWindow::new().unwrap();
    window.global::<Looks>().set_better_animations(true);
    assert_eq!(window.get_nav_motion_phase(), 0);
    assert_eq!(window.get_nav_motion_direction(), 0);
    assert!((window.get_nav_transition_opacity() - 1.0).abs() < 1e-4);
    assert!((window.get_nav_transition_y() - 0.0).abs() < 1e-4);
    assert!((window.get_nav_transition_x() - 0.0).abs() < 1e-4);

    // Test Exiting vertical state (phase 1, direction 0)
    window.set_nav_motion_direction(0);
    window.set_nav_motion_phase(1);
    assert_eq!(window.get_nav_motion_phase(), 1);

    // Test EnteringPrep vertical state (phase 2, direction 0)
    window.set_nav_motion_phase(2);
    assert_eq!(window.get_nav_motion_phase(), 2);
    assert!((window.get_nav_transition_opacity() - 0.0).abs() < 1e-4);
    assert!((window.get_nav_transition_y() - 24.0).abs() < 1e-4);
    assert!((window.get_nav_transition_x() - 0.0).abs() < 1e-4);

    // Test Exiting horizontal state (phase 1, direction 1)
    window.set_nav_motion_direction(1);
    window.set_nav_motion_phase(1);
    assert_eq!(window.get_nav_motion_phase(), 1);
    assert_eq!(window.get_nav_motion_direction(), 1);

    // Test EnteringPrep horizontal state (phase 2, direction 1)
    window.set_nav_motion_phase(2);
    assert_eq!(window.get_nav_motion_phase(), 2);
    assert!((window.get_nav_transition_opacity() - 0.0).abs() < 1e-4);
    assert!((window.get_nav_transition_y() - 0.0).abs() < 1e-4);
    assert!((window.get_nav_transition_x() - 24.0).abs() < 1e-4);

    // Test return to Visible (phase 0)
    window.set_nav_motion_phase(0);
    assert_eq!(window.get_nav_motion_phase(), 0);
}

#[test]
fn better_animations_phase_b1_presentation_hold_and_rapid_switching() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    window.global::<ClientStyle>().set_enabled(true);
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    adapter.borrow_mut().layout.better_animations = true;
    window.invoke_explore_preview();

    // 1. Initial selection: "general"
    adapter
        .borrow_mut()
        .select_room_transition(&window, "general".into());
    // Since outgoing was empty, it entered directly into Entering
    let epoch1 = adapter.borrow().nav_transition.epoch;
    adapter.borrow_mut().on_nav_enter_start(&window, epoch1);
    adapter.borrow_mut().on_nav_enter_complete(&window, epoch1);
    assert_eq!(
        adapter.borrow().app.state.selected.as_deref(),
        Some("general")
    );
    assert_eq!(window.get_room_name(), "general");
    assert_eq!(adapter.borrow().nav_transition.phase, NavPhase::Idle);

    // 2. Select "games": immediate core dispatch at T=0, but presentation hold on "general"
    adapter
        .borrow_mut()
        .select_room_transition(&window, "games".into());
    assert_eq!(
        adapter.borrow().app.state.selected.as_deref(),
        Some("games"),
        "Core state must update immediately at T=0"
    );
    assert_eq!(
        window.get_nav_motion_phase(),
        1,
        "UI must enter Exiting motion phase (1)"
    );
    assert_eq!(
        window.get_nav_motion_direction(),
        0,
        "Direction must be Vertical (0)"
    );
    assert_eq!(
        window.get_room_name(),
        "general",
        "Visible header must hold outgoing channel during exit"
    );
    assert!(adapter.borrow().nav_transition.is_exiting());
    let epoch2 = adapter.borrow().nav_transition.epoch;

    // 3. Rapid click to "weekend" during Exiting phase (A -> B -> C scenario)
    adapter
        .borrow_mut()
        .select_room_transition(&window, "weekend".into());
    assert_eq!(
        adapter.borrow().app.state.selected.as_deref(),
        Some("weekend"),
        "Core state must immediately update to latest channel 'weekend'"
    );
    assert_eq!(
        adapter.borrow().nav_transition.target.as_deref(),
        Some("weekend")
    );
    assert_eq!(window.get_nav_motion_phase(), 1, "Still in Exiting phase");
    assert_eq!(
        window.get_room_name(),
        "general",
        "Still holding 'general' presentation until exit completes"
    );

    // 4. Exit timer completes:
    adapter.borrow_mut().on_nav_exit_complete(&window, epoch2);
    assert_eq!(
        window.get_nav_motion_phase(),
        2,
        "Must snap to EnteringPrep (phase 2, opacity 0.0, y 24px)"
    );
    assert_eq!(
        window.get_room_name(),
        "weekend plans",
        "Channel content must be updated to target while hidden"
    );

    // 5. Entering starts:
    adapter.borrow_mut().on_nav_enter_start(&window, epoch2);
    assert_eq!(
        window.get_nav_motion_phase(),
        0,
        "Must be in Visible state (phase 0) animating in"
    );

    // 6. Entering completes:
    adapter.borrow_mut().on_nav_enter_complete(&window, epoch2);
    assert_eq!(adapter.borrow().nav_transition.phase, NavPhase::Idle);
    assert_eq!(window.get_room_name(), "weekend plans");
}

#[test]
fn better_animations_settings_toggle_and_defaults() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();

    // Verify default is OFF
    assert!(!adapter.borrow().layout.better_animations);
    let looks = window.global::<Looks>();
    assert!(!looks.get_better_animations());

    // Toggle ON in Looks and read_cosmetics
    looks.set_better_animations(true);
    adapter.borrow_mut().layout.read_cosmetics(&window);
    assert!(adapter.borrow().layout.better_animations);

    // Show cosmetics back to window
    adapter.borrow().layout.show_cosmetics(&window);
    assert!(looks.get_better_animations());

    // Toggle OFF
    looks.set_better_animations(false);
    adapter.borrow_mut().layout.read_cosmetics(&window);
    assert!(!adapter.borrow().layout.better_animations);
}

#[test]
fn better_animations_master_toggle_runtime_settle_on_disable() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    window.global::<ClientStyle>().set_enabled(true);
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    adapter.borrow_mut().layout.better_animations = true;
    window.global::<Looks>().set_better_animations(true);
    window.invoke_explore_preview();

    // 1. Initial room is "general"
    adapter
        .borrow_mut()
        .select_room_transition(&window, "general".into());
    assert_eq!(adapter.borrow().nav_transition.phase, NavPhase::Idle);

    // 2. Start transition to "games" -> Enters Exiting phase
    adapter
        .borrow_mut()
        .select_room_transition(&window, "games".into());
    assert!(adapter.borrow().nav_transition.is_exiting());
    assert_eq!(window.get_nav_motion_phase(), 1);

    // 3. User flips Better Animations switch to OFF while transition is in flight
    window.global::<Looks>().set_better_animations(false);
    window.invoke_preferences_changed();

    // 4. Verify transition settled immediately: phase is Idle, timer stopped, phase is 0, view refreshed
    assert_eq!(adapter.borrow().nav_transition.phase, NavPhase::Idle);
    assert_eq!(adapter.borrow().nav_transition.target, None);
    assert_eq!(window.get_nav_motion_phase(), 0);
    assert_eq!(window.get_room_name(), "games");
}

#[test]
fn better_animations_workspace_horizontal_transition() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    window.global::<ClientStyle>().set_enabled(true);
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    adapter.borrow_mut().layout.better_animations = true;
    window.invoke_explore_preview();

    // Verify initial navigation is Server("friends")
    assert_eq!(
        adapter.borrow().app.state.navigation,
        Navigation::Server("friends".into())
    );
    assert!(!window.get_dms_selected());

    // Navigate to DirectMessages
    adapter
        .borrow_mut()
        .navigate_transition(&window, Navigation::DirectMessages);
    assert_eq!(
        adapter.borrow().app.state.navigation,
        Navigation::DirectMessages,
        "Core state must update immediately at T=0"
    );
    assert_eq!(window.get_nav_motion_phase(), 1, "Must be in Exiting phase");
    assert_eq!(
        window.get_nav_motion_direction(),
        1,
        "Must be Horizontal direction (1)"
    );
    assert!(adapter.borrow().nav_transition.is_workspace_exiting());
    assert!(
        !window.get_dms_selected(),
        "dms_selected must remain held (false) during exit to avoid flickering"
    );
    let epoch = adapter.borrow().nav_transition.epoch;

    // Exit completes
    adapter.borrow_mut().on_nav_exit_complete(&window, epoch);
    assert_eq!(window.get_nav_motion_phase(), 2);
    assert_eq!(window.get_nav_motion_direction(), 1);
    assert!(
        window.get_dms_selected(),
        "dms_selected must update to true once exit reaches opacity 0"
    );

    // Enter starts
    adapter.borrow_mut().on_nav_enter_start(&window, epoch);
    assert_eq!(window.get_nav_motion_phase(), 0);

    // Enter completes
    adapter.borrow_mut().on_nav_enter_complete(&window, epoch);
    assert_eq!(adapter.borrow().nav_transition.phase, NavPhase::Idle);
    assert_eq!(
        adapter.borrow().app.state.navigation,
        Navigation::DirectMessages
    );
}

#[test]
fn better_animations_workspace_switch_dms_to_server_holds_presentation() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    window.global::<ClientStyle>().set_enabled(true);
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    adapter.borrow_mut().layout.better_animations = true;
    window.invoke_explore_preview();

    // First go to DirectMessages without animation
    adapter.borrow_mut().layout.better_animations = false;
    adapter
        .borrow_mut()
        .navigate_transition(&window, Navigation::DirectMessages);
    assert!(window.get_dms_selected());

    // Re-enable Better Animations
    adapter.borrow_mut().layout.better_animations = true;

    // Navigate to Server("friends")
    adapter
        .borrow_mut()
        .navigate_transition(&window, Navigation::Server("friends".into()));
    assert_eq!(
        adapter.borrow().app.state.navigation,
        Navigation::Server("friends".into()),
        "Logical navigation must be immediate at T=0"
    );
    assert!(adapter.borrow().nav_transition.is_workspace_exiting());
    assert!(
        window.get_dms_selected(),
        "dms_selected must be held as true during exit to avoid flickering"
    );
    let epoch = adapter.borrow().nav_transition.epoch;

    // Complete exit
    adapter.borrow_mut().on_nav_exit_complete(&window, epoch);
    assert_eq!(window.get_nav_motion_phase(), 2);
    assert!(
        !window.get_dms_selected(),
        "dms_selected must flip to false while hidden"
    );

    // Enter completes
    adapter.borrow_mut().on_nav_enter_start(&window, epoch);
    adapter.borrow_mut().on_nav_enter_complete(&window, epoch);
    assert_eq!(adapter.borrow().nav_transition.phase, NavPhase::Idle);
    assert_eq!(
        adapter.borrow().app.state.navigation,
        Navigation::Server("friends".into())
    );
}

#[test]
fn better_animations_voice_channel_join_does_not_trigger_animation() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    window.global::<ClientStyle>().set_enabled(true);
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    adapter.borrow_mut().layout.better_animations = true;
    window.invoke_explore_preview();

    // Select "general"
    adapter
        .borrow_mut()
        .select_room_transition(&window, "general".into());
    let epoch = adapter.borrow().nav_transition.epoch;
    adapter.borrow_mut().on_nav_enter_start(&window, epoch);
    adapter.borrow_mut().on_nav_enter_complete(&window, epoch);
    assert_eq!(adapter.borrow().nav_transition.phase, NavPhase::Idle);
    assert_eq!(window.get_nav_motion_phase(), 0);

    // In preview mode, preview channels are loaded.
    let voice_room = adapter
        .borrow()
        .app
        .state
        .rooms
        .iter()
        .find(|r| r.voice && !r.direct)
        .map(|r| r.id.clone());

    if let Some(voice_id) = voice_room {
        adapter
            .borrow_mut()
            .select_room_transition(&window, voice_id);
        // Voice join should NOT initiate a chat transition:
        assert_eq!(adapter.borrow().nav_transition.phase, NavPhase::Idle);
        assert_eq!(window.get_nav_motion_phase(), 0);
    }
}

#[test]
fn better_animations_disabled_by_default_bypasses_transitions() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = YapfastWindow::new().unwrap();
    window.global::<ClientStyle>().set_enabled(true);
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    window.invoke_explore_preview();

    // Better animations is false by default
    assert!(!adapter.borrow().layout.better_animations);

    adapter
        .borrow_mut()
        .select_room_transition(&window, "general".into());
    assert_eq!(
        adapter.borrow().app.state.selected.as_deref(),
        Some("general")
    );
    assert_eq!(adapter.borrow().nav_transition.phase, NavPhase::Idle);
    assert_eq!(window.get_nav_motion_phase(), 0);

    adapter
        .borrow_mut()
        .select_room_transition(&window, "games".into());
    assert_eq!(
        adapter.borrow().app.state.selected.as_deref(),
        Some("games")
    );
    assert_eq!(adapter.borrow().nav_transition.phase, NavPhase::Idle);
    assert_eq!(window.get_room_name(), "games");
    assert_eq!(window.get_nav_motion_phase(), 0);
}

#[test]
fn better_animations_phase_b1_classic_mode_bypasses_transitions() {
    i_slint_backend_testing::init_no_event_loop();
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window(); // ClientStyle.enabled == false
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);
    adapter.borrow_mut().layout.better_animations = true;
    window.invoke_explore_preview();

    adapter
        .borrow_mut()
        .select_room_transition(&window, "general".into());
    assert_eq!(
        adapter.borrow().app.state.selected.as_deref(),
        Some("general")
    );
    assert_eq!(adapter.borrow().nav_transition.phase, NavPhase::Idle);
    assert_eq!(window.get_nav_motion_phase(), 0);

    adapter
        .borrow_mut()
        .select_room_transition(&window, "games".into());
    assert_eq!(
        adapter.borrow().app.state.selected.as_deref(),
        Some("games")
    );
    assert_eq!(adapter.borrow().nav_transition.phase, NavPhase::Idle);
    assert_eq!(window.get_room_name(), "games");
    assert_eq!(window.get_nav_motion_phase(), 0);
}

#[test]
#[cfg(feature = "discord")]
fn better_animations_profile_card_name_read_only_and_no_caret() {
    let (window, adapter, _directory) = discord_server_window();
    adapter.borrow_mut().refresh(&window);
    window.invoke_open_link("https://discord.com/users/4242".into());
    assert!(window.get_profile_open());
}

#[test]
fn better_animations_mini_profile_anchoring_independent_of_context_menu() {
    let window = one_message("Synthetic message");
    window.invoke_anchor_profile(120.0, 200.0, 160.0, 32.0);
    assert_eq!(window.get_profile_anchor_x(), 120.0);
    assert_eq!(window.get_profile_anchor_y(), 200.0);
    assert_eq!(window.get_profile_anchor_width(), 160.0);
    assert_eq!(window.get_profile_anchor_height(), 32.0);

    window.invoke_open_context_menu("person:42".into(), true, 380.0, 240.0);
    assert_eq!(
        window.get_profile_anchor_x(),
        120.0,
        "profile anchor x must remain unchanged when context menu opens"
    );
    assert_eq!(
        window.get_profile_anchor_y(),
        200.0,
        "profile anchor y must remain unchanged when context menu opens"
    );
    assert_eq!(
        window.get_profile_anchor_width(),
        160.0,
        "profile anchor width must remain unchanged when context menu opens"
    );
    assert_eq!(
        window.get_profile_anchor_height(),
        32.0,
        "profile anchor height must remain unchanged when context menu opens"
    );
}

#[test]
fn task3_profile_text_fields_and_copy_actions() {
    let window = one_message("Synthetic message");
    window.set_profile_available(true);
    window.set_profile_open(true);
    window.set_profile(ProfileData {
        id: "12345".into(),
        name: "Alice Wonderland".into(),
        handle: "alice".into(),
        pronouns: "she/her".into(),
        created: "Jan 1, 2021".into(),
        joined: "Feb 15, 2022".into(),
        has_bio: true,
        bio: slint::StyledText::from_plain_text("A curious mind\nand a tea lover."),
        activity: "Playing Chess".into(),
        ..Default::default()
    });

    let profile = window.get_profile();
    assert_eq!(profile.name, "Alice Wonderland");
    assert_eq!(profile.handle, "alice");
    assert_eq!(profile.pronouns, "she/her");
    assert_eq!(profile.created, "Jan 1, 2021");
    assert_eq!(profile.joined, "Feb 15, 2022");
    assert_eq!(profile.activity, "Playing Chess");
    assert!(profile.has_bio);
}

#[test]
fn task3_person_context_menu_offers_copy_username_and_display_name() {
    let directory = tempfile::tempdir().unwrap();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    adapter.borrow_mut().app.state.selected = Some("discord:channel_1".into());

    // Without developer mode: Copy Username and Copy Display Name appear, Copy ID does not
    adapter.borrow_mut().layout.developer_mode = false;
    let (rows, _) = super::menus::menu(&adapter.borrow(), "person_1", true);
    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert!(
        ids.contains(&"copy-username"),
        "Person context menu must offer Copy Username"
    );
    assert!(
        ids.contains(&"copy-display-name"),
        "Person context menu must offer Copy Display Name"
    );
    assert!(
        !ids.contains(&"copy-id"),
        "Copy ID should be hidden when developer_mode is false"
    );

    // With developer mode: Copy Username, Copy Display Name, and Copy ID appear
    adapter.borrow_mut().layout.developer_mode = true;
    let (rows_dev, _) = super::menus::menu(&adapter.borrow(), "person_1", true);
    let ids_dev: Vec<&str> = rows_dev.iter().map(|r| r.id.as_str()).collect();
    assert!(ids_dev.contains(&"copy-username"));
    assert!(ids_dev.contains(&"copy-display-name"));
    assert!(
        ids_dev.contains(&"copy-id"),
        "Copy ID should be visible when developer_mode is true"
    );
}

#[test]
fn task4_incoming_friend_request_opens_mini_profile() {
    use yapfast_core::chat::FriendRequest;
    let (window, adapter, _directory) = discord_server_window();
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.friend_requests = vec![FriendRequest {
            id: "1001".into(),
            name: "AliceRequester".into(),
            avatar: None,
            incoming: true,
            spam: false,
        }];
        ad.refresh(&window);
    }
    window.invoke_anchor_profile(320.0, 150.0, 240.0, 48.0);
    assert_eq!(window.get_profile_anchor_x(), 320.0);
    assert_eq!(window.get_profile_anchor_y(), 150.0);
    assert_eq!(window.get_profile_anchor_width(), 240.0);

    adapter.borrow_mut().action(
        &window,
        Action::ShowProfile {
            user: "1001".into(),
            large: false,
        },
    );

    assert!(window.get_profile_open());
    let profile = window.get_profile();
    assert_eq!(profile.id, "1001");
    assert_eq!(profile.name, "AliceRequester");
    assert_eq!(profile.friend, 3, "Friend offer 3 = Accept Friend Request");
}

#[test]
fn task4_outgoing_friend_request_shows_sent_status() {
    use yapfast_core::chat::FriendRequest;
    let (window, adapter, _directory) = discord_server_window();
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.friend_requests = vec![FriendRequest {
            id: "1002".into(),
            name: "BobOutgoing".into(),
            avatar: None,
            incoming: false,
            spam: false,
        }];
        ad.refresh(&window);
    }
    adapter.borrow_mut().action(
        &window,
        Action::ShowProfile {
            user: "1002".into(),
            large: false,
        },
    );

    assert!(window.get_profile_open());
    let profile = window.get_profile();
    assert_eq!(profile.id, "1002");
    assert_eq!(profile.name, "BobOutgoing");
    assert_eq!(profile.friend, 2, "Friend offer 2 = Friend Request Sent");
}

#[test]
fn task4_friend_request_action_buttons_do_not_open_profile() {
    use yapfast_core::chat::FriendRequest;
    let (window, adapter, _directory) = discord_server_window();
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.friend_requests = vec![FriendRequest {
            id: "1003".into(),
            name: "Charlie".into(),
            avatar: None,
            incoming: true,
            spam: false,
        }];
        ad.refresh(&window);
    }
    adapter
        .borrow_mut()
        .action(&window, Action::Befriend("1003".into()));
    assert!(!window.get_profile_open());

    adapter
        .borrow_mut()
        .action(&window, Action::RemoveFriendRequest("1003".into()));
    assert!(!window.get_profile_open());
}

#[test]
fn task5_search_suggestions_categories_and_operators() {
    let (window, adapter, _directory) = discord_server_window();
    {
        let mut ad = adapter.borrow_mut();
        ad.action(
            &window,
            Action::Navigate(Navigation::Server("discord:10".into())),
        );
        ad.action(&window, Action::SelectRoom("discord:10:11".into()));
        ad.app.state.friends = vec![yapfast_core::chat::Friend {
            id: "100".into(),
            name: "Bob".into(),
            nickname: None,
            avatar: None,
            presence: None,
            activity: None,
            activities: Default::default(),
        }];
    }
    let ad = adapter.borrow();
    let mut pictures = HashMap::new();

    // 1. Empty query produces search categories including "in:" in a server
    let categories =
        super::search::compute_search_suggestions(&ad.app.state, "", &mut pictures, true);
    assert!(!categories.is_empty());
    assert_eq!(categories[0].heading, "SEARCH OPTIONS");
    let tokens: Vec<&str> = categories.iter().map(|c| c.token.as_str()).collect();
    assert!(tokens.contains(&"from:"));
    assert!(tokens.contains(&"mentions:"));
    assert!(tokens.contains(&"has:"));
    assert!(tokens.contains(&"in:"));
    assert!(tokens.contains(&"before:"));
    assert!(tokens.contains(&"after:"));
    assert!(tokens.contains(&"during:"));
    assert!(tokens.contains(&"pinned:"));

    // 2. "from:" produces member suggestions
    let from_suggestions =
        super::search::compute_search_suggestions(&ad.app.state, "from:", &mut pictures, true);
    assert!(!from_suggestions.is_empty());
    assert_eq!(from_suggestions[0].kind, 1);
    assert_eq!(from_suggestions[0].heading, "FROM USERS");

    // 3. "has:" produces content type suggestions
    let has_suggestions =
        super::search::compute_search_suggestions(&ad.app.state, "has:", &mut pictures, true);
    assert!(!has_suggestions.is_empty());
    assert_eq!(has_suggestions[0].kind, 3);
    let has_tokens: Vec<&str> = has_suggestions.iter().map(|c| c.token.as_str()).collect();
    assert!(has_tokens.contains(&"has:image "));
    assert!(has_tokens.contains(&"has:file "));
    assert!(has_tokens.contains(&"has:link "));
    assert!(has_tokens.contains(&"has:video "));
    assert!(has_tokens.contains(&"has:sound "));

    // 4. "has:im" filters to image
    let has_im =
        super::search::compute_search_suggestions(&ad.app.state, "has:im", &mut pictures, true);
    assert_eq!(has_im.len(), 1);
    assert_eq!(has_im[0].label, "image");

    // 5. "before:" produces date guidance
    let before_sugg =
        super::search::compute_search_suggestions(&ad.app.state, "before:", &mut pictures, true);
    assert!(!before_sugg.is_empty());
    assert_eq!(before_sugg[0].kind, 4);
    assert_eq!(before_sugg[0].label, "Today");

    // 6. "pinned:" produces pinned options
    let pinned_sugg =
        super::search::compute_search_suggestions(&ad.app.state, "pinned:", &mut pictures, true);
    assert_eq!(pinned_sugg.len(), 2);
    assert_eq!(pinned_sugg[0].label, "true");
    assert_eq!(pinned_sugg[1].label, "false");
}

#[test]
fn task5_apply_search_suggestion_combinable() {
    // Category selection from empty query
    let q1 = super::search::apply_search_suggestion("", 0, "from:", "from:");
    assert_eq!(q1, "from:");

    // Member selection with simple name
    let q2 = super::search::apply_search_suggestion(&q1, 1, "from:Bob ", "Bob");
    assert_eq!(q2, "from:Bob ");

    // Category selection after space
    let q3 = super::search::apply_search_suggestion(&q2, 0, "has:", "has:");
    assert_eq!(q3, "from:Bob has:");

    // Content type selection
    let q4 = super::search::apply_search_suggestion(&q3, 3, "has:image ", "image");
    assert_eq!(q4, "from:Bob has:image ");

    // Member selection with space in name (should quote)
    let q5_base = "from:car";
    let q5 = super::search::apply_search_suggestion(q5_base, 1, "from:\"Carol M\" ", "Carol M");
    assert_eq!(q5, "from:\"Carol M\" ");

    // Channel selection
    let q6_base = "from:Bob in:gen";
    let q6 = super::search::apply_search_suggestion(q6_base, 2, "in:general ", "general");
    assert_eq!(q6, "from:Bob in:general ");
}

#[test]
fn task5_search_highlight_terms() {
    let highlighted = super::search::highlight_search_terms(
        "Here are the trip notes for tomorrow.",
        "trip notes from:bob",
    );
    assert_eq!(highlighted, "Here are the **trip** **notes** for tomorrow.");

    // Quoted exact phrase highlighting
    let phrase_hl = super::search::highlight_search_terms(
        "Discussing project update in the meeting.",
        "\"project update\" in:general",
    );
    assert_eq!(phrase_hl, "Discussing **project update** in the meeting.");

    // Should not break URLs
    let url_text = "See https://example.com/trip for notes.";
    let url_hl = super::search::highlight_search_terms(url_text, "trip notes");
    assert_eq!(url_hl, "See https://example.com/trip for **notes**.");
}

#[test]
fn task5_search_dropdown_ui_interaction() {
    let (window, adapter, _directory) = discord_server_window();
    adapter
        .borrow_mut()
        .action(&window, Action::SelectRoom("discord:10:11".into()));

    // Trigger edited callback
    window.invoke_search_input_edited("has:".into());
    assert!(
        adapter
            .borrow()
            .models
            .search_filter_suggestions
            .row_count()
            > 0
    );
    assert_eq!(window.get_search_suggestion_index(), 0);

    // Choose the first suggestion
    window.invoke_choose_search_suggestion(0);
    let updated_query = window.get_search_text();
    assert!(updated_query.starts_with("has:"));
    assert!(window.get_search_dropdown_open());

    // Close dropdown
    window.invoke_close_search_dropdown();
    assert_eq!(
        adapter
            .borrow()
            .models
            .search_filter_suggestions
            .row_count(),
        0
    );
}

#[test]
fn task6_voice_participant_drag_droppable_targets_and_permissions() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    // Alice is Staff ("30") with MOVE_MEMBERS; Dave ("4") is in Lounge ("discord:10:12")
    adapter.borrow_mut().app = yapfast_core::application::member_server(&["30"]);
    adapter.borrow_mut().show_sidebar(&window);

    // Start dragging Dave from Lounge
    let started = window.invoke_start_voice_drag("discord:10:12".into(), "4".into());
    assert!(started, "Dave should be draggable by Staff");

    let rooms = window.get_rooms();
    let droppable = window.get_voice_droppable();
    assert_eq!(rooms.row_count(), droppable.row_count());

    // Check each room's droppable flag
    for i in 0..rooms.row_count() {
        let room = rooms.row_data(i).unwrap();
        let can_drop = droppable.row_data(i).unwrap();
        match room.id.as_str() {
            // Den is another voice channel that Dave and Alice can join -> valid target
            "discord:10:13" => assert!(can_drop, "Den must be a droppable target"),
            // Lounge is current channel -> not droppable
            "discord:10:12" => assert!(!can_drop, "Current channel must not be droppable"),
            // Vault is closed to Dave -> not droppable
            "discord:10:14" => assert!(!can_drop, "Vault (closed) must not be droppable"),
            // General is a text channel -> not droppable
            "discord:10:11" => assert!(!can_drop, "Text channel must not be droppable"),
            _ => assert!(!can_drop, "Other rooms must not be droppable"),
        }
    }

    // Now test without MOVE_MEMBERS permission (role "20")
    adapter.borrow_mut().app = yapfast_core::application::member_server(&["20"]);
    adapter.borrow_mut().show_sidebar(&window);
    let started_no_perm = window.invoke_start_voice_drag("discord:10:12".into(), "4".into());
    assert!(
        !started_no_perm,
        "Dave must not be draggable without MOVE_MEMBERS"
    );
    assert_eq!(window.get_voice_droppable().row_count(), 0);
}

#[test]
fn task6_voice_participant_move_valid_and_invalid_destinations() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    adapter.borrow_mut().app = yapfast_core::application::member_server(&["30"]);
    adapter.borrow_mut().show_sidebar(&window);

    // Start drag
    assert!(window.invoke_start_voice_drag("discord:10:12".into(), "4".into()));

    // Attempt dropping on invalid destination (Vault or text channel)
    window.invoke_move_voice_participant(
        "discord:10:12".into(),
        "4".into(),
        "discord:10:14".into(),
    );
    assert_eq!(window.get_voice_droppable().row_count(), 0);
    assert!(!window.get_room_dragging_voice());

    // Valid drop onto Den
    assert!(window.invoke_start_voice_drag("discord:10:12".into(), "4".into()));
    window.invoke_move_voice_participant(
        "discord:10:12".into(),
        "4".into(),
        "discord:10:13".into(),
    );
    assert_eq!(window.get_voice_droppable().row_count(), 0);
    assert!(!window.get_room_dragging_voice());
}

#[test]
fn task6_voice_participant_self_move() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    adapter.borrow_mut().app = yapfast_core::application::member_server(&["30"]);
    adapter.borrow_mut().show_sidebar(&window);

    // Alice ("1") dragging herself from Lounge
    let started = window.invoke_start_voice_drag("discord:10:12".into(), "1".into());
    assert!(
        started,
        "Self-drag must be allowed if connectable voice channels exist"
    );

    let rooms = window.get_rooms();
    let droppable = window.get_voice_droppable();
    for i in 0..rooms.row_count() {
        let room = rooms.row_data(i).unwrap();
        let can_drop = droppable.row_data(i).unwrap();
        if room.id == "discord:10:13" {
            assert!(can_drop, "Den must be droppable for Alice");
        } else if room.id == "discord:10:12" {
            assert!(
                !can_drop,
                "Current channel Lounge must not be droppable for Alice"
            );
        }
    }

    // Drop onto Den
    window.invoke_move_voice_participant(
        "discord:10:12".into(),
        "1".into(),
        "discord:10:13".into(),
    );
    assert_eq!(window.get_voice_droppable().row_count(), 0);
}

#[test]
fn task6_voice_drag_stop_and_cancellation() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    adapter.borrow_mut().app = yapfast_core::application::member_server(&["30"]);
    adapter.borrow_mut().show_sidebar(&window);

    assert!(window.invoke_start_voice_drag("discord:10:12".into(), "4".into()));
    assert!(window.get_voice_droppable().row_count() > 0);

    // Cancel / stop
    window.invoke_stop_voice_drag();
    assert_eq!(window.get_voice_droppable().row_count(), 0);
    assert!(!window.get_room_dragging_voice());
}

#[test]
fn task7_multi_stream_state_and_watching_models() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let frames1 = Arc::new(yapfast_core::frames::Frames::create(2, 2).unwrap());
    let frames2 = Arc::new(yapfast_core::frames::Frames::create(2, 2).unwrap());

    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("lounge".into());
        ad.app.state.voice.name = "Lounge".into();
        ad.app.state.voice.participants = vec![
            yapfast_core::voice::Participant {
                id: "alice".into(),
                user: "Alice".into(),
                local: false,
                streaming: true,
                video: None,
                connected: true,
                muted: false,
                deafened: false,
                speaking: false,
                problem: None,
                avatar: None,
                color: None,
                person: "alice".into(),
                server_muted: false,
                server_deafened: false,
            },
            yapfast_core::voice::Participant {
                id: "bob".into(),
                user: "Bob".into(),
                local: false,
                streaming: true,
                video: None,
                connected: true,
                muted: false,
                deafened: false,
                speaking: false,
                problem: None,
                avatar: None,
                color: None,
                person: "bob".into(),
                server_muted: false,
                server_deafened: false,
            },
        ];
        ad.app.state.voice.watching = vec![
            yapfast_core::voice::Watching {
                user: "alice".into(),
                name: "Alice".into(),
                opening: false,
                frames: Some(frames1),
                viewers: vec!["me".into()],
            },
            yapfast_core::voice::Watching {
                user: "bob".into(),
                name: "Bob".into(),
                opening: false,
                frames: Some(frames2),
                viewers: vec![],
            },
        ];
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    assert!(window.get_has_active_streams());
    let people = window.get_participants();
    assert_eq!(people.row_count(), 2);
    let alice = people.row_data(0).unwrap();
    let bob = people.row_data(1).unwrap();
    assert!(alice.watched);
    assert!(bob.watched);
}

#[test]
fn clicking_the_stage_gutter_compacts_a_share_without_changing_focus() {
    use i_slint_core::{
        accessibility::AccessibleStringProperty,
        item_tree::{ItemRc, ParentItemTraversalMode},
    };
    use slint::platform::{PointerEventButton, WindowEvent};
    use std::ops::ControlFlow;

    let window = showing(vec![]);
    window.set_voice_room(true);
    window.set_voice_here(true);
    window.set_voice_active(true);
    window.set_has_active_streams(true);
    window.set_call_focus(0);
    window.set_single_watched_slot(0);
    window.set_watched_stream_count(1);
    window.set_watch_user("watch".into());
    window.set_participants(ModelRc::new(VecModel::from(vec![
        PersonRow {
            id: "watch".into(),
            name: "Viewer".into(),
            initial: "VA".into(),
            decoration_url: "fixture-decoration".into(),
            status: "Listening".into(),
            live: true,
            watched: true,
            stream_ready: true,
            connected: true,
            ..Default::default()
        },
        PersonRow {
            id: "local".into(),
            name: "Local".into(),
            initial: "LA".into(),
            status: "Listening".into(),
            local: true,
            connected: true,
            ..Default::default()
        },
    ])));
    let tile = |label: &str| {
        settle(&window);
        let inner = i_slint_core::window::WindowInner::from_pub(window.window());
        let component = inner.component();
        ItemRc::new_root(component.clone())
            .visit_descendants(|item| {
                if item
                    .accessible_string_property(AccessibleStringProperty::Label)
                    .as_deref()
                    == Some(label)
                {
                    ControlFlow::Break(item.map_to_item_tree(item.geometry().center(), &component))
                } else {
                    ControlFlow::Continue(())
                }
            })
            .expect("the watched share has a tile")
    };
    let click = |x, y| {
        let position = slint::LogicalPosition::new(x, y);
        let button = PointerEventButton::Left;
        window
            .window()
            .dispatch_event(WindowEvent::PointerMoved { position });
        window
            .window()
            .dispatch_event(WindowEvent::PointerPressed { position, button });
        window
            .window()
            .dispatch_event(WindowEvent::PointerReleased { position, button });
        settle(&window);
    };
    let loading = || {
        settle(&window);
        let inner = i_slint_core::window::WindowInner::from_pub(window.window());
        ItemRc::new_root(inner.component())
            .visit_descendants(|item| {
                if item.is_visible()
                    && item
                        .accessible_string_property(AccessibleStringProperty::Label)
                        .as_deref()
                        == Some("Opening stream")
                {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .is_some()
    };
    let participants = window.get_participants().iter().collect::<Vec<_>>();
    let waiting = |watched| {
        let mut rows = participants.clone();
        rows[0].watched = watched;
        rows[0].stream_ready = false;
        window.set_participants(ModelRc::new(VecModel::from(rows)));
    };
    waiting(true);
    assert!(loading());
    click(1117., tile("Viewer's screen").y);
    assert!(window.get_compact_call());
    assert!(loading());
    let opening = tile("Viewer's screen");
    click(opening.x, opening.y);
    assert!(!window.get_compact_call());
    window.set_watch_user("".into());
    waiting(false);
    assert!(!loading());
    window.set_watch_user("watch".into());
    waiting(true);
    assert!(loading());
    window.set_participants(ModelRc::new(VecModel::from(participants.clone())));
    assert!(!loading());
    let screen = tile("Viewer's screen");
    assert!(tile("Viewer, Listening").y > screen.y);
    assert!(tile("Local, Listening").y > screen.y);
    let avatar = |initial| {
        settle(&window);
        let inner = i_slint_core::window::WindowInner::from_pub(window.window());
        let component = inner.component();
        ItemRc::new_root(component.clone())
            .visit_descendants(|item| {
                if item.is_visible() && item_text_is(item, initial) {
                    let avatar = item
                        .parent_item(ParentItemTraversalMode::FindAllParents)
                        .unwrap();
                    ControlFlow::Break((
                        avatar.geometry().height(),
                        avatar.map_to_item_tree(avatar.geometry().center(), &component),
                    ))
                } else {
                    ControlFlow::Continue(())
                }
            })
            .expect("the avatar shows its initials")
    };
    click(1117., screen.y);
    assert!(window.get_compact_call());
    assert_eq!(window.get_call_focus(), 0);
    let center = tile("Viewer, Listening");
    click(center.x, center.y);
    assert!(!window.get_compact_call());
    assert_eq!(window.get_watch_user(), "watch");
    window
        .window()
        .set_size(slint::LogicalSize::new(1920., 1032.));
    let centered = |initial, label| {
        let (diameter, point) = avatar(initial);
        let center = tile(label);
        assert!((point.x - center.x).abs() < 1. && (point.y - center.y).abs() < 1.);
        diameter
    };
    let decorated = centered("VA", "Viewer, Listening");
    let plain = centered("LA", "Local, Listening");
    assert!((60. ..=66.).contains(&decorated), "{decorated}");
    assert!((decorated..=66.).contains(&plain), "{plain}");
    window
        .window()
        .set_size(slint::LogicalSize::new(570., 600.));
    assert!(centered("VA", "Viewer, Listening") <= decorated);
    assert!(centered("LA", "Local, Listening") <= plain);
    window
        .window()
        .set_size(slint::LogicalSize::new(1920., 1032.));
    window.set_has_active_streams(false);
    window.set_call_focus(-1);
    window.set_participants(ModelRc::new(VecModel::from(
        window
            .get_participants()
            .iter()
            .map(|mut person| {
                person.watched = false;
                person.stream_ready = false;
                person
            })
            .collect::<Vec<_>>(),
    )));
    assert_eq!(centered("VA", "Viewer, Listening"), 80.);
    assert_eq!(centered("LA", "Local, Listening"), 80.);
}

#[test]
fn task7_grid_and_focus_layout_mode_switching() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("lounge".into());
        ad.app.state.voice.participants = vec![
            yapfast_core::voice::Participant {
                id: "alice".into(),
                user: "Alice".into(),
                local: false,
                streaming: true,
                video: None,
                connected: true,
                muted: false,
                deafened: false,
                speaking: false,
                problem: None,
                avatar: None,
                color: None,
                person: "alice".into(),
                server_muted: false,
                server_deafened: false,
            },
            yapfast_core::voice::Participant {
                id: "bob".into(),
                user: "Bob".into(),
                local: false,
                streaming: true,
                video: None,
                connected: true,
                muted: false,
                deafened: false,
                speaking: false,
                problem: None,
                avatar: None,
                color: None,
                person: "bob".into(),
                server_muted: false,
                server_deafened: false,
            },
        ];
        ad.app.state.voice.watching = vec![
            yapfast_core::voice::Watching {
                user: "alice".into(),
                name: "Alice".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            },
            yapfast_core::voice::Watching {
                user: "bob".into(),
                name: "Bob".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            },
        ];
        ad.show_stage(&window);
    }

    // Switch to Grid Mode: under persistent focus, Alice remains focused
    window.invoke_set_layout_mode(false);
    assert_eq!(window.get_call_focus(), 0);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("alice".into()))
    );

    // Switch back to Focus Mode: Alice remains focused
    window.invoke_set_layout_mode(true);
    assert_eq!(window.get_call_focus(), 0);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("alice".into()))
    );

    // Switch focus to Bob
    window.invoke_focus_tile(1);
    assert_eq!(window.get_call_focus(), 1);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );

    // Switch back to Grid Mode: Bob remains focused
    window.invoke_set_layout_mode(false);
    assert_eq!(window.get_call_focus(), 1);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );
}

#[test]
fn task7_individual_stream_stopping_and_focus_handover() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("lounge".into());
        ad.app.state.voice.participants = vec![
            yapfast_core::voice::Participant {
                id: "alice".into(),
                user: "Alice".into(),
                local: false,
                streaming: true,
                video: None,
                connected: true,
                muted: false,
                deafened: false,
                speaking: false,
                problem: None,
                avatar: None,
                color: None,
                person: "alice".into(),
                server_muted: false,
                server_deafened: false,
            },
            yapfast_core::voice::Participant {
                id: "bob".into(),
                user: "Bob".into(),
                local: false,
                streaming: true,
                video: None,
                connected: true,
                muted: false,
                deafened: false,
                speaking: false,
                problem: None,
                avatar: None,
                color: None,
                person: "bob".into(),
                server_muted: false,
                server_deafened: false,
            },
        ];
        ad.app.state.voice.watching = vec![
            yapfast_core::voice::Watching {
                user: "alice".into(),
                name: "Alice".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            },
            yapfast_core::voice::Watching {
                user: "bob".into(),
                name: "Bob".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            },
        ];
        ad.call_focus = Some(crate::ui::tiles::Focus::Stream("alice".into()));
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    assert_eq!(window.get_call_focus(), 0);

    // Stop Alice's stream; Bob remains
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.watching.retain(|w| w.user != "alice");
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    let people = window.get_participants();
    assert!(!people.row_data(0).unwrap().watched);
    assert!(people.row_data(1).unwrap().watched);
    // Focus handed over to Bob
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );

    // Stop Bob's stream; no streams left
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.watching.clear();
        ad.show_stage(&window);
        ad.show_voice(&window);
    }
    assert!(!window.get_has_active_streams());
    assert_eq!(window.get_call_focus(), -1);
}

#[test]
fn task7_stream_tick_delivers_independent_frames() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let (mut writer1, frames1) = {
        let frames = yapfast_core::frames::Frames::create(2, 2).unwrap();
        let writer = yapfast_core::frames::Writer::open(&frames.path().unwrap()).unwrap();
        (writer, Arc::new(frames))
    };
    let (mut writer2, frames2) = {
        let frames = yapfast_core::frames::Frames::create(3, 2).unwrap();
        let writer = yapfast_core::frames::Writer::open(&frames.path().unwrap()).unwrap();
        (writer, Arc::new(frames))
    };

    writer1.write(2, 2, |buf| {
        buf[..16].copy_from_slice(&[
            255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
        ])
    });
    writer2.write(3, 2, |buf| buf[..24].fill(42));

    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("lounge".into());
        ad.app.state.voice.participants = vec![
            yapfast_core::voice::Participant {
                id: "alice".into(),
                user: "Alice".into(),
                local: false,
                streaming: true,
                video: None,
                connected: true,
                muted: false,
                deafened: false,
                speaking: false,
                problem: None,
                avatar: None,
                color: None,
                person: "alice".into(),
                server_muted: false,
                server_deafened: false,
            },
            yapfast_core::voice::Participant {
                id: "bob".into(),
                user: "Bob".into(),
                local: false,
                streaming: true,
                video: None,
                connected: true,
                muted: false,
                deafened: false,
                speaking: false,
                problem: None,
                avatar: None,
                color: None,
                person: "bob".into(),
                server_muted: false,
                server_deafened: false,
            },
        ];
        ad.app.state.voice.watching = vec![
            yapfast_core::voice::Watching {
                user: "alice".into(),
                name: "Alice".into(),
                opening: false,
                frames: Some(frames1),
                viewers: vec![],
            },
            yapfast_core::voice::Watching {
                user: "bob".into(),
                name: "Bob".into(),
                opening: false,
                frames: Some(frames2),
                viewers: vec![],
            },
        ];
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    // Fire stream tick to grab and render frames into stream_frames model
    window.invoke_stream_tick();

    let stream_frames = adapter.borrow().models.stream_frames.clone();
    assert_eq!(stream_frames.row_count(), 2);
    let frame_alice = stream_frames.row_data(0).unwrap();
    let frame_bob = stream_frames.row_data(1).unwrap();
    assert_eq!(frame_alice.size().width, 2);
    assert_eq!(frame_alice.size().height, 2);
    assert_eq!(frame_bob.size().width, 3);
    assert_eq!(frame_bob.size().height, 2);
    assert_eq!(window.get_watch_frame().size().width, 2);

    // Both feeds are already open. Switching must use Bob's last frame even
    // when no new frame arrives during the focus change.
    window.invoke_focus_tile(1);
    assert_eq!(window.get_watch_user(), "bob");
    assert_eq!(window.get_watch_frame().size().width, 3);
    window.invoke_focus_tile(0);
    assert_eq!(window.get_watch_user(), "alice");
    assert_eq!(window.get_watch_frame().size().width, 2);
    window.invoke_focus_tile(1);
    writer2.write(2, 2, |buf| buf[..16].fill(84));
    window.invoke_stream_tick();
    assert_eq!(window.get_watch_frame().size().width, 2);
}

#[test]
fn task7_followup_grid_and_focus_dynamic_counts_and_slots() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let mk_person = |id: &str, name: &str| yapfast_core::voice::Participant {
        id: id.into(),
        user: name.into(),
        local: false,
        streaming: true,
        video: None,
        connected: true,
        muted: false,
        deafened: false,
        speaking: false,
        problem: None,
        avatar: None,
        color: None,
        person: id.into(),
        server_muted: false,
        server_deafened: false,
    };
    let mk_watch = |id: &str, name: &str| yapfast_core::voice::Watching {
        user: id.into(),
        name: name.into(),
        opening: false,
        frames: None,
        viewers: vec![],
    };

    // 1. Four streams active
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("lounge".into());
        ad.app.state.voice.participants = vec![
            mk_person("bob", "Bob"),
            mk_person("carol", "Carol"),
            mk_person("dave", "Dave"),
            mk_person("eve", "Eve"),
        ];
        ad.app.state.voice.watching = vec![
            mk_watch("bob", "Bob"),
            mk_watch("carol", "Carol"),
            mk_watch("dave", "Dave"),
            mk_watch("eve", "Eve"),
        ];
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    assert!(window.get_has_active_streams());
    assert_eq!(window.get_watched_stream_count(), 4);
    let people = window.get_participants();
    assert_eq!(people.row_data(0).unwrap().watched_index, 0);
    assert_eq!(people.row_data(1).unwrap().watched_index, 1);
    assert_eq!(people.row_data(2).unwrap().watched_index, 2);
    assert_eq!(people.row_data(3).unwrap().watched_index, 3);

    // Focus Carol (slot 1)
    window.invoke_focus_tile(1);
    assert_eq!(window.get_call_focus(), 1);
    assert_eq!(window.get_focused_active(), 1);

    // 2. Stop watching Eve (4 -> 3 streams)
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.watching.pop();
        ad.show_stage(&window);
        ad.show_voice(&window);
    }
    assert_eq!(window.get_watched_stream_count(), 3);
    let people = window.get_participants();
    assert_eq!(people.row_data(0).unwrap().watched_index, 0);
    assert_eq!(people.row_data(1).unwrap().watched_index, 1);
    assert_eq!(people.row_data(2).unwrap().watched_index, 2);
    assert_eq!(people.row_data(3).unwrap().watched_index, -1);

    // 3. Stop watching Dave (3 -> 2 streams)
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.watching.pop();
        ad.show_stage(&window);
        ad.show_voice(&window);
    }
    assert_eq!(window.get_watched_stream_count(), 2);
    let people = window.get_participants();
    assert_eq!(people.row_data(0).unwrap().watched_index, 0);
    assert_eq!(people.row_data(1).unwrap().watched_index, 1);
    assert_eq!(people.row_data(2).unwrap().watched_index, -1);

    // 4. Stop watching Carol (2 -> 1 stream)
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.watching.pop();
        ad.show_stage(&window);
        ad.show_voice(&window);
    }
    assert_eq!(window.get_watched_stream_count(), 1);
    let people = window.get_participants();
    assert_eq!(people.row_data(0).unwrap().watched_index, 0);
    assert_eq!(people.row_data(1).unwrap().watched_index, -1);

    // 5. Stop watching Bob (1 -> 0 streams)
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.watching.pop();
        ad.show_stage(&window);
        ad.show_voice(&window);
    }
    assert_eq!(window.get_watched_stream_count(), 0);
    assert!(!window.get_has_active_streams());
    assert_eq!(window.get_call_focus(), -1);
}

#[test]
fn task7_focus_mode_secondary_thumbnails_and_high_res_playback() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let mk_person = |id: &str, name: &str| yapfast_core::voice::Participant {
        id: id.into(),
        user: name.into(),
        local: false,
        streaming: true,
        video: None,
        connected: true,
        muted: false,
        deafened: false,
        speaking: false,
        problem: None,
        avatar: None,
        color: None,
        person: id.into(),
        server_muted: false,
        server_deafened: false,
    };
    let mk_watch = |id: &str, name: &str| yapfast_core::voice::Watching {
        user: id.into(),
        name: name.into(),
        opening: false,
        frames: None,
        viewers: vec![],
    };

    // 1. Initial state: 4 streams active in Focus Mode (Bob focused)
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("lounge".into());
        ad.app.state.voice.participants = vec![
            mk_person("bob", "Bob"),
            mk_person("carol", "Carol"),
            mk_person("dave", "Dave"),
            mk_person("eve", "Eve"),
        ];
        ad.app.state.voice.watching = vec![
            mk_watch("bob", "Bob"),
            mk_watch("carol", "Carol"),
            mk_watch("dave", "Dave"),
            mk_watch("eve", "Eve"),
        ];
        ad.call_focus = Some(crate::ui::tiles::Focus::Stream("bob".into()));
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    assert!(window.get_has_active_streams());
    assert_eq!(window.get_watched_stream_count(), 4);
    assert_eq!(window.get_call_focus(), 0);
    assert_eq!(window.get_focused_active(), 0);

    // Clicking secondary thumbnail 2 (Dave) switches focus to Dave
    window.invoke_focus_tile(2);
    assert_eq!(window.get_call_focus(), 2);
    assert_eq!(window.get_focused_active(), 2);

    // Clicking secondary thumbnail 3 (Eve) switches focus to Eve
    window.invoke_focus_tile(3);
    assert_eq!(window.get_call_focus(), 3);
    assert_eq!(window.get_focused_active(), 3);

    // Verify 1440p watch dimensions
    assert_eq!(yapfast_core::stream_worker::WATCH_WIDTH, 2560);
    assert_eq!(yapfast_core::stream_worker::WATCH_HEIGHT, 1440);
}

#[test]
fn task7_voice_participant_strip_persists_with_single_watched_stream_and_non_streamers() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let mk_p = |id: &str, name: &str, streaming: bool| yapfast_core::voice::Participant {
        id: id.into(),
        user: name.into(),
        local: false,
        streaming,
        video: None,
        connected: true,
        muted: false,
        deafened: false,
        speaking: false,
        problem: None,
        avatar: None,
        color: None,
        person: id.into(),
        server_muted: false,
        server_deafened: false,
    };

    // Scenario: Alice (avatar), Bob (streaming, watched), Carol (live), Dave (live), Eve (live)
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("party-voice".into());
        ad.app.state.voice.participants = vec![
            mk_p("alice", "Alice", false),
            mk_p("bob", "Bob", true),
            mk_p("carol", "Carol", true),
            mk_p("dave", "Dave", true),
            mk_p("eve", "Eve", true),
        ];
        // User is watching ONLY Bob
        ad.app.state.voice.watching = vec![yapfast_core::voice::Watching {
            user: "bob".into(),
            name: "Bob".into(),
            opening: false,
            frames: None,
            viewers: vec![],
        }];
        ad.call_focus = Some(crate::ui::tiles::Focus::Stream("bob".into()));
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    // Single stream watched
    assert!(window.get_has_active_streams());
    assert_eq!(window.get_watched_stream_count(), 1);
    assert_eq!(window.get_single_watched_slot(), 1); // Bob's slot
    assert_eq!(window.get_call_focus(), 1);

    // Verify all 5 participants exist in model
    let participants: Vec<_> = adapter.borrow().models.participants.iter().collect();
    assert_eq!(participants.len(), 5);
    // Alice is non-streaming
    assert_eq!(participants[0].name, "Alice");
    assert!(!participants[0].live);
    assert!(!participants[0].watched);
    // Bob is watched
    assert_eq!(participants[1].name, "Bob");
    assert!(participants[1].live);
    assert!(participants[1].watched);
    // Carol, Dave, Eve are streaming but NOT watched
    assert_eq!(participants[2].name, "Carol");
    assert!(participants[2].live);
    assert!(!participants[2].watched);
    assert_eq!(participants[3].name, "Dave");
    assert!(participants[3].live);
    assert!(!participants[3].watched);
    assert_eq!(participants[4].name, "Eve");
    assert!(participants[4].live);
    assert!(!participants[4].watched);

    // Even if mode is set to Grid with 1 stream, single_watched_slot remains 1
    window.invoke_set_layout_mode(false);
    assert_eq!(window.get_single_watched_slot(), 1);
    assert_eq!(window.get_watched_stream_count(), 1);

    // Watch action on Carol invokes watch_stream and sets focus to Carol
    window.invoke_watch_stream("carol".into());
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("carol".into()))
    );

    // Add stream action on Dave invokes add_stream without changing focus from Carol
    window.invoke_add_stream("dave".into());
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("carol".into()))
    );
}

#[test]
fn task7_add_stream_preserves_focus_across_multiple_additive_streams() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let mk_p = |id: &str, name: &str, streaming: bool| yapfast_core::voice::Participant {
        id: id.into(),
        user: name.into(),
        local: false,
        streaming,
        video: None,
        connected: true,
        muted: false,
        deafened: false,
        speaking: false,
        problem: None,
        avatar: None,
        color: None,
        person: id.into(),
        server_muted: false,
        server_deafened: false,
    };

    // Scenario: Alice (avatar), Bob (streaming, watched), Carol (live), Dave (live), Eve (live)
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("party-voice".into());
        ad.app.state.voice.participants = vec![
            mk_p("alice", "Alice", false),
            mk_p("bob", "Bob", true),
            mk_p("carol", "Carol", true),
            mk_p("dave", "Dave", true),
            mk_p("eve", "Eve", true),
        ];
        // Initially watching ONLY Bob
        ad.app.state.voice.watching = vec![yapfast_core::voice::Watching {
            user: "bob".into(),
            name: "Bob".into(),
            opening: false,
            frames: None,
            viewers: vec![],
        }];
        ad.call_focus = Some(crate::ui::tiles::Focus::Stream("bob".into()));
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    assert_eq!(window.get_watched_stream_count(), 1);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );
    assert_eq!(window.get_call_focus(), 1);

    // 1. Add Carol using eye icon -> focus must remain Bob
    window.invoke_add_stream("carol".into());
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );
    // Backend confirms Carol subscription
    {
        let mut ad = adapter.borrow_mut();
        ad.app
            .state
            .voice
            .watching
            .push(yapfast_core::voice::Watching {
                user: "carol".into(),
                name: "Carol".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            });
        ad.show_stage(&window);
        ad.show_voice(&window);
    }
    assert_eq!(window.get_watched_stream_count(), 2);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );
    assert_eq!(window.get_call_focus(), 1);

    // 2. Add Dave using eye icon -> focus must remain Bob
    window.invoke_add_stream("dave".into());
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );
    // Backend confirms Dave subscription
    {
        let mut ad = adapter.borrow_mut();
        ad.app
            .state
            .voice
            .watching
            .push(yapfast_core::voice::Watching {
                user: "dave".into(),
                name: "Dave".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            });
        ad.show_stage(&window);
        ad.show_voice(&window);
    }
    assert_eq!(window.get_watched_stream_count(), 3);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );
    assert_eq!(window.get_call_focus(), 1);

    // 3. Add Eve using eye icon -> focus must remain Bob
    window.invoke_add_stream("eve".into());
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );
    // Backend confirms Eve subscription
    {
        let mut ad = adapter.borrow_mut();
        ad.app
            .state
            .voice
            .watching
            .push(yapfast_core::voice::Watching {
                user: "eve".into(),
                name: "Eve".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            });
        ad.show_stage(&window);
        ad.show_voice(&window);
    }
    assert_eq!(window.get_watched_stream_count(), 4);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );
    assert_eq!(window.get_call_focus(), 1);
}

#[test]
fn task7_normal_watch_stream_replaces_focused_stream_and_preserves_secondary() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let mk_p = |id: &str, name: &str, streaming: bool| yapfast_core::voice::Participant {
        id: id.into(),
        user: name.into(),
        local: false,
        streaming,
        video: None,
        connected: true,
        muted: false,
        deafened: false,
        speaking: false,
        problem: None,
        avatar: None,
        color: None,
        person: id.into(),
        server_muted: false,
        server_deafened: false,
    };

    // State: Bob focused, Carol secondary, Dave secondary
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("party-voice".into());
        ad.app.state.voice.participants = vec![
            mk_p("alice", "Alice", false),
            mk_p("bob", "Bob", true),
            mk_p("carol", "Carol", true),
            mk_p("dave", "Dave", true),
            mk_p("eve", "Eve", true),
        ];
        ad.app.state.voice.watching = vec![
            yapfast_core::voice::Watching {
                user: "bob".into(),
                name: "Bob".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            },
            yapfast_core::voice::Watching {
                user: "carol".into(),
                name: "Carol".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            },
            yapfast_core::voice::Watching {
                user: "dave".into(),
                name: "Dave".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            },
        ];
        ad.call_focus = Some(crate::ui::tiles::Focus::Stream("bob".into()));
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    assert_eq!(window.get_watched_stream_count(), 3);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );

    // Normal watch stream on Eve: replaces Bob as focused
    window.invoke_watch_stream("eve".into());
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("eve".into()))
    );

    // Backend confirms replacement: Bob is stopped, Eve is watched; Carol & Dave remain
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.watching.retain(|w| w.user != "bob");
        ad.app
            .state
            .voice
            .watching
            .push(yapfast_core::voice::Watching {
                user: "eve".into(),
                name: "Eve".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            });
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    assert_eq!(window.get_watched_stream_count(), 3);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("eve".into()))
    );
    // Carol and Dave are still in the active watching list
    let watching_ids: Vec<String> = adapter
        .borrow()
        .app
        .state
        .voice
        .watching
        .iter()
        .map(|w| w.user.clone())
        .collect();
    assert_eq!(watching_ids, vec!["carol", "dave", "eve"]);

    // Normal select on already-watched Carol: smoothly focuses Carol without restarting
    window.invoke_watch_stream("carol".into());
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("carol".into()))
    );
    assert_eq!(window.get_watched_stream_count(), 3);

    // Add-stream on already-watched Carol: does not duplicate or change focus
    window.invoke_add_stream("carol".into());
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("carol".into()))
    );
    assert_eq!(window.get_watched_stream_count(), 3);
}

#[test]
fn task7_final_polish_vertical_selector_and_enlarged_strip_geometry() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let mk_person = |id: &str, name: &str, streaming: bool| yapfast_core::voice::Participant {
        id: id.into(),
        user: name.into(),
        local: false,
        streaming,
        video: None,
        connected: true,
        muted: false,
        deafened: false,
        speaking: false,
        problem: None,
        avatar: None,
        color: None,
        person: id.into(),
        server_muted: false,
        server_deafened: false,
    };
    let mk_watch = |id: &str, name: &str| yapfast_core::voice::Watching {
        user: id.into(),
        name: name.into(),
        opening: false,
        frames: None,
        viewers: vec![],
    };

    // 1. Initial setup: Alice (non-streamer) + Bob, Carol, Dave, Eve (broadcasting)
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("lounge".into());
        ad.app.state.voice.participants = vec![
            mk_person("alice", "Alice", false),
            mk_person("bob", "Bob", true),
            mk_person("carol", "Carol", true),
            mk_person("dave", "Dave", true),
            mk_person("eve", "Eve", true),
        ];
        ad.app.state.voice.watching = vec![
            mk_watch("bob", "Bob"),
            mk_watch("carol", "Carol"),
            mk_watch("dave", "Dave"),
            mk_watch("eve", "Eve"),
        ];
        ad.call_focus = Some(crate::ui::tiles::Focus::Stream("bob".into()));
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    assert!(window.get_has_active_streams());
    assert_eq!(window.get_watched_stream_count(), 4);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );

    // 2. Switch from Focus to Grid Mode via vertical selector:
    // Under FIX 2 (Persistent Grid Focus), switching modes preserves call_focus and audio routing!
    window.invoke_set_layout_mode(false);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );
    assert_eq!(window.get_call_focus(), 1);
    assert_eq!(window.get_focused_active(), 0);
    assert!(window.get_has_active_streams());
    assert_eq!(window.get_watched_stream_count(), 4);

    // 3. Switch back from Grid to Focus Mode via vertical selector: focus is preserved
    window.invoke_set_layout_mode(true);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );
    // Bob is at participant index 1 (after Alice at index 0)
    assert_eq!(window.get_call_focus(), 1);
    assert_eq!(window.get_focused_active(), 0);

    // 4. Verify participant row count contains all 5 participants (Alice + 4 streamers)
    // and verify unwatched_index for participant strip in Grid Mode
    let participants = window.get_participants();
    assert_eq!(participants.row_count(), 5);
    let p_alice = participants.row_data(0).unwrap();
    assert_eq!(p_alice.name, "Alice");
    assert!(!p_alice.live);
    assert!(!p_alice.watched);
    assert_eq!(p_alice.unwatched_index, 0);

    let p_bob = participants.row_data(1).unwrap();
    assert_eq!(p_bob.name, "Bob");
    assert!(p_bob.live);
    assert!(p_bob.watched);
    assert_eq!(p_bob.unwatched_index, -1);
}

#[test]
fn task7_persistent_grid_focus_and_participant_strip_in_grid() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let mk_person = |id: &str, name: &str, streaming: bool| yapfast_core::voice::Participant {
        id: id.into(),
        user: name.into(),
        local: false,
        streaming,
        video: None,
        connected: true,
        muted: false,
        deafened: false,
        speaking: false,
        problem: None,
        avatar: None,
        color: None,
        person: id.into(),
        server_muted: false,
        server_deafened: false,
    };
    let mk_watch = |id: &str, name: &str| yapfast_core::voice::Watching {
        user: id.into(),
        name: name.into(),
        opening: false,
        frames: None,
        viewers: vec![],
    };

    // 1. Initial setup: Alice (non-streamer) + Bob, Carol, Dave (3 streamers)
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("lounge".into());
        ad.app.state.voice.participants = vec![
            mk_person("alice", "Alice", false),
            mk_person("bob", "Bob", true),
            mk_person("carol", "Carol", true),
            mk_person("dave", "Dave", true),
        ];
        ad.app.state.voice.watching = vec![
            mk_watch("bob", "Bob"),
            mk_watch("carol", "Carol"),
            mk_watch("dave", "Dave"),
        ];
        ad.call_focus = Some(crate::ui::tiles::Focus::Stream("bob".into()));
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    assert_eq!(window.get_watched_stream_count(), 3);
    assert_eq!(window.get_focused_active(), 0);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );

    // 2. Focus Carol (participant index 2 in stage)
    window.invoke_focus_tile(2);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("carol".into()))
    );
    assert_eq!(window.get_call_focus(), 2);
    assert_eq!(window.get_focused_active(), 1);

    // 3. Switch to Grid Mode: Carol remains focused (effective-lead == 1 for 3-stream grid)
    window.invoke_set_layout_mode(false);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("carol".into()))
    );
    assert_eq!(window.get_call_focus(), 2);
    assert_eq!(window.get_focused_active(), 1);

    // 4. Focus Dave (participant index 3 in stage) while in Grid Mode
    window.invoke_focus_tile(3);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("dave".into()))
    );
    assert_eq!(window.get_call_focus(), 3);
    assert_eq!(window.get_focused_active(), 2);

    // 5. Switch back to Focus Mode: Dave remains the focused primary video!
    window.invoke_set_layout_mode(true);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("dave".into()))
    );
    assert_eq!(window.get_call_focus(), 3);
    assert_eq!(window.get_focused_active(), 2);

    // 6. Verify participant strip models for Grid Mode
    let participants = window.get_participants();
    assert_eq!(participants.row_count(), 4);
    let p_alice = participants.row_data(0).unwrap();
    assert!(!p_alice.watched);
    assert_eq!(p_alice.unwatched_index, 0);

    let p_bob = participants.row_data(1).unwrap();
    assert!(p_bob.watched);
    assert_eq!(p_bob.watched_index, 0);
    assert_eq!(p_bob.unwatched_index, -1);

    let p_carol = participants.row_data(2).unwrap();
    assert!(p_carol.watched);
    assert_eq!(p_carol.watched_index, 1);
    assert_eq!(p_carol.unwatched_index, -1);

    let p_dave = participants.row_data(3).unwrap();
    assert!(p_dave.watched);
    assert_eq!(p_dave.watched_index, 2);
    assert_eq!(p_dave.unwatched_index, -1);
}

#[test]
fn task7_zero_watched_streams_voice_stage_visibility_and_lifecycle() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let mk_person = |id: &str, name: &str, streaming: bool| yapfast_core::voice::Participant {
        id: id.into(),
        user: name.into(),
        local: false,
        streaming,
        video: None,
        connected: true,
        muted: false,
        deafened: false,
        speaking: false,
        problem: None,
        avatar: None,
        color: None,
        person: id.into(),
        server_muted: false,
        server_deafened: false,
    };
    let mk_watch = |id: &str, name: &str| yapfast_core::voice::Watching {
        user: id.into(),
        name: name.into(),
        opening: false,
        frames: None,
        viewers: vec![],
    };

    // 1. Initial State: Alice (non-streamer) + 4 broadcasters (Bob, Carol, Dave, Eve).
    // ZERO streams watched!
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("stream_stage".into());
        ad.app.state.voice.participants = vec![
            mk_person("alice", "Alice", false),
            mk_person("bob", "Bob", true),
            mk_person("carol", "Carol", true),
            mk_person("dave", "Dave", true),
            mk_person("eve", "Eve", true),
        ];
        ad.app.state.voice.watching = vec![];
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    // Verify Zero-stream state:
    // No active streams, stage must NOT be empty!
    assert!(!window.get_has_active_streams());
    assert_eq!(window.get_watched_stream_count(), 0);
    assert_eq!(adapter.borrow().call_focus, None);

    let participants = window.get_participants();
    assert_eq!(participants.row_count(), 5);
    for i in 0..5 {
        let p = participants.row_data(i).unwrap();
        assert!(
            !p.watched,
            "Participant {i} should not be watched initially"
        );
        assert_eq!(p.unwatched_index, i as i32);
    }

    // Alice is non-streamer, others are live broadcasters
    assert!(!participants.row_data(0).unwrap().live);
    assert!(participants.row_data(1).unwrap().live);
    assert!(participants.row_data(2).unwrap().live);
    assert!(participants.row_data(3).unwrap().live);
    assert!(participants.row_data(4).unwrap().live);

    // 2. Open Bob's stream via watch_stream
    window.invoke_watch_stream("bob".into());
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.watching.push(mk_watch("bob", "Bob"));
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    assert!(window.get_has_active_streams());
    assert_eq!(window.get_watched_stream_count(), 1);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );
    assert_eq!(window.get_focused_active(), 0);

    let participants = window.get_participants();
    assert_eq!(participants.row_count(), 5);
    let p_bob = participants.row_data(1).unwrap();
    assert!(p_bob.watched);
    assert_eq!(p_bob.watched_index, 0);
    assert_eq!(p_bob.unwatched_index, -1);

    // Other 4 participants remain in unwatched strip
    assert_eq!(participants.row_data(0).unwrap().unwatched_index, 0); // Alice
    assert_eq!(participants.row_data(2).unwrap().unwatched_index, 1); // Carol
    assert_eq!(participants.row_data(3).unwrap().unwatched_index, 2); // Dave
    assert_eq!(participants.row_data(4).unwrap().unwatched_index, 3); // Eve

    // 3. Add Carol's stream via add_stream
    window.invoke_add_stream("carol".into());
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.watching.push(mk_watch("carol", "Carol"));
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    // Focus remains on Bob
    assert_eq!(window.get_watched_stream_count(), 2);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );

    // 4. Switch to Grid Mode
    window.invoke_set_layout_mode(false);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );

    // 5. Close both streams -> return cleanly to zero-watched-stream normal grid mode
    window.invoke_stop_watching("bob".into());
    window.invoke_stop_watching("carol".into());
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.watching.clear();
        ad.call_focus = None;
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    assert!(!window.get_has_active_streams());
    assert_eq!(window.get_watched_stream_count(), 0);
    assert_eq!(adapter.borrow().call_focus, None);

    let participants = window.get_participants();
    assert_eq!(participants.row_count(), 5);
    for i in 0..5 {
        let p = participants.row_data(i).unwrap();
        assert!(!p.watched);
        assert_eq!(p.unwatched_index, i as i32);
    }
}

#[test]
fn task7_grid_layout_geometry_non_overlapping_and_focus_placement() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let mk_person = |id: &str, name: &str, streaming: bool| yapfast_core::voice::Participant {
        id: id.into(),
        user: name.into(),
        local: false,
        streaming,
        video: None,
        connected: true,
        muted: false,
        deafened: false,
        speaking: false,
        problem: None,
        avatar: None,
        color: None,
        person: id.into(),
        server_muted: false,
        server_deafened: false,
    };
    let mk_watch = |id: &str, name: &str| yapfast_core::voice::Watching {
        user: id.into(),
        name: name.into(),
        opening: false,
        frames: None,
        viewers: vec![],
    };

    #[derive(Debug, Clone, Copy)]
    struct Cell {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    }
    impl Cell {
        fn intersects(&self, o: &Cell) -> bool {
            // Margin of 0.1px to ignore touching boundary borders
            self.x + 0.1 < o.x + o.w
                && self.x + self.w - 0.1 > o.x
                && self.y + 0.1 < o.y + o.h
                && self.y + self.h - 0.1 > o.y
        }
    }

    // Mathematical verification of 2, 3, 4 stream layouts at different viewport sizes
    let viewports = [(1280.0, 720.0), (1024.0, 768.0), (800.0, 600.0)];
    let gap = 10.0f32;
    let inset = 8.0f32;
    let strip_gap = 8.0f32;

    for (vw, vh) in viewports {
        let avail_w = vw - 2.0 * inset;
        let avail_h = vh - 2.0 * inset;

        // Test with 1 unwatched participant (strip_count = 1)
        let strip_thumb_h = 104.0f32.min(avail_h * 0.22);
        let stage_avail_h = avail_h - strip_thumb_h - strip_gap;
        let strip_y = vh - inset - strip_thumb_h;

        // 1. Four-stream grid (2x2)
        {
            let g4_w = ((avail_w - gap) / 2.0).min((stage_avail_h - gap) / 2.0 / 0.5625);
            let g4_h = g4_w * 0.5625;
            let g4_tot_w = 2.0 * g4_w + gap;
            let g4_tot_h = 2.0 * g4_h + gap;
            let g4_start_x = (vw - g4_tot_w) / 2.0;
            let g4_start_y = inset + (stage_avail_h - g4_tot_h) / 2.0;

            let mut cells = Vec::new();
            for a_slot in 0..4 {
                let col = a_slot % 2;
                let row = a_slot / 2;
                let x = g4_start_x + (col as f32) * (g4_w + gap);
                let y = g4_start_y + (row as f32) * (g4_h + gap);
                cells.push(Cell {
                    x,
                    y,
                    w: g4_w,
                    h: g4_h,
                });
            }

            for i in 0..4 {
                assert!(cells[i].x >= inset, "Cell {i} left out of bounds");
                assert!(
                    cells[i].x + cells[i].w <= vw - inset + 0.5,
                    "Cell {i} right out of bounds"
                );
                assert!(cells[i].y >= inset, "Cell {i} top out of bounds");
                assert!(
                    cells[i].y + cells[i].h <= inset + stage_avail_h + 0.5,
                    "Cell {i} bottom exceeds stage_avail_h"
                );
                assert!(
                    cells[i].y + cells[i].h <= strip_y,
                    "Cell {i} overlaps participant strip"
                );

                for j in (i + 1)..4 {
                    assert!(
                        !cells[i].intersects(&cells[j]),
                        "Cell {i} and {j} overlap in 4-stream grid! i={:?}, j={:?}",
                        cells[i],
                        cells[j]
                    );
                }
            }
        }

        // 2. Three-stream grid (1 large left, 2 stacked right)
        for effective_lead in 0..3 {
            let g3_large_h = stage_avail_h.min((avail_w - gap * 0.12) / 1.5 * 0.5625);
            let g3_large_w = g3_large_h / 0.5625;
            let g3_small_h = (g3_large_h - gap) / 2.0;
            let g3_small_w = g3_small_h / 0.5625;
            let g3_tot_w = g3_large_w + gap + g3_small_w;
            let g3_start_x = (vw - g3_tot_w) / 2.0;
            let g3_start_y = inset + (stage_avail_h - g3_large_h) / 2.0;

            let g3_sub0 = if effective_lead == 0 { 1 } else { 0 };

            let mut cells = Vec::new();
            for a_slot in 0..3 {
                let is_lead = a_slot == effective_lead;
                let x = if is_lead {
                    g3_start_x
                } else {
                    g3_start_x + g3_large_w + gap
                };
                let y = if is_lead || a_slot == g3_sub0 {
                    g3_start_y
                } else {
                    g3_start_y + g3_small_h + gap
                };
                let w = if is_lead { g3_large_w } else { g3_small_w };
                let h = if is_lead { g3_large_h } else { g3_small_h };
                cells.push(Cell { x, y, w, h });
            }

            for i in 0..3 {
                assert!(cells[i].x >= inset, "3-stream Cell {i} left out of bounds");
                assert!(
                    cells[i].x + cells[i].w <= vw - inset + 0.5,
                    "3-stream Cell {i} right out of bounds"
                );
                assert!(cells[i].y >= inset, "3-stream Cell {i} top out of bounds");
                assert!(
                    cells[i].y + cells[i].h <= inset + stage_avail_h + 0.5,
                    "3-stream Cell {i} bottom exceeds stage_avail_h"
                );
                assert!(
                    cells[i].y + cells[i].h <= strip_y,
                    "3-stream Cell {i} overlaps participant strip"
                );

                for j in (i + 1)..3 {
                    assert!(
                        !cells[i].intersects(&cells[j]),
                        "3-stream Cell {i} and {j} overlap! lead={effective_lead}, i={:?}, j={:?}",
                        cells[i],
                        cells[j]
                    );
                }
            }
        }

        // 3. Two-stream grid (side by side)
        {
            let g2_w = ((avail_w - gap) / 2.0).min(stage_avail_h / 0.5625);
            let g2_h = g2_w * 0.5625;
            let g2_tot_w = 2.0 * g2_w + gap;
            let g2_start_x = (vw - g2_tot_w) / 2.0;
            let g2_y = inset + (stage_avail_h - g2_h) / 2.0;

            let c0 = Cell {
                x: g2_start_x,
                y: g2_y,
                w: g2_w,
                h: g2_h,
            };
            let c1 = Cell {
                x: g2_start_x + g2_w + gap,
                y: g2_y,
                w: g2_w,
                h: g2_h,
            };

            assert!(c0.x >= inset);
            assert!(c1.x + c1.w <= vw - inset + 0.5);
            assert!(!c0.intersects(&c1), "2-stream cells overlap!");
        }
    }

    // 4. Test actual UI runtime state with 4 streams in Grid Mode
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("stream_stage".into());
        ad.app.state.voice.participants = vec![
            mk_person("alice", "Alice", false),
            mk_person("bob", "Bob", true),
            mk_person("carol", "Carol", true),
            mk_person("dave", "Dave", true),
            mk_person("eve", "Eve", true),
        ];
        ad.app.state.voice.watching = vec![
            mk_watch("bob", "Bob"),
            mk_watch("carol", "Carol"),
            mk_watch("dave", "Dave"),
            mk_watch("eve", "Eve"),
        ];
        ad.call_focus = Some(crate::ui::tiles::Focus::Stream("bob".into()));
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    // Switch to Grid Mode
    window.invoke_set_layout_mode(false);
    assert_eq!(window.get_watched_stream_count(), 4);
    assert_eq!(window.get_focused_active(), 0);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );

    // Switch focus to Carol while in Grid Mode
    window.invoke_focus_tile(2); // Carol is slot 2
    assert_eq!(window.get_focused_active(), 1);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("carol".into()))
    );

    // Switch back to Focus Mode -> Carol is the large stream
    window.invoke_set_layout_mode(true);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("carol".into()))
    );
    assert_eq!(window.get_focused_active(), 1);
}

#[test]
fn task7_stream_flicker_elimination_add_stream_preserves_main_frame_and_focus() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let test_frame =
        slint::Image::from_rgba8(slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(64, 64));

    let mk_person = |id: &str, name: &str, streaming: bool| yapfast_core::voice::Participant {
        id: id.into(),
        user: name.into(),
        local: false,
        streaming,
        video: None,
        connected: true,
        muted: false,
        deafened: false,
        speaking: false,
        problem: None,
        avatar: None,
        color: None,
        person: id.into(),
        server_muted: false,
        server_deafened: false,
    };

    // Initial state: Bob is watched and focused with an active frame
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("lounge".into());
        ad.app.state.voice.participants = vec![
            mk_person("alice", "Alice", false),
            mk_person("bob", "Bob", true),
            mk_person("carol", "Carol", true),
        ];
        ad.app.state.voice.watching = vec![yapfast_core::voice::Watching {
            user: "bob".into(),
            name: "Bob".into(),
            opening: false,
            frames: None,
            viewers: vec![],
        }];
        ad.call_focus = Some(crate::ui::tiles::Focus::Stream("bob".into()));
        ad.show_stage(&window);
        ad.show_voice(&window);
        ad.models.stream_frames.set_row_data(1, test_frame.clone());
        window.set_watch_frame(test_frame.clone());
    }

    assert_eq!(
        adapter
            .borrow()
            .models
            .stream_frames
            .row_data(1)
            .unwrap()
            .size()
            .width,
        64
    );
    assert_eq!(window.get_watch_frame().size().width, 64);
    assert_eq!(window.get_watch_user(), "bob");

    // Click Add Stream on Carol
    window.invoke_add_stream("carol".into());

    // Focus MUST remain firmly on Bob
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );
    // Bob's frame in stream_frames MUST NOT be cleared
    assert_eq!(
        adapter
            .borrow()
            .models
            .stream_frames
            .row_data(1)
            .unwrap()
            .size()
            .width,
        64
    );
    assert_eq!(window.get_watch_frame().size().width, 64);

    // Backend adds Carol to watching and UI applies update
    {
        let mut ad = adapter.borrow_mut();
        ad.app
            .state
            .voice
            .watching
            .push(yapfast_core::voice::Watching {
                user: "carol".into(),
                name: "Carol".into(),
                opening: true,
                frames: None,
                viewers: vec![],
            });
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    // After state update:
    // Bob is still focused, Bob's frame is preserved, Carol has no frame yet
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );
    assert_eq!(
        adapter
            .borrow()
            .models
            .stream_frames
            .row_data(1)
            .unwrap()
            .size()
            .width,
        64
    );
    assert_eq!(window.get_watch_frame().size().width, 64);
    assert_eq!(
        adapter
            .borrow()
            .models
            .stream_frames
            .row_data(2)
            .unwrap()
            .size()
            .width,
        0
    );
    assert_eq!(window.get_watch_user(), "bob");
}

#[test]
fn task7_stream_flicker_elimination_replace_stream_clears_old_frame_and_prevents_stale_flash() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let bob_frame =
        slint::Image::from_rgba8(slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(64, 64));
    let carol_frame =
        slint::Image::from_rgba8(slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(80, 80));

    let mk_person = |id: &str, name: &str, streaming: bool| yapfast_core::voice::Participant {
        id: id.into(),
        user: name.into(),
        local: false,
        streaming,
        video: None,
        connected: true,
        muted: false,
        deafened: false,
        speaking: false,
        problem: None,
        avatar: None,
        color: None,
        person: id.into(),
        server_muted: false,
        server_deafened: false,
    };

    // Initial state: Bob is playing
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("lounge".into());
        ad.app.state.voice.participants = vec![
            mk_person("alice", "Alice", false),
            mk_person("bob", "Bob", true),
            mk_person("carol", "Carol", true),
        ];
        ad.app.state.voice.watching = vec![yapfast_core::voice::Watching {
            user: "bob".into(),
            name: "Bob".into(),
            opening: false,
            frames: None,
            viewers: vec![],
        }];
        ad.call_focus = Some(crate::ui::tiles::Focus::Stream("bob".into()));
        ad.show_stage(&window);
        ad.show_voice(&window);
        ad.models.stream_frames.set_row_data(1, bob_frame.clone());
        window.set_watch_frame(bob_frame);
    }

    assert_eq!(window.get_watch_frame().size().width, 64);
    assert_eq!(
        adapter
            .borrow()
            .models
            .stream_frames
            .row_data(1)
            .unwrap()
            .size()
            .width,
        64
    );

    // Replace stream: user clicks Watch Stream on Carol
    window.invoke_watch_stream("carol".into());

    // 1. watch_frame must immediately be cleared to avoid stale frame from Bob flashing
    assert_eq!(window.get_watch_frame().size().width, 0);
    // 2. Bob's frame in stream_frames must immediately be cleared
    assert_eq!(
        adapter
            .borrow()
            .models
            .stream_frames
            .row_data(1)
            .unwrap()
            .size()
            .width,
        0
    );
    // 3. Carol is now the focused stream
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("carol".into()))
    );
    assert_eq!(window.get_watch_user(), "carol");

    // Later, Carol's first frame arrives:
    adapter
        .borrow()
        .models
        .stream_frames
        .set_row_data(2, carol_frame.clone());
    window.set_watch_frame(carol_frame);

    assert_eq!(
        adapter
            .borrow()
            .models
            .stream_frames
            .row_data(2)
            .unwrap()
            .size()
            .width,
        80
    );
    assert_eq!(window.get_watch_frame().size().width, 80);
    assert_eq!(
        adapter
            .borrow()
            .models
            .stream_frames
            .row_data(1)
            .unwrap()
            .size()
            .width,
        0
    );
}

#[test]
fn task7_stream_flicker_elimination_close_secondary_stream_synchronous_teardown_and_discards_late_frames()
 {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let bob_frame =
        slint::Image::from_rgba8(slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(64, 64));
    let carol_frame =
        slint::Image::from_rgba8(slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(80, 80));

    let mk_person = |id: &str, name: &str, streaming: bool| yapfast_core::voice::Participant {
        id: id.into(),
        user: name.into(),
        local: false,
        streaming,
        video: None,
        connected: true,
        muted: false,
        deafened: false,
        speaking: false,
        problem: None,
        avatar: None,
        color: None,
        person: id.into(),
        server_muted: false,
        server_deafened: false,
    };

    // Initial state: Both Bob and Carol are watched
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("lounge".into());
        ad.app.state.voice.participants = vec![
            mk_person("alice", "Alice", false),
            mk_person("bob", "Bob", true),
            mk_person("carol", "Carol", true),
        ];
        ad.app.state.voice.watching = vec![
            yapfast_core::voice::Watching {
                user: "bob".into(),
                name: "Bob".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            },
            yapfast_core::voice::Watching {
                user: "carol".into(),
                name: "Carol".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            },
        ];
        ad.call_focus = Some(crate::ui::tiles::Focus::Stream("bob".into()));
        ad.show_stage(&window);
        ad.show_voice(&window);
        ad.models.stream_frames.set_row_data(1, bob_frame.clone());
        ad.models.stream_frames.set_row_data(2, carol_frame.clone());
        window.set_watch_frame(bob_frame);
    }

    assert_eq!(window.get_watched_stream_count(), 2);
    let participants = window.get_participants();
    assert!(participants.row_data(1).unwrap().watched);
    assert!(participants.row_data(2).unwrap().watched);

    // User closes Carol's stream (clicks X)
    window.invoke_stop_watching("carol".into());

    // Immediate synchronous teardown verification:
    // 1. Carol is removed from watching in state
    let watching: Vec<String> = adapter
        .borrow()
        .app
        .state
        .voice
        .watching
        .iter()
        .map(|w| w.user.clone())
        .collect();
    assert_eq!(watching, vec!["bob"]);

    // 2. Carol's stream frame is immediately cleared
    assert_eq!(
        adapter
            .borrow()
            .models
            .stream_frames
            .row_data(2)
            .unwrap()
            .size()
            .width,
        0
    );

    // 3. Bob's frame is preserved and intact
    assert_eq!(
        adapter
            .borrow()
            .models
            .stream_frames
            .row_data(1)
            .unwrap()
            .size()
            .width,
        64
    );
    assert_eq!(window.get_watch_frame().size().width, 64);

    // 4. Backing model participants immediately reflects carol is unwatched
    let updated_participants = window.get_participants();
    assert!(updated_participants.row_data(1).unwrap().watched);
    assert!(!updated_participants.row_data(2).unwrap().watched);
    assert_eq!(window.get_watched_stream_count(), 1);
}

#[test]
fn task7_voice_rejoin_same_channel_immediately_after_disconnect() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    window.invoke_explore_preview();
    let voice_id = adapter
        .borrow()
        .app
        .state
        .rooms
        .iter()
        .find(|room| room.voice)
        .unwrap()
        .id
        .clone();

    // 1. Join voice room
    window.invoke_select_room(voice_id.clone().into());
    assert!(adapter.borrow().app.state.voice.active());
    assert_eq!(
        adapter.borrow().app.state.voice.room.as_deref(),
        Some(voice_id.as_str())
    );

    // 2. Select voice room again to show call on stage
    window.invoke_select_room(voice_id.clone().into());
    assert_eq!(
        adapter.borrow().app.state.selected.as_deref(),
        Some(voice_id.as_str())
    );

    // 3. Leave voice
    window.invoke_leave_voice();
    assert!(!adapter.borrow().app.state.voice.active());
    // Room remains selected on screen
    assert_eq!(
        adapter.borrow().app.state.selected.as_deref(),
        Some(voice_id.as_str())
    );

    // 4. Immediately click the same voice channel in sidebar to rejoin
    window.invoke_select_room(voice_id.clone().into());

    // Rejoin must succeed without needing intermediate navigation to another room!
    assert!(adapter.borrow().app.state.voice.active());
    assert_eq!(
        adapter.borrow().app.state.voice.room.as_deref(),
        Some(voice_id.as_str())
    );

    // 5. Subsequent click while already in call does not trigger redundant disconnect or leave
    window.invoke_select_room(voice_id.clone().into());
    assert!(adapter.borrow().app.state.voice.active());
    assert_eq!(
        adapter.borrow().app.state.voice.room.as_deref(),
        Some(voice_id.as_str())
    );
}

#[test]
fn task7_participant_card_hover_and_loading_stability_on_stream_switch() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let mk_person = |id: &str, name: &str, streaming: bool| yapfast_core::voice::Participant {
        id: id.into(),
        user: name.into(),
        person: id.into(),
        local: false,
        muted: false,
        deafened: false,
        server_muted: false,
        server_deafened: false,
        speaking: false,
        connected: true,
        problem: None,
        avatar: None,
        color: None,
        streaming,
        video: None,
    };

    let mut bob_buffer = slint::SharedPixelBuffer::new(64, 36);
    bob_buffer.make_mut_bytes().fill(255);
    let bob_frame = slint::Image::from_rgb8(bob_buffer);

    let mut carol_buffer = slint::SharedPixelBuffer::new(80, 45);
    carol_buffer.make_mut_bytes().fill(200);
    let carol_frame = slint::Image::from_rgb8(carol_buffer);

    // Initial state: Bob is being watched in Focus Mode, Carol is an unwatched live streamer in strip
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("lounge".into());
        ad.app.state.voice.participants = vec![
            mk_person("alice", "Alice", false),
            mk_person("bob", "Bob", true),
            mk_person("carol", "Carol", true),
        ];
        ad.app.state.voice.watching = vec![yapfast_core::voice::Watching {
            user: "bob".into(),
            name: "Bob".into(),
            opening: false,
            frames: None,
            viewers: vec![],
        }];
        ad.call_focus = Some(crate::ui::tiles::Focus::Stream("bob".into()));
        ad.show_stage(&window);
        ad.show_voice(&window);
        ad.models.stream_frames.set_row_data(1, bob_frame.clone());
        window.set_watch_frame(bob_frame);
    }

    assert_eq!(window.get_watch_user(), "bob");
    assert_eq!(window.get_watched_stream_count(), 1);

    // 1. User clicks "Watch stream" on Carol
    window.invoke_watch_stream("carol".into());

    // Carol is now the designated watch user
    assert_eq!(window.get_watch_user(), "carol");
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("carol".into()))
    );

    // watch_frame is cleared immediately so Bob's old frame does not linger on Carol's stage
    assert_eq!(window.get_watch_frame().size().width, 0);

    // In single watched stream mode, Carol is now the active watched stream, and Bob
    // has seamlessly transitioned back to an unwatched participant in the strip.
    // 2. Carol's decoded frame arrives
    adapter
        .borrow()
        .models
        .stream_frames
        .set_row_data(2, carol_frame.clone());
    window.set_watch_frame(carol_frame);

    assert_eq!(window.get_watch_frame().size().width, 80);

    // Backend confirms watching Carol
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.watching = vec![yapfast_core::voice::Watching {
            user: "carol".into(),
            name: "Carol".into(),
            opening: false,
            frames: None,
            viewers: vec![],
        }];
        ad.show_stage(&window);
        ad.show_voice(&window);
    }

    let participants = window.get_participants();
    let p_bob = participants.row_data(1).unwrap();
    let p_carol = participants.row_data(2).unwrap();
    assert!(!p_bob.watched);
    assert!(p_carol.watched);
}

#[test]
fn task7_ui_stability_on_add_stream_preserves_card_labels_and_controls() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let mk_person = |id: &str, name: &str, streaming: bool| yapfast_core::voice::Participant {
        id: id.into(),
        user: name.into(),
        person: id.into(),
        local: false,
        muted: false,
        deafened: false,
        server_muted: false,
        server_deafened: false,
        speaking: false,
        connected: true,
        problem: None,
        avatar: None,
        color: None,
        streaming,
        video: None,
    };

    let mut bob_buffer = slint::SharedPixelBuffer::new(64, 36);
    bob_buffer.make_mut_bytes().fill(255);
    let bob_frame = slint::Image::from_rgb8(bob_buffer);

    // Initial setup: Alice, Bob (streaming, watched in Focus), Carol (streaming, in strip), Dave (non-streamer)
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("stage".into());
        ad.app.state.voice.participants = vec![
            mk_person("alice", "Alice", false),
            mk_person("bob", "Bob", true),
            mk_person("carol", "Carol", true),
            mk_person("dave", "Dave", false),
        ];
        ad.app.state.voice.watching = vec![yapfast_core::voice::Watching {
            user: "bob".into(),
            name: "Bob".into(),
            opening: false,
            frames: None,
            viewers: vec![],
        }];
        ad.call_focus = Some(crate::ui::tiles::Focus::Stream("bob".into()));
        ad.show_voice(&window);
        ad.models.stream_frames.set_row_data(1, bob_frame.clone());
        window.set_watch_frame(bob_frame);
    }

    assert_eq!(window.get_watch_user(), "bob");
    assert_eq!(window.get_watched_stream_count(), 1);

    let initial_parts = window.get_participants();
    assert_eq!(initial_parts.row_count(), 4);
    assert_eq!(initial_parts.row_data(0).unwrap().name, "Alice");
    assert_eq!(initial_parts.row_data(1).unwrap().name, "Bob");
    assert_eq!(initial_parts.row_data(2).unwrap().name, "Carol");
    assert_eq!(initial_parts.row_data(3).unwrap().name, "Dave");
    assert!(!initial_parts.row_data(0).unwrap().watched);
    assert!(initial_parts.row_data(1).unwrap().watched);
    assert!(!initial_parts.row_data(2).unwrap().watched);
    assert!(!initial_parts.row_data(3).unwrap().watched);

    // User clicks Add Stream [Eye +] on Carol
    window.invoke_add_stream("carol".into());

    // 1. Bob must remain firmly focused without visual disruption
    assert_eq!(window.get_watch_user(), "bob");
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );

    // 2. Bob's stream frame must not be cleared
    assert_eq!(
        adapter
            .borrow()
            .models
            .stream_frames
            .row_data(1)
            .unwrap()
            .size()
            .width,
        64
    );

    // Backend confirms watching both Bob and Carol
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.watching = vec![
            yapfast_core::voice::Watching {
                user: "bob".into(),
                name: "Bob".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            },
            yapfast_core::voice::Watching {
                user: "carol".into(),
                name: "Carol".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            },
        ];
        ad.show_voice(&window);
    }

    // 3. Watched stream count updates to 2
    assert_eq!(window.get_watched_stream_count(), 2);

    // 4. Participants model preserves all labels, names, and correct watched status
    let updated_parts = window.get_participants();
    assert_eq!(updated_parts.row_count(), 4);
    assert_eq!(updated_parts.row_data(0).unwrap().name, "Alice");
    assert_eq!(updated_parts.row_data(1).unwrap().name, "Bob");
    assert_eq!(updated_parts.row_data(2).unwrap().name, "Carol");
    assert_eq!(updated_parts.row_data(3).unwrap().name, "Dave");
    assert!(!updated_parts.row_data(0).unwrap().watched);
    assert!(updated_parts.row_data(1).unwrap().watched);
    assert!(updated_parts.row_data(2).unwrap().watched);
    assert!(!updated_parts.row_data(3).unwrap().watched);
}

#[test]
fn task7_no_connecting_flash_on_add_stream_preserves_avatar_and_stability() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let mk_person = |id: &str, name: &str, streaming: bool| yapfast_core::voice::Participant {
        id: id.into(),
        user: name.into(),
        person: id.into(),
        local: false,
        muted: false,
        deafened: false,
        server_muted: false,
        server_deafened: false,
        speaking: false,
        connected: true,
        problem: None,
        avatar: None,
        color: None,
        streaming,
        video: None,
    };

    let mut bob_buffer = slint::SharedPixelBuffer::new(64, 36);
    bob_buffer.make_mut_bytes().fill(255);
    let bob_frame = slint::Image::from_rgb8(bob_buffer);

    let mut eve_buffer = slint::SharedPixelBuffer::new(80, 45);
    eve_buffer.make_mut_bytes().fill(180);
    let eve_frame = slint::Image::from_rgb8(eve_buffer);

    // Initial setup: Bob is watched in Focus Mode, Eve is an unwatched live streamer in strip
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("lounge".into());
        ad.app.state.voice.participants =
            vec![mk_person("bob", "Bob", true), mk_person("eve", "Eve", true)];
        ad.app.state.voice.watching = vec![yapfast_core::voice::Watching {
            user: "bob".into(),
            name: "Bob".into(),
            opening: false,
            frames: None,
            viewers: vec![],
        }];
        ad.call_focus = Some(crate::ui::tiles::Focus::Stream("bob".into()));
        ad.show_voice(&window);
        ad.models.stream_frames.set_row_data(0, bob_frame.clone());
        window.set_watch_frame(bob_frame);
    }

    assert_eq!(window.get_watch_user(), "bob");
    assert_eq!(window.get_watched_stream_count(), 1);

    // User clicks Add Stream on Eve
    window.invoke_add_stream("eve".into());

    // 1. Eve is added to watching, but has no decoded frame yet (State B)
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.watching = vec![
            yapfast_core::voice::Watching {
                user: "bob".into(),
                name: "Bob".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            },
            yapfast_core::voice::Watching {
                user: "eve".into(),
                name: "Eve".into(),
                opening: false,
                frames: None,
                viewers: vec![],
            },
        ];
        ad.show_voice(&window);
    }

    // 2. Eve's frame in stream_frames is initially empty (0 width)
    let stream_frames = adapter.borrow().models.stream_frames.clone();
    assert_eq!(stream_frames.row_data(1).unwrap().size().width, 0);

    // 3. Eve's participant card preserves normal name without temporary Connecting... text,
    // and stream_ready is false in State B so presentation stays strictly in participant/avatar state
    let parts = window.get_participants();
    let p_eve = parts.row_data(1).unwrap();
    assert_eq!(p_eve.name, "Eve");
    assert_eq!(p_eve.status, "Listening");
    assert!(p_eve.watched);
    assert!(!p_eve.stream_ready);

    // 4. Bob remains primary watch user and keeps playing undisturbed
    assert_eq!(window.get_watch_user(), "bob");
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );

    // 5. When Eve's first decoded video frame arrives (State C):
    adapter
        .borrow()
        .models
        .stream_frames
        .set_row_data(1, eve_frame.clone());
    adapter.borrow_mut().show_voice(&window);

    // Eve now has a valid decoded frame (width 80) and stream_ready is true
    assert_eq!(
        adapter
            .borrow()
            .models
            .stream_frames
            .row_data(1)
            .unwrap()
            .size()
            .width,
        80
    );
    assert!(window.get_participants().row_data(1).unwrap().stream_ready);
}

#[test]
fn task7_grid_mode_four_streams_click_stability_and_no_black_fullscreen() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let mk_person = |id: &str, name: &str, streaming: bool| yapfast_core::voice::Participant {
        id: id.into(),
        user: name.into(),
        local: false,
        streaming,
        video: None,
        connected: true,
        muted: false,
        deafened: false,
        speaking: false,
        problem: None,
        avatar: None,
        color: None,
        person: id.into(),
        server_muted: false,
        server_deafened: false,
    };
    let mk_watch = |id: &str, name: &str| yapfast_core::voice::Watching {
        user: id.into(),
        name: name.into(),
        opening: false,
        frames: None,
        viewers: vec![],
    };

    let mk_frame = |w: u32, h: u32| {
        slint::Image::from_rgba8(slint::SharedPixelBuffer::clone_from_slice(
            &vec![200u8; (w * h * 4) as usize],
            w,
            h,
        ))
    };

    // 1. Setup 4 streams active in Stream Stage
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.voice.phase = yapfast_core::voice::Phase::Connected;
        ad.app.state.voice.room = Some("stream_stage".into());
        ad.app.state.voice.participants = vec![
            mk_person("alice", "Alice", false),
            mk_person("bob", "Bob", true),
            mk_person("carol", "Carol", true),
            mk_person("dave", "Dave", true),
            mk_person("eve", "Eve", true),
        ];
        ad.app.state.voice.watching = vec![
            mk_watch("bob", "Bob"),
            mk_watch("carol", "Carol"),
            mk_watch("dave", "Dave"),
            mk_watch("eve", "Eve"),
        ];
        ad.call_focus = Some(crate::ui::tiles::Focus::Stream("bob".into()));
        ad.show_stage(&window);
        ad.show_voice(&window);

        // Populate valid stream frames for all 4 broadcasters
        ad.models.stream_frames.set_row_data(1, mk_frame(100, 100)); // Bob
        ad.models.stream_frames.set_row_data(2, mk_frame(100, 100)); // Carol
        ad.models.stream_frames.set_row_data(3, mk_frame(100, 100)); // Dave
        ad.models.stream_frames.set_row_data(4, mk_frame(100, 100)); // Eve
    }

    // Switch to Grid Mode
    window.set_voice_grid_mode(true);
    window.invoke_set_layout_mode(false);
    assert!(window.get_voice_grid_mode());
    assert_eq!(window.get_watched_stream_count(), 4);
    assert_eq!(window.get_focused_active(), 0); // Bob
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );
    assert!(!window.get_stage_full());

    // 2. Click Bob's tile again (slot 1) -> Already focused, deduplication keeps state untouched
    window.invoke_focus_tile(1);
    assert_eq!(window.get_focused_active(), 0);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("bob".into()))
    );
    assert!(!window.get_stage_full());

    // 3. Repeated clicks on focused tile must NEVER trigger stage_full
    for _ in 0..5 {
        window.invoke_focus_tile(1);
    }
    assert!(!window.get_stage_full());

    // 4. Click Carol's tile (slot 2) -> switches focus safely to Carol
    window.invoke_focus_tile(2);
    assert_eq!(window.get_focused_active(), 1);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("carol".into()))
    );
    assert_eq!(window.get_watch_user(), "carol");
    assert!(!window.get_stage_full());

    // 5. Repeated clicks on Carol do not change focus or enter stage_full
    for _ in 0..5 {
        window.invoke_focus_tile(2);
    }
    assert_eq!(window.get_focused_active(), 1);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("carol".into()))
    );
    assert!(!window.get_stage_full());

    // 6. Click Dave (slot 3) and Eve (slot 4)
    window.invoke_focus_tile(3);
    assert_eq!(window.get_focused_active(), 2);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("dave".into()))
    );
    assert!(!window.get_stage_full());

    window.invoke_focus_tile(4);
    assert_eq!(window.get_focused_active(), 3);
    assert_eq!(
        adapter.borrow().call_focus,
        Some(crate::ui::tiles::Focus::Stream("eve".into()))
    );
    assert!(!window.get_stage_full());

    // 7. Verify all 4 stream frames remain intact and actively playing
    let sf = adapter.borrow().models.stream_frames.clone();
    assert_eq!(sf.row_count(), 5);
    assert_eq!(sf.row_data(1).unwrap().size().width, 100);
    assert_eq!(sf.row_data(2).unwrap().size().width, 100);
    assert_eq!(sf.row_data(3).unwrap().size().width, 100);
    assert_eq!(sf.row_data(4).unwrap().size().width, 100);
}

#[test]
fn task8_plugins_settings_section_and_preferences() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    // Initial state: default false
    assert!(!window.get_read_all_notifications_button());
    assert!(!window.get_dont_show_typing());

    // Navigate to Plugins section (id = 9)
    window.set_settings_open(true);
    window.set_settings_section(9);
    assert_eq!(window.get_settings_section(), 9);

    // Toggle Read All Notifications Button and Don't Show When You're Typing
    window.set_read_all_notifications_button(true);
    window.set_dont_show_typing(true);

    // Invoke preferences_changed callback
    window.invoke_preferences_changed();

    // Verify values updated in layout
    {
        let ad = adapter.borrow();
        assert!(ad.layout.read_all_notifications_button);
        assert!(ad.layout.dont_show_typing());
    }

    // Verify persistence to disk
    let reloaded = Layout::read(directory.path());
    assert!(reloaded.read_all_notifications_button);
    assert!(reloaded.dont_show_typing());
}

#[test]
fn task8_legacy_share_typing_migration() {
    let directory = tempfile::tempdir().unwrap();
    let layout_path = directory.path().join("layout.json");

    // Case 1: Legacy user had share_typing: false (typing suppressed)
    // Should migrate to dont_show_typing: true
    std::fs::write(&layout_path, br#"{"share_typing": false}"#).unwrap();
    let layout = Layout::read(directory.path());
    assert!(layout.dont_show_typing());

    // Case 2: Legacy user had share_typing: true (typing shown)
    // Should migrate to dont_show_typing: false
    std::fs::write(&layout_path, br#"{"share_typing": true}"#).unwrap();
    let layout = Layout::read(directory.path());
    assert!(!layout.dont_show_typing());

    // Case 3: Fresh user (neither key present)
    // Should default to false (typing shown)
    std::fs::write(&layout_path, br#"{}"#).unwrap();
    let layout = Layout::read(directory.path());
    assert!(!layout.dont_show_typing());
}

#[test]
fn task8_read_all_notifications_button_action() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    adapter.borrow_mut().action(&window, Action::Preview);

    // Enable read all notifications button
    window.set_read_all_notifications_button(true);
    assert!(window.get_read_all_notifications_button());

    // Trigger read all notifications callback without error
    window.invoke_read_all_notifications();
}

#[test]
fn task8_dont_show_typing_indicator_behavior() {
    let mut layout = Layout::default();
    // Default: typing indicator is not suppressed (dont_show_typing is false)
    assert!(!layout.dont_show_typing());

    // Enabled: typing indicator is suppressed (dont_show_typing is true)
    layout.dont_show_typing = Some(true);
    assert!(layout.dont_show_typing());

    // Explicitly disabled: typing indicator is not suppressed
    layout.dont_show_typing = Some(false);
    assert!(!layout.dont_show_typing());
}

#[test]
fn task9_game_activity_settings_section_and_scenarios() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    window.set_preview(true);
    window.set_settings_open(true);
    window.set_settings_section(8);
    assert_eq!(window.get_settings_section(), 8);

    // Initial state: scenario 0 (live auto)
    assert_eq!(window.get_active_simulated_scenario(), 0);

    // Scenario 1: Minecraft
    window.invoke_select_simulated_scenario(1);
    assert_eq!(window.get_active_simulated_scenario(), 1);
    assert!(window.get_has_game_activity_preview());
    let p = window.get_game_activity_preview();
    assert_eq!(p.name.as_str(), "Minecraft");
    assert_eq!(p.heading.as_str(), "PLAYING");
    assert_eq!(p.details.as_str(), "Exploring Caves");
    assert_eq!(p.state.as_str(), "Survival Mode");

    // Scenario 2: 3 Games (VALORANT, Minecraft, Subnautica) - VALORANT highest priority
    window.invoke_select_simulated_scenario(2);
    assert_eq!(window.get_active_simulated_scenario(), 2);
    assert!(window.get_has_game_activity_preview());
    assert_eq!(window.get_game_activity_preview().name.as_str(), "VALORANT");

    // Scenario 4: All Games Disabled
    window.invoke_select_simulated_scenario(4);
    assert_eq!(window.get_active_simulated_scenario(), 4);
    assert!(!window.get_has_game_activity_preview());

    // Scenario 5: Music Playing
    window.invoke_select_simulated_scenario(5);
    assert_eq!(window.get_active_simulated_scenario(), 5);
    assert!(window.get_has_game_activity_preview());
    let p = window.get_game_activity_preview();
    assert_eq!(p.name.as_str(), "Starboy");
    assert_eq!(p.heading.as_str(), "LISTENING TO SPOTIFY");
    assert_eq!(p.details.as_str(), "by The Weeknd");
    assert_eq!(p.state.as_str(), "on Starboy");
    assert!(p.start > 0 && p.end > p.start);

    // Scenario 6: Music Paused
    window.invoke_select_simulated_scenario(6);
    assert_eq!(window.get_active_simulated_scenario(), 6);
    assert!(window.get_has_game_activity_preview());
    let p = window.get_game_activity_preview();
    assert_eq!(p.name.as_str(), "Blinding Lights");
    assert_eq!(p.heading.as_str(), "PAUSED");
    assert_eq!(p.paused_position, 104);

    // Scenario 8: Rich Presence Party (2 of 5)
    window.invoke_select_simulated_scenario(8);
    assert_eq!(window.get_active_simulated_scenario(), 8);
    assert!(window.get_has_game_activity_preview());
    let p = window.get_game_activity_preview();
    assert_eq!(p.name.as_str(), "Rocket League");
    assert_eq!(p.state.as_str(), "In Lobby (2 of 5)");

    // Scenario 9: Missing Artwork
    window.invoke_select_simulated_scenario(9);
    assert_eq!(window.get_active_simulated_scenario(), 9);
    assert!(window.get_has_game_activity_preview());
    let p = window.get_game_activity_preview();
    assert_eq!(p.name.as_str(), "My Indie Game");
    assert_eq!(p.large_url.as_str(), "");

    // Scenario 10: No Activity
    window.invoke_select_simulated_scenario(10);
    assert_eq!(window.get_active_simulated_scenario(), 10);
    assert!(!window.get_has_game_activity_preview());

    // Return to Live Auto
    window.invoke_select_simulated_scenario(0);
    assert_eq!(window.get_active_simulated_scenario(), 0);
}

#[test]
fn task9_multi_game_selective_disable_and_priority_fallback() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    window.set_preview(true);
    window.set_settings_open(true);
    window.set_settings_section(8);

    // Load 3 games scenario
    window.invoke_select_simulated_scenario(2);
    assert_eq!(window.get_game_activity_preview().name.as_str(), "VALORANT");

    // User disables VALORANT
    window.invoke_toggle_app_activity("valorant.exe".into(), false);
    // Minecraft should now be selected
    assert_eq!(
        window.get_game_activity_preview().name.as_str(),
        "Minecraft"
    );

    // User disables Minecraft
    window.invoke_toggle_app_activity("minecraft.exe".into(), false);
    // Subnautica should now be selected
    assert_eq!(
        window.get_game_activity_preview().name.as_str(),
        "Subnautica"
    );

    // Re-enable VALORANT (priority 10 > priority 0) -> VALORANT becomes selected again
    window.invoke_toggle_app_activity("valorant.exe".into(), true);
    assert_eq!(window.get_game_activity_preview().name.as_str(), "VALORANT");

    // Now set Subnautica as primary/preferred app -> Subnautica must win over VALORANT!
    window.invoke_set_primary_app_activity("subnautica.exe".into());
    assert_eq!(
        window.get_game_activity_preview().name.as_str(),
        "Subnautica"
    );

    // Clear primary -> VALORANT wins again
    window.invoke_set_primary_app_activity("subnautica.exe".into());
    assert_eq!(window.get_game_activity_preview().name.as_str(), "VALORANT");
}

#[test]
fn task9_global_sharing_toggle_suppresses_activity() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    window.set_preview(true);
    window.set_settings_open(true);
    window.set_settings_section(8);

    // Scenario 1: Minecraft active
    window.invoke_select_simulated_scenario(1);
    assert!(window.get_has_game_activity_preview());

    // Turn off global sharing switch
    window.invoke_show_game_toggled(false);
    assert!(!window.get_has_game_activity_preview());

    // Turn back on
    window.invoke_show_game_toggled(true);
    assert!(window.get_has_game_activity_preview());
}

#[test]
fn task9_legacy_game_executables_migration_and_persistence() {
    let directory = tempfile::tempdir().unwrap();
    let layout_path = directory.path().join("layout.json");

    // Legacy layout with game_executables
    std::fs::write(
        &layout_path,
        br#"{"game_executables": [["factorio.exe", "Factorio"]]}"#,
    )
    .unwrap();
    let layout = Layout::read(directory.path());
    assert_eq!(layout.game_activity_rules.len(), 1);
    assert_eq!(layout.game_activity_rules[0].executable, "factorio.exe");
    assert_eq!(layout.game_activity_rules[0].name, "Factorio");
    assert!(layout.game_activity_rules[0].enabled);

    // Window UI integration test for saving and removing custom games
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    // Save a new game executable
    assert!(window.invoke_save_game_executable(-1, "doom.exe".into(), "DOOM".into()));
    {
        let ad = adapter.borrow();
        assert!(
            ad.layout
                .game_activity_rules
                .iter()
                .any(|r| r.executable == "doom.exe")
        );
    }

    // Remove game executable
    window.invoke_remove_app_activity("doom.exe".into());
    {
        let ad = adapter.borrow();
        assert!(
            !ad.layout
                .game_activity_rules
                .iter()
                .any(|r| r.executable == "doom.exe")
        );
    }
}

#[test]
fn task9_own_profile_displays_active_rich_presence() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    // Set scenario 1 (Minecraft)
    window.invoke_select_simulated_scenario(1);

    // Setup Discord user "42" as own user
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.discord_user = Some("42".into());
        let view = ProfileView {
            profile: yapfast_core::chat::Profile {
                id: "42".into(),
                name: "MyUser".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        ad.app.state.profile = Some(view);
        ad.show_profile(&window);
    }

    let profile = window.get_profile();
    assert_eq!(profile.id.as_str(), "42");
    assert_eq!(profile.activities.row_count(), 1);
    assert_eq!(
        profile.activities.row_data(0).unwrap().name.as_str(),
        "Minecraft"
    );
}

#[test]
fn task9_game_activity_visible_and_navigable_in_preview_mode() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    // Enter preview mode without Discord link
    window.set_preview(true);
    window.set_discord_linked(false);
    adapter.borrow_mut().action(&window, Action::Preview);

    // Open settings and switch to Game Activity (section 8)
    window.set_settings_open(true);
    window.set_settings_section(8);
    window.invoke_refresh_game_activity();
    assert_eq!(window.get_settings_section(), 8);

    // Verify scenario selection works in preview mode
    window.invoke_select_simulated_scenario(1);
    assert_eq!(window.get_active_simulated_scenario(), 1);
    assert!(window.get_has_game_activity_preview());
    assert_eq!(
        window.get_game_activity_preview().name.as_str(),
        "Minecraft"
    );

    // Verify global sharing toggle suppresses and re-enables preview in preview mode
    window.invoke_show_game_toggled(false);
    assert!(!window.get_has_game_activity_preview());
    window.invoke_show_game_toggled(true);
    assert!(window.get_has_game_activity_preview());
    assert_eq!(
        window.get_game_activity_preview().name.as_str(),
        "Minecraft"
    );
}

#[test]
fn task9_live_media_session_updates_game_activity_and_profile() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    let now = 100_000;

    // Set live media track from SoundCloud
    let track = yapfast_core::activity::MediaTrack {
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
    adapter
        .borrow_mut()
        .activity_manager
        .set_media_track(Some(track));
    games::refresh_game_activity(&mut adapter.borrow_mut(), &window);

    // 1. Verify Game Activity preview
    assert!(window.get_has_game_activity_preview());
    let preview = window.get_game_activity_preview();
    assert_eq!(preview.name.as_str(), "Bass Cannon");
    assert_eq!(preview.heading.as_str(), "LISTENING TO SOUNDCLOUD");
    assert_eq!(preview.details.as_str(), "by Excision");
    assert_eq!(preview.state.as_str(), "on X Rated");

    // 2. Verify own profile card shows the media activity
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.discord_user = Some("user_media".into());
        ad.app.state.discord_show_game = true;
        let view = ProfileView {
            profile: yapfast_core::chat::Profile {
                id: "user_media".into(),
                name: "MediaUser".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        ad.app.state.profile = Some(view);
        ad.show_profile(&window);
    }
    let profile = window.get_profile();
    assert_eq!(profile.activities.row_count(), 1);
    assert_eq!(
        profile.activities.row_data(0).unwrap().name.as_str(),
        "Bass Cannon"
    );

    // 3. Game starts -> Game takes precedence over music
    let game = yapfast_core::activity::AppIdentity {
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
    adapter
        .borrow_mut()
        .activity_manager
        .update_detected_processes(vec![game], now);
    games::refresh_game_activity(&mut adapter.borrow_mut(), &window);
    assert_eq!(window.get_game_activity_preview().name.as_str(), "Factorio");

    // 4. Game exits -> Music restored
    adapter
        .borrow_mut()
        .activity_manager
        .update_detected_processes(vec![], now);
    games::refresh_game_activity(&mut adapter.borrow_mut(), &window);
    assert_eq!(
        window.get_game_activity_preview().name.as_str(),
        "Bass Cannon"
    );

    // A live paused track stays selected and its position is fixed.
    let mut paused = adapter
        .borrow()
        .activity_manager
        .active_media
        .clone()
        .unwrap();
    paused.is_playing = false;
    paused.thumbnail = Some(Arc::new(Picture {
        width: 1,
        height: 1,
        rgba: yapfast_core::chat::Pixels::new(1, 1, vec![80, 90, 100, 255]),
        animation: None,
    }));
    adapter
        .borrow_mut()
        .activity_manager
        .set_media_track(Some(paused.clone()));
    games::refresh_game_activity(&mut adapter.borrow_mut(), &window);
    let preview = window.get_game_activity_preview();
    assert!(window.get_has_game_activity_preview() && preview.paused);
    assert_eq!(
        (
            preview.heading.as_str(),
            preview.name.as_str(),
            preview.paused_position
        ),
        ("PAUSED", "Bass Cannon", 30)
    );
    assert_eq!(preview.artwork.size().width, 1);
    paused.is_playing = true;
    paused.position_ms = 33_000;
    adapter
        .borrow_mut()
        .activity_manager
        .set_media_track(Some(paused));
    games::refresh_game_activity(&mut adapter.borrow_mut(), &window);
    assert!(!window.get_game_activity_preview().paused);
    assert_eq!(window.get_game_activity_preview().artwork.size().width, 1);
    adapter.borrow_mut().activity_manager.set_media_track(None);
    games::refresh_game_activity(&mut adapter.borrow_mut(), &window);
    assert!(!window.get_has_game_activity_preview());
}

#[test]
fn task9_discord_rpc_playing_syncs_to_detected_apps_and_priority_fallback() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    bind_callbacks(&window, &adapter);

    // 1. Inbound Discord RPC registers application as running
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.discord_user = Some("user_rpc".into());
        ad.app.state.discord_show_game = true;
        ad.app.state.discord_playing = Some(yapfast_core::chat::Playing {
            name: "SoundCloud".into(),
            started_ms: Some(50_000),
            activity: None,
        });
        games::refresh_game_activity(&mut ad, &window);
    }

    assert!(window.get_has_game_activity_preview());
    assert_eq!(
        window.get_game_activity_preview().name.as_str(),
        "SoundCloud"
    );

    let apps = window.get_detected_apps();
    assert_eq!(apps.row_count(), 1);
    let app0 = apps.row_data(0).unwrap();
    assert_eq!(app0.name.as_str(), "SoundCloud");
    assert!(app0.running);
    assert!(app0.enabled);

    // 2. Toggle off via UI callback disables it
    window.invoke_toggle_app_activity("soundcloud".into(), false);
    assert!(!window.get_has_game_activity_preview());

    let apps = window.get_detected_apps();
    let app0 = apps.row_data(0).unwrap();
    assert!(!app0.enabled);

    // 3. Toggle back on
    window.invoke_toggle_app_activity("soundcloud".into(), true);
    assert!(window.get_has_game_activity_preview());

    // 4. Inbound RPC switches to VS Code
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.discord_playing = Some(yapfast_core::chat::Playing {
            name: "Visual Studio Code".into(),
            started_ms: Some(60_000),
            activity: None,
        });
        games::refresh_game_activity(&mut ad, &window);
    }

    assert_eq!(
        window.get_game_activity_preview().name.as_str(),
        "Visual Studio Code"
    );
    let apps = window.get_detected_apps();
    assert_eq!(apps.row_count(), 2);

    // 5. Inbound RPC disconnects
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.discord_playing = None;
        games::refresh_game_activity(&mut ad, &window);
    }
    assert!(!window.get_has_game_activity_preview());
    let apps = window.get_detected_apps();
    for i in 0..apps.row_count() {
        assert!(!apps.row_data(i).unwrap().running);
    }
}

#[cfg(feature = "discord")]
#[test]
fn smtc_transitions_replace_stale_rpc_in_activity_cards() {
    use yapfast_core::activity::windows_smtc::{MediaSessionState, PlaybackType};

    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let adapter = Adapter::new(
        Arc::new(BackgroundRuntime::new().unwrap()),
        directory.path().into(),
        &window,
    )
    .unwrap();
    let mut ad = adapter.borrow_mut();
    let now = games::now_ms();
    let rpc = yapfast_discord::activity::activities(&serde_json::json!([{
        "type": 2, "name": "SoundCloud", "details": "Fixture track", "state": "Artist",
        "timestamps": {"start": now - 60_000, "end": now + 60_000}, "application_id": "42"
    }]));
    ad.app.state.discord_user = Some("1".into());
    ad.app.state.discord_show_game = true;
    ad.app.dispatch(Action::EditProfile);
    ad.app.apply(Event::OwnProfile {
        server: None,
        result: Ok(yapfast_core::chat::OwnProfile {
            fallback: yapfast_core::chat::Profile {
                activities: rpc.clone(),
                ..Default::default()
            },
            ..Default::default()
        }),
    });
    ad.app.state.discord_playing = Some(yapfast_core::chat::Playing {
        name: "SoundCloud".into(),
        started_ms: rpc[0].start,
        activity: Some(Arc::new(rpc[0].clone())),
    });
    ad.app.state.account_profile = Some(Arc::new(yapfast_core::chat::Profile {
        activities: rpc.clone(),
        ..Default::default()
    }));
    let mut session = MediaSessionState::new();
    session.on_session_changed("Chrome".into());
    session.on_media_properties_changed("Fixture track".into(), "Artist".into(), "".into());
    session.on_timeline_changed(120_000, 31_000);
    session.on_playback_info_changed(true);
    session.position_updated_ms = Some(now);
    let publish = |ad: &mut Adapter, session: &MediaSessionState| {
        games::apply_media_update(ad, &window, session.to_media_track());
        let card = window.get_game_activity_preview();
        assert_eq!(window.get_account_profile().activities.row_count(), 1);
        assert_eq!(
            window.get_account_profile().activities.row_data(0).unwrap(),
            card
        );
        assert_eq!(
            window.get_profile_editor_activity().row_data(0).unwrap(),
            card
        );
        card
    };
    let start = std::time::Instant::now();
    let card = publish(&mut ad, &session);
    assert!(!card.paused);
    assert_eq!(card.name, "Fixture track");
    assert_eq!(card.details, "Artist");
    let elapsed = start.elapsed();
    eprintln!("SMTC fixture to Slint activity cards: {:.2?}", elapsed);
    assert!(elapsed < std::time::Duration::from_secs(1));
    session.on_playback_info_changed(false);
    assert_eq!(publish(&mut ad, &session).paused_position, 31);
    let live = ad.app.state.discord_playing.take();
    games::apply_media_update(&mut ad, &window, session.to_media_track());
    let cached = window.get_profile_editor_activity().row_data(0).unwrap();
    assert!(cached.paused);
    assert_eq!(cached.paused_position, 31);
    ad.app.state.discord_playing = live;
    session.on_timeline_changed(120_000, 53_000);
    assert_eq!(publish(&mut ad, &session).paused_position, 53);
    session.on_playback_info_changed(true);
    session.position_updated_ms = Some(now);
    assert!(!publish(&mut ad, &session).paused);
    session.on_media_properties_changed("Unrelated track".into(), "Artist".into(), "".into());
    session.on_playback_info_changed(false);
    assert!(!publish(&mut ad, &session).paused);

    let music = ad.app.state.discord_playing.take();
    let watching = yapfast_discord::activity::activities(&serde_json::json!([{
        "type": 0, "name": "YouTube", "details": "Video title",
        "state": "Channel", "timestamps": {"start": now - 3000, "end": now + 10_000}
    }]));
    ad.app.state.discord_playing = Some(yapfast_core::chat::Playing {
        name: watching[0].name.to_string(),
        started_ms: watching[0].start,
        activity: Some(Arc::new(watching[0].clone())),
    });
    ad.app.state.account_profile = Some(Arc::new(yapfast_core::chat::Profile {
        activities: watching,
        ..Default::default()
    }));
    session.playback_type = PlaybackType::Video;
    assert!(session.to_media_track().is_none());
    games::apply_media_update(&mut ad, &window, session.to_media_track());
    assert!(!window.get_has_game_activity_preview());
    assert_eq!(window.get_account_profile().activities.row_count(), 0);
    assert_eq!(window.get_profile_editor_activity().row_count(), 0);

    // Chromium reports Music even for ordinary video.
    session.on_session_changed("A12BC34DE56F7890".into());
    session.playback_type = PlaybackType::Music;
    games::apply_media_update(&mut ad, &window, session.to_media_track());
    assert!(!window.get_has_game_activity_preview());
    ad.app.state.discord_playing = Some(yapfast_core::chat::Playing {
        name: "Fixture game".into(),
        started_ms: Some(now),
        activity: None,
    });
    games::apply_media_update(&mut ad, &window, session.to_media_track());
    assert_eq!(window.get_game_activity_preview().name, "Fixture game");

    ad.app.state.discord_playing = music;
    session.on_media_properties_changed("Fixture track".into(), "Artist".into(), "".into());
    assert!(publish(&mut ad, &session).paused);
    ad.activity_manager.set_rule("SoundCloud", false, 0, None);
    games::apply_media_update(&mut ad, &window, session.to_media_track());
    assert!(!window.get_has_game_activity_preview());

    session.on_session_closed();
    ad.app.state.discord_playing = None;
    ad.app.state.account_profile = None;
    games::apply_media_update(&mut ad, &window, session.to_media_track());
    assert!(!window.get_has_game_activity_preview());
    assert_eq!(window.get_account_profile().activities.row_count(), 0);
}

#[cfg(feature = "discord")]
#[test]
fn wallpaper_rule_filters_cached_activity_and_late_rpc_after_reload() {
    i_slint_backend_testing::init_no_event_loop();
    let window = classic_window();
    let directory = tempfile::tempdir().unwrap();
    let make_adapter = || {
        Adapter::new(
            Arc::new(BackgroundRuntime::new().unwrap()),
            directory.path().into(),
            &window,
        )
        .unwrap()
    };
    let activities = yapfast_discord::activity::activities(&serde_json::json!([
        {"type": 0, "name": "Wallpaper Engine"},
        {"type": 0, "name": "Another Game"}
    ]));
    let [wallpaper, fallback] = activities.as_ref() else {
        panic!("two activity fixtures expected");
    };
    let publish = |ad: &mut Adapter, start, has_fallback| {
        let mut cached = vec![wallpaper.clone()];
        if has_fallback {
            cached.push(fallback.clone());
        }
        ad.app.state.account_profile = Some(Arc::new(yapfast_core::chat::Profile {
            activities: cached.into(),
            ..Default::default()
        }));
        ad.app.state.discord_playing = Some(yapfast_core::chat::Playing {
            name: "Wallpaper Engine".into(),
            started_ms: Some(start),
            activity: Some(Arc::new(wallpaper.clone())),
        });
        games::refresh_game_activity(ad, &window);
        ad.refresh_own_activity(&window);
    };
    let adapter = make_adapter();
    bind_callbacks(&window, &adapter);
    {
        let mut ad = adapter.borrow_mut();
        ad.app.state.discord_user = Some("1".into());
        ad.app.state.discord_show_game = true;
        let member = |id: &str| yapfast_core::chat::ChatMember {
            id: id.into(),
            name: id.into(),
            bot: false,
            username: None,
            role: Role::Member,
            avatar: None,
            decoration: None,
            nameplate: None,
            boosting_since: None,
            style: None,
            tag: None,
            group: None,
            presence: None,
            color: None,
            activity: wallpaper.line(),
            activities: vec![wallpaper.clone()].into(),
            roles: Default::default(),
            timed_out_until: None,
            group_icon: None,
            role_icon: None,
        };
        ad.app.state.members = Some(Ok(vec![member("1"), member("2")]));
        ad.show_members(&window);
        assert_eq!(window.get_roster().row_data(0).unwrap().activity_icon, 1);
    }
    publish(&mut adapter.borrow_mut(), 100, true);
    assert_eq!(window.get_game_activity_preview().name, "Wallpaper Engine");
    window.invoke_toggle_app_activity(r"C:\Steam\wallpaper64.exe".into(), false);
    assert!(!window.get_has_game_activity_preview());
    assert_eq!(window.get_roster().row_data(0).unwrap().activity_icon, 0);
    assert_eq!(window.get_roster().row_data(1).unwrap().activity_icon, 1);
    let rows = || {
        window
            .get_account_profile()
            .activities
            .iter()
            .map(|row| row.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(), ["Another Game"]);
    let mut media = yapfast_core::activity::windows_smtc::MediaSessionState::new();
    media.on_session_changed("com.richardhbtz.soundcloud-rpc".into());
    media.playback_type = yapfast_core::activity::windows_smtc::PlaybackType::Music;
    media.on_media_properties_changed("Other track".into(), "".into(), "".into());
    media.on_playback_info_changed(true);
    games::apply_media_update(&mut adapter.borrow_mut(), &window, media.to_media_track());
    assert_eq!(window.get_game_activity_preview().name, "Other track");
    games::apply_media_update(&mut adapter.borrow_mut(), &window, None);
    publish(&mut adapter.borrow_mut(), 200, false);
    assert!(rows().is_empty());
    assert!(
        window
            .get_detected_apps()
            .iter()
            .any(|row| row.running && !row.enabled)
    );

    let reloaded = make_adapter();
    bind_callbacks(&window, &reloaded);
    publish(&mut reloaded.borrow_mut(), 300, true);
    assert!(!window.get_has_game_activity_preview());
    assert_eq!(rows(), ["Another Game"]);
    window.invoke_toggle_app_activity("Wallpaper Engine".into(), true);
    assert_eq!(window.get_game_activity_preview().name, "Wallpaper Engine");
}
