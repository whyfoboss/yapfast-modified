# Fork roadmap

Planned work after the fork bootstrap. It records what exists today and the
order the pieces should be taken in. Nothing here is implemented yet unless a
section says so. Paths and symbols are in `docs/fork/CODEMAP.md`.

Sizes are rough: S is one focused change and its tests, M crosses UI, core and
service, L means media, protocol or a large UI surface. Measure before
promising performance; this host is a WSL2 machine with 11 GiB of RAM.

## Rules for this list

- No real Discord account. Every service change gets a route or control in
  `testing/fake_discord.ts` and a test in the same commit.
- Settings go through `Layout` and `layout.json`; do not add a second store.
- Palette values go in `ui/theme.slint`; do not hardcode a theme colour in a
  widget. Scrims and shadows over media may stay literal.
- Strings: the checkout has no translation layer yet. Until the Russian
  language item lands, follow the existing literal style. Do not invent a
  half-working per-screen translation mechanism.
- A feature is done when the fake test covers it, `scripts/check --fast`
  passes, and the relevant doc is updated.

## Ordered plan

### 1. Onyx-like black-and-white built-in theme (S)

Goal: a monochrome built-in theme in the same family as Discord's Onyx, dark
and neutral, selected from Appearance without custom CSS.

Today: ten built-ins in `src/ui/theme.rs` (`BUILT_IN`, `BuiltIn`), the complete
palette global in `ui/theme.slint`, picker cards in `ui/settings.slint`, and
`docs/themes.md` with a table of every built-in. Custom themes already cover
the persistence path.

Work: add one `BuiltIn` entry with all required fields, a thumbnail/card, and
the docs row; audit contrast for text, muted text, mention, link, danger and
success on each surface. Check the 29 literal `#ffffff` and 28 literal
`#00000080` uses in `ui/`; most are scrims over media, but any that is a
surface colour must move into `Theme`.

Risks: "black and white" must still show presence, role colour, unread and
danger states; a literal-grey palette can erase them. Accessibility review of
focus rings and hover is required. No service or fake work.

Verify: preview all screens with the theme, take an Xvfb screenshot, run
`scripts/check --fast`, update `docs/themes.md`.

### 2. Role colour everywhere, with a Discord settings section (M)

Goal: role colour on every name surface, and toggles that actually change it,
plus the message-logger and animation controls from the same settings area.

Today: the core keeps `ChatMember.color` (highest coloured role) and
`ChatMember.roles`; messages, member lists, profile role rows and mention
suggestions use it. `DiscordSettings` already owns per-device switches and
writes `Layout`. Role colour is one flat colour; gradients and multiple role
chips are not modelled.

Work: first decide the policy (keep the highest role colour, or draw a
gradient when a role has colours). Apply it to the rail, voice participant
rows, friends and request rows, search results, reply previews, mentions,
profiles and the composer. Add a `Layout` field for the switch, a
`DiscordSettings` row, a dispatch path, and apply it in `src/ui/refresh.rs`
so no stale colour survives a toggle. Parse role gradient colours in the
service if the policy needs them, with a fake member whose role has one.

Message logger and better animations are separate sub-items; keep them in
their own commits. A logger needs a bounded local store (the current cache is
an in-memory snapshot with size caps; there is no database dependency in the
workspace) and a clear privacy decision: what is logged, where, for how long,
and how the user deletes it. Animations already have `Animate`
(focused/always/never) and `Theme.quick`; "better" needs concrete cases and a
frame-time check, not a global speed change.

Risks: colours computed in several refresh paths drift; gradient support
touches the service and the member model; a logger can grow without bound and
must never log tokens or message contents unexpectedly. Do not enable logging
by default.

Verify: fake role with a colour and a gradient; tests for the setting
round-trip and refresh; screenshots in the Onyx and the default theme.

### 3. Selectable and copyable usernames (S)

Goal: select or copy any displayed username, with a context-menu action where
selection is awkward.

Today: message author and time are `StyledText` (`ui/messages.slint:2459`,
`:2485`) and the message body is selectable; most names in the rail, member
list, friends, voice rows, search and headers are plain `Text` (audit each
site before changing it). Menus already have Copy ID and profile actions.

Work: add "Copy username" to the person menu (and the message author menu),
using the existing name/tag formatter so the copied value matches what is
shown. For places where the user expects selection, replace `Text` with the
same selectable control used by messages, without stealing clicks from row
open/voice actions. Check keyboard focus order and that selection does not
leave a stuck highlight when a row is recycled.

