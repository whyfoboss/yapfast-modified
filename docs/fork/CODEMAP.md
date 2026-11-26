# Fork code map

Verified by reading the checkout on 2026-11-09 (branch `main`, commit before
the fork docs). Paths are relative to the repository root. Line numbers are
starting points, not API guarantees. Items marked UNVERIFIED were not exercised
in this session.

## Flow in one line

`ui/*.slint` callback -> `src/ui/callbacks.rs` -> `Action` (or `Command`) ->
`crates/yapfast-core/src/application.rs::dispatch` -> `Effect` / `Command` ->
`crates/yapfast-discord/src/service.rs` -> gateway/HTTP -> `Event`/`Live` ->
`Application::apply` -> `src/ui/refresh.rs` -> models and properties in Slint.

The UI crate is `crates/yapfast-ui`; it compiles `ui/yapfast.slint` from
`crates/yapfast-ui/build.rs`. Rust UI code lives in `src/ui.rs` and `src/ui/`.

## 1. Settings screens and persistence

- Files: `ui/settings.slint`, `src/ui.rs`, `src/ui/callbacks.rs`,
  `src/ui/appearance.rs`, `docs/chat-ui.md`.
- `SettingsScreen` (`ui/settings.slint:356`) owns `settings-section`; 4 is
  Discord, 9 is Plugins. `SettingsFrame` (`:400`) and `SettingsNavigation` show the rail.
  Rows with Discord come from `with-discord` (`:373`); without it,
  `without-discord`.
- Plugins page: `PluginsSettings` (`ui/settings.slint:1540`) groups behavioral
  plugins and extensions: Better Animations, Read All Notifications Button,
  Role Colours Everywhere, Message Logger (with Clear Log), Show Hidden Channels,
  Don't Show When You're Typing, and an **Experimental** section containing Fake
  Deafen (`fake_deafen_plugin` in `Layout`, off by default).
- Read All button: `ReadAllButton` (`ui/rail.slint:845`) is placed in `ServerRail`
  below the DM item and above the server list separator. Enabled when
  `read_all_notifications_button` is on. Dispatches `Action::ReadAllNotifications`
  (`crates/yapfast-core/src/application/rooms.rs::mark_all_read`) marking all
  unread guild channels and threads read while excluding DMs.
- Discord page: `DiscordSettings` (`ui/settings.slint:1450`) with
  `sync-discord-reads`, `show-idle`, `hide-blocked`,
  `BlockedRow`, unlink. Account linking lives in `DiscordLinkPanel`
  (`ui/discord.slint`), used at `ui/settings.slint:1383`.
- Persistence: `Layout` (`src/ui.rs:1130`) is the whole local settings file.
  `LAYOUT = "layout.json"` (`src/ui.rs:1103`), stored under the data directory
  (`yapfast_core::default_directory`, `crates/yapfast-core/src/lib.rs:98`).
  `Layout::read` parses it and repairs custom themes; `Layout::write`
  (`src/ui.rs:1641`) serialises it. `LAYOUT_WRITE` is 1000 ms and
  `write_layout_soon` (`src/ui.rs:2919`) debounces rapid writes.
- Example: `on_hide_blocked_toggled` (`src/ui/callbacks.rs:2275`) writes the
  `Layout` field, saves, then dispatches `Action::HideBlocked` so
  `AppState::hide_blocked` (`crates/yapfast-core/src/application.rs:522`)
  re-renders. `show-hidden-channels`, `sync-discord-reads`, `show-idle` follow
  the same pattern (`src/ui/callbacks.rs:1848`, `:2285`, `:2295`).
- Settings that only affect drawing are re-applied by `src/ui/refresh.rs`
  (for example `set_show_hidden_channels` at `:784`).

## 2. Theme

- Files: `ui/theme.slint`, `src/ui/theme.rs`, `src/ui/appearance.rs`,
  `ui/settings.slint`, `docs/themes.md`, `docs/yapfast-ui.md`.
- `export global Theme` in `ui/theme.slint` lists every colour (rail,
  sidebar, panel, chat, raised, text, accent, and so on). Widgets read the
  global; no widget owns a palette.
