# Themes

Settings → Appearance shows the window in one of the themes Yapfast comes with,
or one of the person's own. Each is a full set of the colours of the Slint
global `Theme` (`ui/theme.slint`), plus whether it's dark, which sets the
std-widgets' colour scheme and the scrollbars'. `src/ui/theme.rs` holds the built-in ones as
`BUILT_IN`; `ui/theme.slint` says what each colour paints. The choice
is kept in `layout.json` as `theme`, the theme's id, and applied before the
window's first frame, and again to a window made anew. An id Yapfast doesn't
know, or a deleted theme's, shows Yap.

On Windows, the selected theme's accent and foreground also colour the initial
window and taskbar icon (resolved from `layout.json` before window creation and
reapplied after reopening from tray), the tray icon, and Windows Toast
notifications (`notifications/icon.png`). Existing Yapfast shortcuts that point
to the running executable are updated with a multi-size `.ico` (32, 64 and
256 px) when changed, avoiding redundant shortcut refreshes via `applied.txt`.
Windows may keep a cached shortcut icon temporarily. The executable's own icon
stays unchanged.

## Custom themes

New theme opens an editor on a copy of the theme shown; START FROM swaps it
for a copy of any other. The editor lists every colour by its name in
`ui/theme.slint`, with a swatch and its hex, which takes `#rgb`, `#rrggbb` or
`#rrggbbaa` and marks anything else. Each change shows at once, so the whole
window is the preview. Save keeps the theme and chooses it; Cancel, leaving
Appearance or closing settings shows the theme from before. A saved theme's
card has a pencil that opens it again to rename, change or delete it;
deleting the chosen one shows Yap. A window rebuilt while the editor is open,
as closing to the [tray](tray.md) does, drops the unsaved edit.

`layout.json` keeps them in `custom_themes`, in the order Appearance shows
them after the built-in ones:

```json
{
  "id": "custom:1",
  "name": "Plum",
  "base": "yap",
  "dark": true,
  "colours": { "rail": "#111215", "chat": "#2a1a3a", "row-hover": "#1a1b20" }
}
```

`base` is the built-in theme the first copy came from (a copy of a custom
theme takes its base). Yapfast writes every colour, but a colour that's missing,
such as one added in a later version, or that isn't a colour, takes the
base's value, and a name Yapfast doesn't know is ignored, so themes survive new
colours. Ids are `custom:` and a number, and no built-in id has a colon, so
they can't collide; on reading, Yapfast gives a fresh one to any custom theme
whose id isn't of that form or repeats another's. The editor's rows are a
Slint model only while it's open; otherwise a custom theme costs its parsed
colours and a card.