Risks: Slint selection inside a clickable row can swallow clicks; check the
DelayedClick/hover pattern used by message rows. Do not add a separate
clipboard path: `src/copy.rs` is for images; text should use the existing
selection/arboard convention.

Verify: UI test for the menu callback, manual selection/copy in Xvfb, a check
that a click still opens the profile after copying.

### 4. Mini profile for a friend requester (S)

Goal: clicking the avatar or name in an incoming/sent request opens a compact
profile instead of only opening the DM.

Today: `RequestRow`/`RequestItem` know the user id, name, direction, message
and room; clicking opens the request conversation. Profiles are fetched by
`Application::show_profile` through `service.rs::profile`, which caches a kept
profile and revalidates it; `ShowFriendProfile`/`ShowProfile` already exist.

Work: add a profile button on the request row and dispatch the existing
profile action with the requester's id and the private guild context. Reuse
the existing profile panel/modal at a compact size; show name, avatar, mutual
servers if already available, and the request message. Do not add a new
profile endpoint. If the requester is not yet a friend, load the public
profile and keep the actions limited to Accept/Remove/Message.

Risks: the profile may lack a member face outside a server; the cache key
(`guild`, `user`) must stay correct. Check that opening a profile does not
mark the request as read or accept it by accident.

Verify: fake user profile and request; service test for the profile fetch;
UI test for the callback; Xvfb screenshot.

### 5. Search: Russian filters, suggestions, jump to result N (M)

Goal: type Russian filter names, get suggestions while typing, and jump to
the Nth result.

Today: `search::Query` supports `content`, `from`, `mentions`, `in`, `has`,
`after`/`before`/`during`, `pinned`; the wire filter names are Discord's
English ones. `Search` holds pages with `next`; the panel renders `FoundRow`s.
The composer has a reusable suggestion frame, but search has no suggestion
popup and no result counter/navigation.

Work: add an alias layer in `Query::parse` (for example `от:`, `в:`, `имеет:`,
`после:`, `до:`, `закреплено:`), keeping the canonical wire form and the
English names working. Build suggestions from the same `MentionRow` data
(people, channels, `has` values) and key them to the active filter token, not
only to `@`/`#`. Add a result counter and "jump to result N" that loads pages
until N is available, then asks the existing jump/open-message path to focus
the row. Keep the fake's search route as the source of truth and test aliases
there.

Risks: aliases are not sent to Discord; the parser must not treat a Russian
word before `:` as a filter unless it is known. Jumping to a late result may
require many pages and rate limiting; cap the fetch and show progress. Search
results can expire while pages load; do not mix stale ids.

Verify: service tests for every alias and for the canonical query; fake search
by alias; UI test for suggestion selection and jump; check memory with a
large result set.

### 6. Drag people between voice channels (M)

Goal: drag a participant row from one voice channel to another.

Today: `MemberChange::Move`, `service::move_member` (PATCH member
`channel_id`), `Application::may`/`may_in_server`/`outranks`/`may_grant`, and
the context-menu "Move to" path exist. Only channel rows are draggable, for
reordering; person rows are not drag sources or targets.

Work: make people under a voice channel draggable. On drop over another
voice channel, check `MOVE_MEMBERS` and connect permission for the mover and
the target, then dispatch the existing move action. Show the target highlight,
a pending state and an error toast; let the gateway voice-state event confirm
the move instead of moving the row locally without confirmation. Keep channel
reordering working; distinguish a row drag from a channel drag by the drag
type.

Risks: dragging yourself, dragging a user who outranks you, and moving to a
full or forbidden channel must be rejected in the core, not only greyed out.
Dragging across collapsed categories or auto-scroll needs explicit handling.
Do not add a second move implementation: extend `MemberChange::Move`.

Verify: fake call with two voice channels; tests for allowed/denied moves and
gateway confirmation; UI test for drag callbacks; manual Xvfb check.

### 7. Watch several streams at once (L)

Goal: more than one live stream visible and audible at the same time.

Today: `stream_worker` runs one child process with `share`, `glance`, `watch`
and `play`; `AppState.pending_watch` holds one `(guild, channel)`; the UI has
one `StreamPip`. Frames arrive through a shared `Frames` buffer and playback is
one `Playback`/`video_player`.

Work: first measure one 720p watch (CPU, RSS, frame latency) and read
`docs/discord/go-live.md` and `stream_worker.rs` before choosing a design.
Then make watches keyed by stream, keep N frame buffers with resize and
eviction, add a tile layout and per-stream audio mixing/volume, and stop the
worker for a stream when its tile closes. Keep the first stream's path as the
single-watch fast path.