- Built-ins: `BuiltIn` (`src/ui/theme.rs:599`) and
  `BUILT_IN: [BuiltIn; 11]` (`:607`): `yap`, `onyx`, `bonfire`,
  `catppuccin-mocha`, `dracula`, `nord`, `gruvbox-dark`, `tokyo-night`,
  `catppuccin-latte`, `rose-pine-dawn`, `sakura`. `Theme` starts at `:26`;
  `lookup` `:670`, `chosen` `:683`, custom `:731`, `tidy` `:822`,
  `starts` `:845`, `cards` `:907`.
- Custom format: `custom_themes` in `layout.json`; each has `id`, `name`,
  `base`, `dark`, `colours` (for example `{"id":"custom:1","name":"Plum",
  "base":"yap","dark":true,"colours":{"chat":"#2a1a3a"}}`). Missing colours
  take the base theme's value. Ids are `custom:N`; `tidy` repairs collisions.
- Picker: `ThemeCard` (`ui/settings.slint:782`), `ThemeTile` (`:808`), editor
  rows `:883`, `AppearanceSettings` (`:915`). `src/ui/appearance.rs` wires
  choose/edit/save/delete to `Layout.theme` and `Layout.custom_themes`.
  The handler writes `layout.json` immediately.

## 3. i18n status

- Status: no internationalisation yet. `rg` finds no `@tr(` in `ui/` or
  `src/`, and no `gettext`/`translations` configuration in
  `crates/yapfast-ui/build.rs`. Every visible string is a literal in
  `.slint` (for example `SettingsTitle { text: "Appearance"; }`) or in Rust
  UI refresh/formatting code.
- Build: `crates/yapfast-ui/build.rs` calls
  `slint_build::compile_with_config("../../ui/yapfast.slint",
  CompilerConfiguration::new().with_style("fluent".into()))`. There is no
  `with_bundled_translations(...)`, no `translations/` directory, and no
  `init_translations!`.
- Pinned Slint support: the git Slint 1.19 checkout used by Cargo supports
  bundled translations. `slint_build::CompilerConfiguration::with_bundled_translations`
  (`api/rs/build/lib.rs:226`) expects
  `translations/<lang>/LC_MESSAGES/<crate>.po`; the public runtime side is
  `select_bundled_translation` and `update_all_translations` (names from the
  Slint API; exact calls are UNVERIFIED in this checkout). The `gettext`
  Cargo feature exists but is not enabled by
  `crates/yapfast-ui/Cargo.toml`.
- Vendored runtime: `vendor/i-slint-core/translations.rs` has `translate`,
  `TranslationsBundled`, `select_bundled_translation`, the bundled-frame and
  plural APIs, and `mark_translations_dirty`; `tr` is an optional dependency
  (`vendor/i-slint-core/Cargo.toml:117`). The vendored core is patched into
  the build through `[patch."https://github.com/slint-ui/slint"]`.
- UNVERIFIED: no translation has been built or loaded in this checkout.

## 4. Friends, requests, profiles

- Files: `ui/friends.slint`, `src/ui/refresh.rs`, `src/ui/callbacks.rs`,
  `crates/yapfast-core/src/application.rs`,
  `crates/yapfast-core/src/application/profiles.rs`,
  `crates/yapfast-core/src/application/members.rs`, `docs/discord/people.md`.
- `RequestRow` (`ui/friends.slint:7`) carries `id`, `name`, `friend` (0 none,
  1 incoming, 2 sent), `room`, `message`, `unread`; `RequestItem` (`:81`) has
  accept/remove/open. `FriendItem` (`:165`) has message/open-voice/context
  menu; `FriendsScreen` (`:236`), `RequestsScreen` (`:326`).
- Rows are built in `src/ui/refresh.rs`: incoming/sent/spam models
  (`:887-889`) and friend rows around `:872`; data comes from
  `AppState::friend_requests` (`crates/yapfast-core/src/application.rs:478`)
  and `state.friends`.
- Profile opening: `on_show_friend_profile` (`src/ui/callbacks.rs:1081`)
  dispatches `Action::ShowFriendProfile` (`application.rs:2161`) and
  `Action::ShowProfile` (`:2149`); both reach
  `Application::show_profile` (`crates/yapfast-core/src/application/profiles.rs:8`).
  `Action::ShowDmProfile` opens the DM's profile panel.
- A friend request has no profile popup today: clicking opens the request
  conversation/DM (`open`), not the person card.

## 5. Message search

- Files: `crates/yapfast-discord/src/search.rs`, `crates/yapfast-core/src/application.rs`,
  `crates/yapfast-discord/src/service.rs`, `ui/header.slint`,
  `testing/fake_discord.ts`, `docs/discord/messages.md`.