| Theme | Id | Dark | Accent | From |
| --- | --- | --- | --- | --- |
| Yap | `yap` | yes | marigold | Yapfast's own, the default |
| Onyx | `onyx` | yes | white | Yapfast's own, black and white |
| Bonfire | `bonfire` | yes | blurple | Discord's Dark theme |
| Catppuccin Mocha | `catppuccin-mocha` | yes | mauve | [Catppuccin](https://github.com/catppuccin/catppuccin) |
| Dracula | `dracula` | yes | purple | [Dracula](https://draculatheme.com/contribute) |
| Nord | `nord` | yes | frost nord8 | [Nord](https://www.nordtheme.com/docs/colors-and-palettes) |
| Gruvbox Dark | `gruvbox-dark` | yes | orange | [Gruvbox](https://github.com/morhetz/gruvbox) |
| Tokyo Night | `tokyo-night` | yes | blue | [Tokyo Night](https://github.com/tokyo-night/tokyo-night-vscode-theme), Night style |
| Catppuccin Latte | `catppuccin-latte` | no | mauve | [Catppuccin](https://github.com/catppuccin/catppuccin) |
| Rosé Pine Dawn | `rose-pine-dawn` | no | rose | [Rosé Pine](https://rosepinetheme.com/palette/) |
| Sakura | `sakura` | no | blossom pink | Yapfast's own |

Onyx is Yapfast's own black-and-white theme: pure black grounds, raised
surfaces at `#0a0a0a`, `#111111` and `#1a1a1a`, `#2a2a2a` lines, white text
and a white accent. Colour is kept only where it carries meaning: presence
dots, danger, live voice, mention and unread badges, and the role colours
servers set. Mention rows and links are monochrome, and scrims and shadows
over media stay literal, as in every theme.

Its contrast, from `onyx_text_is_readable_on_every_surface` (WCAG 2.1):

| Colour | rail | sidebar | panel | chat | raised | field | hover | active | row-hover | menu | composer |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| text | 21.00 | 19.80 | 19.80 | 21.00 | 18.88 | 19.80 | 17.40 | 17.40 | 19.80 | 18.10 | 19.56 |
| secondary | 8.03 | 7.57 | 7.57 | 8.03 | 7.22 | 7.57 | 6.66 | 6.66 | 7.57 | 6.92 | 7.48 |
| muted | 6.08 | 5.73 | 5.73 | 6.08 | 5.47 | 5.73 | 5.04 | 5.04 | 5.73 | 5.24 | 5.67 |
| faint | 3.66 | 3.45 | 3.45 | 3.66 | 3.29 | 3.45 | 3.03 | 3.03 | 3.45 | 3.15 | 3.41 |
| link | 13.48 | 12.71 | 12.71 | 13.48 | 12.12 | 12.71 | 11.17 | 11.17 | 12.71 | 11.62 | 12.56 |
| danger | 5.61 | 5.29 | 5.29 | 5.61 | 5.04 | 5.29 | 4.65 | 4.65 | 5.29 | 4.83 | 5.22 |
| success | 6.60 | 6.22 | 6.22 | 6.60 | 5.94 | 6.22 | 5.47 | 5.47 | 6.22 | 5.69 | 6.15 |
| focus ring (accent) | 21.00 | 19.80 | 19.80 | 21.00 | 18.88 | 19.80 | 17.40 | 17.40 | 19.80 | 18.10 | 19.56 |
| mention (text on row) | 18.73 | 17.22 | 17.22 | 18.73 | 15.91 | 17.22 | 14.35 | 14.35 | 17.22 | 15.13 | 16.86 |

`faint` is de-emphasised text and icons, held to at least 2.5:1 like the
other themes; the focus ring is held to 3:1. The filled states are not
surface-dependent:

| State | Against | Ratio |
| --- | --- | --- |
| on-accent | accent | 21.00 |
| on-danger (also a mention badge) | danger | 5.61 |
| on-danger | danger-hover | 6.96 |

The palettes of Catppuccin, Dracula, Nord, Gruvbox, Tokyo Night and Rosé Pine
are published under the MIT licence; thanks to their authors. For Tokyo
Night, the steps between its grounds and text (bg_highlight, fg_gutter, dark3,
dark5) are named as folke's tokyonight.nvim names them. Yapfast takes their
colours, not their names for UI parts, so each mapping is Yapfast's:

- The ground steps go rail, sidebar, chat from the deepest to the one text
  sits on, with raised surfaces (popovers, reactions, stage tiles) and the
  composer one step off the chat. Where a palette has no shade below its
  background (Nord's nord0, Tokyo Night's bg_dark), the rail, panel and edge
  darken it, and hover and some row grounds are halfway mixes of two palette
  colours.
- Text, secondary, muted and faint take the palette's text ramp; faint keeps
  at least 2.5:1 against the sidebar, near Yap's 2.8:1.
- Content on the accent and on red (on-accent, on-danger) takes the palette's
  darkest or lightest colour, whichever reads; on pastel accents, as in
  Catppuccin and Tokyo Night, that's dark.
- Light themes wash hovers with a dark overlay (black, or the palette's text
  colour) instead of white, sweep skeletons with a white band, and draw the
  tooltip light, since its text is the theme's text colour.

Bonfire maps Discord's Dark theme from the CSS variables of its web client
(`.theme-dark` with `.theme-darker`, which is what its settings call Dark;
`.theme-dark` alone is Ash, `.theme-midnight` Onyx), read from the public
bundle: the rail and sidebar are `--background-base-lowest` (#121214), the chat
`--background-base-lower` (#1a1a1e), the account panel and composer
`--background-base-low` (#202024), raised surfaces and the tooltip
`--background-surface-high` (#242429), text `--text-default`, muted
`--text-muted`, links `--text-link`, the accent
`--control-primary-background-default` (blurple, #5865f2, hover #4452bb, white
on it), red `--control-critical-primary-background-default` (#d22d39), live
`--status-positive`, and hovers, selections, lines and secondary buttons the
`--interactive-background-*`, `--border-subtle` and
`--control-secondary-background-*` washes laid over their grounds. It carries
no Discord name or logo.

Discord role colours stay as servers set them, and some read faintly on a
light theme; Yapfast doesn't adjust them yet.
