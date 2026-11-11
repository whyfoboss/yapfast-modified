# YAPFAST — Native Windows 11 & Codex Migration Handoff

**Authoritative Migration Guide & Technical Specification**  
*Date: 2026-11-11*  
*Source Host: WSL2 (Ubuntu 24.04 / Nix)*  
*Destination Host: Native Windows 11 (`C:\Dev\yapfast`)*  
*Target AI Pair-Programmer: OpenAI Codex (ChatGPT Desktop App)*  

---

## 1. Executive Summary & Purpose of Migration

This document serves as the permanent, authoritative technical handoff for migrating the development of **Yapfast** from WSL2/Nix to a 100% native Windows 11 environment powered by **Codex** in the ChatGPT desktop app.

Yapfast is a high-performance, native, GPU-rendered Discord and Matrix desktop client written in Rust with its user interface in Slint. Memory efficiency, instant responsiveness, native platform integration, and strict account safety are foundational product goals.

All initial fork development milestones (**TASKS 1 through 9**) have been implemented, verified, regression-tested on the Fake Discord test suite, and manually verified on native Windows 11.

---

## 2. Completed Tasks & Architectural State (TASKS 1–9)

All feature development was conducted on fork branches and fast-forwarded into `main`:

| Task | Area | Description & Key Components |
|---|---|---|
| **TASK 1** | Themes & Visuals | Onyx monochrome built-in theme, high-contrast palette auditing in `ui/theme.slint` and `src/ui/theme.rs`. |
| **TASK 2** | Roles & Colors | Server role color cascading across all message lists, user popovers, and channel member views. |
| **TASK 3** | User Interaction | Selectable profile text, one-click copy handle/username actions in mini profiles. |
| **TASK 4** | Relationship UI | Mini profile card for incoming friend requests with accept/ignore actions. |
| **TASK 5** | Search & Discovery | Discord-style search filter dropdown (`from:`, `has:`, `in:`, `before:`, `after:`), Russian token support, and query highlighting. |
| **TASK 6** | Voice & Media | Drag-and-drop voice participant relocation across voice channels with permission auditing. |
| **TASK 7** | Multi-Stream Video | Multi-stream focus grid, dynamic 1..N video layout, eliminated layout flicker during stream switching, stability under high packet loss. |
| **TASK 8** | Settings & Plugins | Dedicated Plugins settings section, server rail "Mark All as Read" action, layout state migrations. |
| **TASK 9** | Game Activity & Rich Presence | Full native Game Activity engine: Windows WinRT SMTC media session integration, Tokio Windows Named Pipe Discord RPC receiver (`\\.\pipe\discord-ipc-0`..`9`), deterministic multi-activity priority/fallback, per-app controls, interactive UI preview, and own-profile Rich Presence card. |

---

## 3. TASK 9: Native Windows Technical Deep Dive

### 3.1. Windows SMTC Media Session Listener
- **Module:** `crates/yapfast-core/src/activity/windows_smtc.rs`
- **Technology:** Official Windows Runtime WinRT APIs via `windows = 0.62` (`Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager`).
- **Mechanism:**
  - Subscribes asynchronously to WinRT events:
    - `CurrentSessionChanged`
    - `SessionsChanged`
    - `MediaPropertiesChanged`
    - `PlaybackInfoChanged`
    - `TimelinePropertiesChanged`
  - Zero polling overhead: Windows OS thread pool pushes updates only when playback state, track metadata, timeline position, or active session changes.
  - Supports browsers (Chrome, Edge) playing SoundCloud / YouTube, and desktop players (Spotify, Windows Media Player, Apple Music).
  - Normalizes artist, title, album, duration, elapsed position, and playback status into `MediaTrack`.
- **UI Integration:**
  - Bound in `src/ui/games.rs` via `slint::spawn_local` directly on the Slint event loop, guaranteeing thread safety without thread hopping or `Send` violations.

### 3.2. Windows Tokio Named Pipe Discord RPC Receiver
- **Module:** `crates/yapfast-discord/src/rpc/windows.rs`
- **Protocol Parser:** `crates/yapfast-discord/src/rpc/proto.rs` (generic, platform-independent).
- **Technology:** Asynchronous Windows Named Pipes using `tokio::net::windows::named_pipe`.
- **Anti-Collision Architecture:**
  - Scans and binds pipe instances from `\\.\pipe\discord-ipc-0` up to `\\.\pipe\discord-ipc-9`.
  - Uses `ServerOptions::first_pipe_instance(true)` on `discord-ipc-0` to detect whether official Discord is currently running.
  - **Coexistence Guarantee:** If official Discord is running, Yapfast leaves `discord-ipc-0` untouched and safely binds `discord-ipc-1` (or next free slot). If official Discord is closed, Yapfast takes slot 0, allowing third-party RPC clients (SoundCloud RPC, VS Code Discord Presence, Cider) to seamlessly connect without configuration.