- Query parser: `search::Query` with `content`, `from`, `mentions`, `in`,
  `has`, `min_id`/`max_id` (from `after`/`before`/`during`), `pinned`;
  `Query::parse`; `HAS` lists link, embed, poll, file, video, image, sound,
  sticker, forward (wire name `snapshot`). It becomes
  `GET /guilds/{id}/messages/search` or
  `GET /channels/{id}/messages/search` (`search.rs` module docs).
- Core state: `Search` (`application.rs:254`) with `place`, `query`, `total`,
  `results: Vec<chat::SearchHit>`, `next`, `loading`, `error`;
  `search_place` (`:1323`), `search_shown` (`:1332`), actions `SearchHere`,
  `SearchForward` (`:1520`) and `SearchSwitcher` (`:1527`).
- UI: `FoundRow` (`ui/header.slint:9`), search box `HeaderSearch` (`:65`),
  results `SearchPanel` (`:609`, including the "Search here" action that re-runs
  the saved query and bindings in the current room/server); message rows use the
  same `FoundRow`.
- Fake: `testing/fake_discord.ts:110` documents per-channel and per-server
  search, 25 at a time, with content, `has`, `min_id`, `max_id`, `pinned`;
  `searchFilter` (`:1696`) implements it. Filters are Discord names only,
  with no Russian aliases and no suggestion popup.
- UNVERIFIED in this session: UI search interaction; the service tests cover
  search against the fake.

## 6. Composer mention suggestions

- Files: `ui/composer.slint`, `src/ui/refresh.rs`,
  `crates/yapfast-core/src/application.rs`, `ui/controls.slint`.
- Action: `Action::MentionQuery(Option<(char, String)>)`
  (`crates/yapfast-core/src/application.rs:1469`); dispatch `:3087` calls
  `Application::mention_query`.
- Rows: `MentionRow` (`ui/composer.slint:214`) with name, avatar, role colour,
  group/role/channel flags, sigil, voice flag, detail and heading;
  `SuggestionFrame` (`:232`), `MentionList` (`:243`), emoji suggestions
  (`:287`), request bar (`:320`).
- Refresh: `state.mention_suggestions()` -> `MentionRow` model
  (`src/ui/refresh.rs:3589`); headings are MEMBERS, ROLES, CHANNELS; colour
  comes from `candidate.color`. The same frame is reusable for search filter
  suggestions, but it is currently sized and keyed for the composer.

## 7. Voice channels, participant rows, menus, permissions

- Files: `ui/rail.slint`, `ui/voice.slint`, `ui/people.slint`,
  `src/ui/menus.rs`, `src/ui/callbacks.rs`,
  `crates/yapfast-core/src/application/members.rs`,
  `crates/yapfast-core/src/application/server.rs`,
  `crates/yapfast-discord/src/ready.rs`,
  `crates/yapfast-discord/src/service.rs`.
- Channel tree: `RoomRow`/`ChannelRow`/`StatusDot` in `ui/rail.slint`; voice
  channels list their people under the row (`channel.voice`/`voice-people`);
  a row is draggable for channel reordering (`draggable`, `start-drag`,
  `dragged`, `dropped`, `ui/rail.slint:435-523`). That is row reordering,
  not moving people: no voice-person drag exists.
- Participant rows: `PersonRow` struct (`ui/voice.slint:6`),
  `ParticipantGrid` (`:742`), `VoiceDock` (`:334`),
  `CallControls` (`:989`, including the Discord-only experimental Fake Deafen
  toggle between Mute and Deafen when enabled in Plugins settings), `IncomingCall`
  (`:1069`), `StreamPip` (`:581`).
  Voices also appear in the rail and in `ui/people.slint`.
- Menus: `src/ui/menus.rs::menu` (`:78`), `voice_target` (`:518`),
  `move_rows` (`:524`), `choose` (`:810`); bindings
  `on_prepare_menu`/`on_choose_menu` (`src/ui/callbacks.rs:1034`, `:1061`).
  `Application::person_choices` and `voice_person_choices` are in
  `crates/yapfast-core/src/application/members.rs`.
- Permissions: core `Application::may` (`application/server.rs:121`),
  `may_in_server` (`:160`), `outranks` (`:172`), `may_grant` (`:193`);
  Discord `server_permissions` (`ready.rs:1046`),
  `channel_permissions` (`:1069`), `outranks` (`:1155`), `may_grant`
  (`:1173`), plus `KICK_MEMBERS`/`BAN_MEMBERS`/`MOVE_MEMBERS` constants.