Risks: memory and decode cost grow per stream; audio mixing and echo
cancellation are easy to get wrong. Discord's protocol may limit how many
streams the client can watch (UNVERIFIED). Multiple simultaneous sockets and
encryption keys add failure modes. This is the first item that needs a
measurement table, not an estimate.

Verify: fake stream fixtures for two publishers; service tests for independent
watch/stop; UI test for two tiles; a recorded RSS/CPU/frame-time measurement
in the commit message.

### 8. Rich presence with images/music, optionally local IPC (L)

Goal: show the current activity with its artwork, track, artist and elapsed
time; optionally publish Yapfast's own activity over local Discord IPC.

Today: inbound activities are parsed (`Activity`, `activities`,
`Activity::spotify`/`line`/`heading`), shown in member rows, the friends list,
profiles and the Activity panel. Large/small images are requested through
`PictureStore` and timers tick elapsed/remaining time. There is no progress
bar, no button row, no track-length parsing beyond start/end, and no local IPC
client.

Work: render the image pair as Discord does, show the music line plus a
progress bar based on `start`/`end`, keep the timer paused when the window is
unfocused if that is the existing animation policy, and handle missing images
without a layout jump. For local IPC, treat it as a separate protocol client
(Unix socket on Linux, named pipe on Windows), request the user's consent, and
never send message content. Implement only after the display part is done.

Risks: start/end are Unix milliseconds and can be absent or stale; the
artwork URL may 404. Local IPC is a new external protocol and platform-specific
file paths; it must not block the UI thread or log the payload. UNVERIFIED:
the exact IPC framing/version we would target.

Verify: fake activities with images and music; service tests for parsing;
UI screenshot; for IPC, a fake local socket server and a test that a missing
socket is non-fatal.

### 9. Russian UI language (L)

Goal: the whole interface in Russian, with the same layout and no clipped text.

Today: zero translation infrastructure. `ui/` has 555 literal `text:`
occurrences (not all are user-visible; some are glyphs and test strings), plus
Rust-side strings for menus, errors, notifications, date/time formatting and
accessibility. Slint 1.19 supports bundled gettext translations through
`with_bundled_translations` and `select_bundled_translation`; the vendored
`i-slint-core` also has the runtime, and the `gettext` feature is not enabled.
`crates/yapfast-ui/build.rs` has no translation setup.

Work, in stages: (a) enable the build/runtime path and prove one translated
string end to end; (b) tag user-visible strings with `@tr` and move Rust-side
strings into a small message table; (c) add `translations/ru/.../*.po` with
contexts and plurals for counts; (d) add a language setting to `Layout` and
the Appearance/Settings screen, defaulting to system language; (e) audit
layout at the longest Russian strings, including buttons, tabs and the
composer.

Risks: 555 literals plus Rust strings is a large mechanical surface that
conflicts with every other UI change, so do it after the UI work above
settles. Russian plurals (1, 2-4, 5+) and date/time formats need explicit
tests. Do not translate protocol values, IDs, role names or user content.
Translations must fall back to English when a key is missing.

Verify: build with the translation bundle, switch languages without a
restart if the runtime supports it (otherwise document the restart), an Xvfb
screenshot in Russian at the narrowest supported window, and a check for
missing keys and untranslated visible strings.

## Suggested order and why

| Order | Item | Size | Why here |
| --- | --- | --- | --- |
| 1 | Onyx theme | S | Independent, immediately visible, forces the palette/contrast audit for later colour work. |
| 2 | Role colour everywhere + settings | M | Builds on the palette audit; shared refresh paths are easier to change before more name surfaces appear. |
| 3 | Copy usernames | S | Small UI plumbing, no protocol; settles how names are rendered before the remaining UI work. |
| 4 | Friend-request mini profile | S | Reuses the existing profile fetch/cache; gives a complete feature with little risk. |
| 5 | Search aliases, suggestions, jump | M | Core, service and UI test path in one area; the fake already models search. |
| 6 | Drag people between channels | M | The move action and permissions exist; remaining work is UI and confirmation. |
| 7 | Several streams | L | Needs measurement and media work; do it after the smaller voice changes. |
| 8 | Rich presence display/local IPC | L | Display is moderate; IPC is a new protocol and should stay optional and last. |
| 9 | Russian language | L | Touches every visible string and conflicts with all UI edits; land it once the text surface is stable. |

The order favours small, independently verifiable changes and defers the
largest text and media surfaces. If a release needs Russian first, split the
language item: enable the mechanism and extract strings early, translate and
switch the default late, keeping the extraction commits mechanical.