- **Protocol Support:**
  - Handshake (Opcode 0), Snowflake validation.
  - Frame framing (Opcode 1 / Frame), Opcode 2 (Close).
  - `SET_ACTIVITY` payload validation, length clamping, Discord media proxy asset resolution (`rpc_external_assets`).
- **Account Safety:**
  - Local-only RPC display: Yapfast displays the Rich Presence status in the local client and UI preview without executing unauthorized user-token self-bot Gateway Opcode 3 broadcasting.

### 3.3. Deterministic Priority & Multi-Activity Resolution
- **Module:** `crates/yapfast-core/src/activity.rs` (`ActivityManager`)
- **Resolution Strategy:**
  1. Primary User Pick: If an application is explicitly marked as "Primary" by the user and is currently running, it always takes precedence.
  2. Per-Application Enablement: Applications toggled OFF in Settings are completely suppressed.
  3. Priority Weighting: Process-detected games and Discord RPC games receive priority over background media playback.
  4. Automatic Fallback: When the primary application closes or an RPC client disconnects, Yapfast instantly and smoothly transfers focus to the next active candidate (e.g. falling back to SMTC music).
  5. UI Preview Test Scenarios: 10 deterministic test states in `ui/settings.slint` are gated on `root.preview` so that simulation controls are available only in preview/testing mode and completely hidden from normal production users.

---

## 4. Repository Structure & Key Files

```
yapfast/
├── Cargo.toml                          # Workspace root configuration (tokio, serde, slint)
├── Cargo.lock                          # Pinned dependency versions
├── build.rs                            # Windows rc.exe fallback icon compilation
├── src/
│   ├── main.rs                         # Application entrypoint, CLI, single-instance lock
│   ├── ui.rs                           # Main Slint adapter, state models, layout persistence
│   ├── ui/
│   │   ├── games.rs                    # Game Activity UI callbacks, SMTC listener hook
│   │   ├── profile.rs                  # User profile Rich Presence card rendering
│   │   ├── refresh.rs                  # Synchronizes core state to Slint widgets
│   │   └── tests.rs                    # Comprehensive UI unit tests
├── crates/
│   ├── yapfast-core/                   # Core business logic, audio, streaming, activity
│   │   ├── src/activity.rs             # Multi-source ActivityManager & priority resolution
│   │   └── src/activity/windows_smtc.rs# Native WinRT SMTC event listener
│   ├── yapfast-discord/                # Discord API, Gateway, voice, RPC receiver
│   │   ├── src/rpc.rs                  # Cross-platform RPC dispatcher
│   │   ├── src/rpc/proto.rs            # Generic Discord IPC protocol parsing & framing
│   │   └── src/rpc/windows.rs          # Tokio Windows Named Pipe server (slots 0..9)
│   └── yapfast-ui/                     # Slint build.rs compiler integration
├── ui/                                 # Slint declarative interface files
│   ├── yapfast.slint                   # Main root window and global callbacks
│   ├── settings.slint                  # Settings pages including GameActivitySettings
│   └── yapfast/settings-nav.slint      # Settings navigation sidebar
├── scripts/
│   ├── prepare-windows-skia.py         # Native Windows script: builds static /MT Skia
│   └── package-windows.py              # Windows packaging into portable ZIP and installer
└── docs/                               # Architecture and area specifications
```

---

## 5. Native Windows 11 Build Prerequisites

To develop and build Yapfast natively on Windows 11 without WSL2, the following tools must be installed:

### 5.1. Required Tools & Compilers
1. **Rust Toolchain:**
   - Rust 1.97.1+ (MSVC x64 target: `x86_64-pc-windows-msvc`).
   - Check with: `rustc --version`, `rustup target list | findstr installed`.
2. **Visual Studio 2022 / Build Tools:**
   - Workload: "Desktop development with C++" (MSVC v143 or later, Windows 10/11 SDK).
   - Provides `cl.exe`, `rc.exe`, and Windows SDK headers.
3. **LLVM / Clang:**
   - LLVM 21+ with `clang-cl.exe`, `llvm-rc.exe`, and `libclang.dll` on `PATH` (or installed via Scoop / official LLVM Windows installer).
   - Set environment variable: `LIBCLANG_PATH` pointing to `bin` containing `libclang.dll`.
4. **NASM:**
   - NASM 2.16+ on system `PATH` (required for FFmpeg and AWS-LC x86 assembly).