- Existing actions: `MemberChange` (`crates/yapfast-core/src/chat.rs:3619`)
  with move/kick/ban/timeout/nickname; service `move_member`, `kick`, `ban`
  (`crates/yapfast-discord/src/service.rs:2339`, `:2345`, `:2350`).
  A member move is `PATCH /guilds/{id}/members/{user}` with
  `{"channel_id": ...}`.

## 8. Message rows, author colour, role data

- Files: `ui/messages.slint`, `src/ui/messages.rs`, `src/ui/refresh.rs`,
  `crates/yapfast-core/src/chat.rs`, `ui/controls.slint`.
- `MessageRow` (`ui/messages.slint:732`) includes `author`, `sender`,
  `color`/`has-color`, `name-style`, `bot`, `tag`, reply fields, and
  `author-styled`/`time-styled` for selectable text. The name is drawn with
  the role colour at `:2453` (and styled at `:2461`/`:2465`).
- `src/ui/messages.rs::row` (around `:687`) sets
  `color: role_color(message.color)`,
  `has_color: message.color.is_some()`, and the name style. The core fills
  `ChatMessage.color` from the highest coloured Discord role.
- Member data: `ChatMember` (`crates/yapfast-core/src/chat.rs:1033`) has
  `color` (highest coloured role, `:1061`), `roles: Arc<[String]>` (`:1068`),
  `role_icon`, and profile `roles: Vec<ProfileRole>` (`:861`).
- Role colour is one flat colour: the highest coloured role's, as
  `ChatMember.color` and `Server::color` give it; gradients and every role's
  chips are still not modelled. The per-device "Role colours everywhere"
  switch in `DiscordSettings` (`Layout::role_colors_everywhere`) turns those
  names' colours on across server contexts: messages, notices, replies,
  member and role-member lists, mentions and the composer, search, threads,
  forums, voice, the Activity panel and profile cards opened in a server. It
  is off by default, then names keep their theme colours. Role badges, role
  management, permission screens and DM or global-friend lists stay as they
  are.

## 9. Gateway message delete/update/bulk delete

- Files: `crates/yapfast-discord/src/event.rs`,
  `crates/yapfast-discord/src/service/live.rs`,
  `crates/yapfast-discord/src/service/tracker.rs`,
  `crates/yapfast-core/src/discord_text/open_channel.rs`,
  `crates/yapfast-core/src/application.rs`.
- Event names: `events!` (`event.rs:5`) defines `MessageCreate`,
  `MessageUpdate`, `MessageDelete`, `MessageDeleteBulk`; the parse test is
  `event.rs:150`.
- Handlers: `live.rs` around `:130-160` maps create/update to
  `Live::Created`/`Live::Updated`, delete to `Live::Deleted { channel, id }`,
  and bulk delete to one `Live::Deleted` per id. `tracker.rs` learns names
  from message events (`:791`) and guild messages (`:612`).
- Core: `Live::Deleted` updates the open channel in
  `crates/yapfast-core/src/discord_text/open_channel.rs:1958`;
  `Live::Updated` at `:1925`; `Live::Created` at `:638`; the deleted branch
  in the event loop is `:604`. `Application::apply`/`apply_event` are
  `application.rs:2712` and `:3832`.

## 10. Activities, presence, pictures

- Files: `crates/yapfast-discord/src/activity.rs`,
  `crates/yapfast-discord/src/member_list.rs`,
  `crates/yapfast-discord/src/activity_feed.rs`,
  `crates/yapfast-core/src/chat.rs`, `crates/yapfast-core/src/application.rs`,
  `ui/people.slint`, `ui/friends.slint`, `ui/controls.slint`.
- Parsing: `Activity` (`activity.rs:35`) with `Kind::{Playing, Streaming,
  Listening, Watching, Competing, Hang}`, `name`, `details`, `state`,
  `party`, `application_id`, `start`/`end` (Unix ms), large/small images and
  texts, `url`, `link`, `emoji`; `activities` (`:148`) / `activity` (`:158`).
  `Activity::line`, `spotify`, `heading`; `Subline`/`subline` for the compact
  member-list line. `member_list::activity` (`member_list.rs:400`) produces
  the one-line text.
- Core: `chat::Presence` (`chat.rs:1108`) = Online/Mobile/Idle/DND/Offline;
  `ChatMember.presence` (`:1059`); `FeedEntry`/`FeedPerson` aliases
  (`chat.rs:42`) and `AppState.activity_feed`/`activity_people`
  (`application.rs:527-529`); events `ActivityFeed`/`ActivityFeedDue`
  (`chat.rs:3788`).
- UI: `MemberActivityRow` and the Activity panel (`ui/people.slint:85`,
  `:100`), `ActivityRow` (`:349`), `ActivityClock` (`:367`), `ActivityCard`
  (`:669`) with large/small pictures and elapsed/left time; profile Activity
  tab (`:1140`); friend rows show an activity line
  (`ui/friends.slint:220`).
- Pictures: `PictureStore` global (`ui/controls.slint:241`) with
  `picture(url, revision)`, `request(url)`, `animated`, `frames`, `delays`,
  `plays`; implemented around `src/ui/callbacks.rs:621-642` and
  `src/ui/pictures.rs` (`picture` `:230`). `Picture`/`Motion` components hold
  animation frames and stop timers when held (`ui/controls.slint:257`).

## 11. Go Live and watching streams

- Files: `crates/yapfast-discord/src/stream.rs`,
  `crates/yapfast-discord/src/stream_connection.rs`,
  `crates/yapfast-core/src/stream_worker.rs`,
  `crates/yapfast-core/src/playback.rs`,
  `crates/yapfast-core/src/video_player.rs`,
  `crates/yapfast-core/src/application.rs`, `ui/voice.slint`,
  `docs/discord/go-live.md`.
- Key: `stream::key(guild, channel, user)`, `stream::parse`, and
  `StreamServer` (`stream.rs:13-35`). The worker is a child process:
  `Job` (`stream_worker.rs:76`), `Worker::share` (`:1417`),
  `glance` (`:1444`), `watch` (`:1458`), `play` (`:1481`), `run` (`:357`).
  Frames go through shared `Frames`; preview/watch sizes are constants in
  that file.
- Core state: `AppState.pending_watch: Option<(String, String)>`
  (`application.rs:538`) holds at most one stream to watch; `ShowVoicePerson`
  (`:2091`) starts a watch or shows the call; the pending watch is cleared on
  disconnect (`:3846`). Decode/playback: `Playback`/`Wake` (`playback.rs:16`,
  `:27`), `Progress`/`Playing`/`play` (`video_player.rs:55`, `:246`, `:260`).
- UI: `StreamPip` (`ui/voice.slint:581`) is one picture-in-picture tile;
  call stage and controls in `VoiceDock`/`ParticipantGrid`/`CallControls`
  (`:334`, `:742`, `:989`); `ViewerStack` (`:44`) shows viewers of one
  stream. Multiple simultaneous streams, multiple tiles, and mixing several
  stream audio tracks do not exist.

## 12. Clipboard and the local store

- Files: `src/copy.rs`, `src/ui.rs`, `crates/yapfast-discord/src/cache.rs`,
  `crates/yapfast-discord/src/cookies.rs`,
  `crates/yapfast-core/src/lib.rs`, `docs/sessions.md`,
  `docs/discord/messages.md`.
- Clipboard: `copy::picture` (`src/copy.rs:10`) spawns a `copy-picture`
  thread; `copy` (`:23`) decodes with limits (8192x8192, 256 MiB) and writes
  RGBA to `arboard`; on Linux it waits to serve the image until another copy.
  `Effect::CopyPicture` calls it (`src/ui.rs:2688`). Plain-text copying is
  handled by Slint's selectable text, not by `src/copy.rs`.
- Data directory: `yapfast_core::default_directory` (`lib.rs:99`) uses
  `directories::ProjectDirs::from("io.github", "yapfast", "Yapfast")` and
  `data_local_dir()`; `--data-dir` overrides it (`src/main.rs:142`).
- Files: `layout.json` (all device settings, window placement, per-server
  channel, custom themes, pins, volumes); Discord cookies in
  `discord-cookies.json`; the Discord account snapshot in
  `discord-account.json` (`keep_discord_files`, `lib.rs:106`); Matrix SDK
  stores under the same root.