5. **Python 3.10+ & Ninja:**
   - Required for `scripts/prepare-windows-skia.py`.
   - Install Ninja via Scoop (`scoop install ninja`) or ensure Visual Studio's `ninja.exe` is on `PATH`.

---

## 6. Native Windows Development Workflow (Codex Guide)

Codex can execute all development workflows directly in Windows PowerShell inside `C:\Dev\yapfast`:

### 6.1. Environment Setup (One-time Skia Preparation)
Yapfast uses a custom-namespaced static `/MT` build of Skia to avoid symbol collisions with libwebrtc's bundled libjpeg-turbo.

In PowerShell:
```powershell
# 1. Ensure clang-cl and ninja are on PATH
clang-cl --version
ninja --version

# 2. Build or verify the static Skia archive (cached in .local\windows\skia\)
python scripts\prepare-windows-skia.py

# 3. Print or export the required environment variable
python scripts\prepare-windows-skia.py --print-env
# Example output: $env:SKIA_BINARIES_URL="file://C:/Dev/yapfast/.local/windows/skia/prepared/skia-binaries-....tar.gz"
```

### 6.2. Fast Incremental Development Build
During feature development and bug fixing, **always use the `fast` profile** with static CRT:
```powershell
# Build fast development executable
cargo build --profile fast --no-default-features --features desktop,discord

# The output executable is located at:
# .\target\fast\yapfast.exe
```

### 6.3. Running and Testing the GUI
```powershell
# Run in preview mode (all UI pages accessible without account login)
.\target\fast\yapfast.exe --preview

# Run standard client
.\target\fast\yapfast.exe

# View real-time diagnostic logs:
# %LOCALAPPDATA%\Yapfast\Yapfast\data\yapfast.log
```

### 6.4. Running Targeted Unit Tests
Avoid running the full workspace test suite blindly. Target the specific crate under development:
```powershell
# Test core activity manager & priority resolution
cargo test --profile fast -p yapfast-core activity

# Test discord RPC protocol parsing
cargo test --profile fast -p yapfast-discord rpc

# Test UI state transitions
cargo test --profile fast --bin yapfast ui::tests
```

### 6.5. Code Quality & Formatting
Before completing any task, ensure formatting and clippy pass without warnings:
```powershell
cargo fmt --all -- --check
cargo clippy --profile fast --all-targets --no-default-features --features desktop,discord -- -D warnings
```

---

## 7. Migration Execution: Moving from WSL2 to `C:\Dev\yapfast`

### Step 1: Create the destination folder
In PowerShell or WSL2:
```bash
mkdir -p /mnt/c/Dev/yapfast
```

### Step 2: Clone complete Git repository
To preserve 100% of Git history, commit graph, tags, and local branches while stripping all transient Linux build caches (`target/`):
```bash
git clone /home/whyfo/yapfast /mnt/c/Dev/yapfast
```

### Step 3: Fast-forward local `main`
In `C:\Dev\yapfast`:
```powershell
cd C:\Dev\yapfast
git checkout main
git merge --ff-only feat/game-activity-rich-presence
```

### Step 4: Copy prepared Windows Skia cache & export assets
To save Codex 15–20 minutes of recompiling Skia:
```bash
mkdir -p /mnt/c/Dev/yapfast/.local/windows/skia-static-production/prepared/
cp /home/whyfo/yapfast/.local/windows/skia-static-production/prepared/*.tar.gz /mnt/c/Dev/yapfast/.local/windows/skia-static-production/prepared/
cp -r /home/whyfo/yapfast/work /mnt/c/Dev/yapfast/work
```

---

## 8. Rules & Commitments for Codex

1. **One Heavy Compilation at a Time:**
   The Slint UI crate (`crates/yapfast-ui`) generates ~700,000 lines of Rust and requires 6–8 GiB of RAM. Never run multiple concurrent `cargo build` jobs.
2. **Account Safety First:**
   Do not introduce outbound user-token automation, unsupported Gateway Opcode 3 spoofing, or fake presence broadcasting.
3. **Layer Separation:**
   UI code does not perform network or filesystem I/O. Follow the established architecture:
   `ui/*.slint` $\rightarrow$ `src/ui/callbacks.rs` $\rightarrow$ `Action` $\rightarrow$ `Application::dispatch` $\rightarrow$ `Effect` $\rightarrow$ `service` $\rightarrow$ `Event` $\rightarrow$ `Application::apply` $\rightarrow$ `src/ui/refresh.rs`.
4. **Preserve Documentation & Comments:**
   Maintain docstrings, file references, and architecture guides.
5. **Git Discipline:**
   Do not push to remote repositories or rewrite published Git history. Keep commits logical, descriptive, and verified.