- Cache caps (`crates/yapfast-discord/src/cache.rs`): pictures 64 MiB
  (`:86`), previews 192 MiB (`:88`), GIFs 200 MiB (`:92`), channels 300
  (`:96`), 100 messages per channel (`:98`), profiles 500 (`:100`), kept
  people 20000 (`crates/yapfast-discord/src/message.rs:1297`). `prune` evicts
  by size/count.
- Message Logger: `MessageLogger` (`crates/yapfast-core/src/discord_text/logger.rs`)
  holds an in-memory log of deleted messages and previous edit versions when
  enabled via `Layout::message_logger`. Memory is bounded: max 500 global entries,
  100 per channel, 5 previous versions per message, 2,000 characters per version,
  and a 16 MiB global ceiling with oldest-first eviction. No message content is
  ever written to disk. The edit history sidebar (`ui/history.slint`, `EditHistoryPanel`)
  docks beside the timeline at 420px width via `Action::ViewEditHistory` /
  `Event::EditHistory` and closes on Escape or its close button. Clear Log
  (`Action::ClearMessageLog`) purges all retained records immediately. Deleted
  messages display body text in `Theme.danger-text` without extra deletion badges,
  and right-clicking offers "Hide this deleted message" (`Action::HideDeletedMessage`)
  and "Hide all deleted messages" (`Action::HideAllDeletedMessages`).
- Profile Editor: `ProfileEditorDialog` (`ui/server.slint`) provides a classic
  centered single-column modal (~600px wide) with banner, avatar overlay,
  display name and pronouns editing with pencil icons, compact Rich Presence /
  game activities directly beneath the username/pronouns, bio editing, member since,
  and a compact "Customize" dialog (`ProfileCustomize`) for advanced customization
  (nameplates, decorations, name styles, profile themes). Ordinary profile viewers
  (`ProfileCard`) remain unchanged.

## 13. Tests and the fake Discord

- Files: `testing/fake_discord.ts`, `testing/fake_discord_env.ts`,
  `crates/yapfast-discord/tests/fake_server.rs`,
  `crates/yapfast-discord/tests/fake_env.rs`,
  `crates/yapfast-core/src/discord_text/tests.rs`,
  `crates/yapfast-core/src/discord_voice/tests.rs`,
  `scripts/test-fake-discord`, `docs/discord/adding-a-feature.md`.
- `testing/fake_discord.ts` is a Bun HTTP/WebSocket server with seeded
  servers, DMs, calls, profiles and media. Its header documents routes and
  `/_control/...` endpoints; `searchFilter` (`:1696`) and the env loader
  (`fake_discord_env.ts`, needs `ffprobe`/`ffmpeg`) are examples to copy.
- Each test starts its own fake: `Fixture::open`/`start_with_games`
  (`fake_server.rs:1238`), whose child is `bun testing/fake_discord.ts`.
  `control`/`post_control` (`:788`, `:779`) drive `/_control`. Tests are
  `#[tokio::test]` + `#[ignore = "starts testing/fake_discord.ts with bun;
  run scripts/test-fake-discord"]`; `WAIT` is 10 s.
- `scripts/test-fake-discord [FILTER]` runs, in order: nextest
  `-p yapfast-discord --test fake_server`, ignored core
  `discord_text::tests`/`discord_voice::tests` against its own fake, then
  `fake_env` against `testing/envs/example.json`.
- Adding a route: give the fake the same Discord shape, add a control for
  anything a test must observe, add a service method/test, then the core
  action/event, then the UI.

## Build and verification notes

- `cargo build --profile fast` in the dev shell: 6 m 24 s from a cold
  `target/` on this machine.
- `scripts/test-fake-discord`: all 61 service tests, 13 core tests and the
  env test pass, 31.82 s warm, when `ffmpeg` is on `PATH`; without it the env
  test aborts in `fake_discord_env.ts` at `ffprobe`.
- `scripts/check --fast`: all steps pass, 246.54 s on this checkout (fmt 1 s,
  doc links 0 s, clippy 150 s, tests 59 s, build 36 s, smoke 0 s).
- `scripts/ui-session` cannot start its private Weston display on this WSL2
  host: the headless GL renderer fails with "EGL surfaceless platform cannot
  be used". With a pixman Weston, Slint's winit backend still forces X11
  under WSL and fails without an X server. A working private check is Xvfb:
  `Xvfb :99`, fake Discord, `target/fast/yapfast --renderer software
  --data-dir /tmp/...`, then `xwd -root`. The resulting
  `.local/ui-session/shots/xvfb-proof.png` shows the real UI and fake data.
