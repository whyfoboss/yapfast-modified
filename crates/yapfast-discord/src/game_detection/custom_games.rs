//! The games this device names itself, which the desktop sets from its
//! settings. They're the device's, not an account's, so every session shares
//! them, and a change reaches the detection running.
use std::sync::OnceLock;
use tokio::sync::watch;

/// An executable, by file name or full path, and the game to show for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomGame {
    pub executable: String,
    pub name: String,
}

fn channel() -> &'static watch::Sender<Vec<CustomGame>> {
    static CHANNEL: OnceLock<watch::Sender<Vec<CustomGame>>> = OnceLock::new();
    CHANNEL.get_or_init(|| watch::channel(Vec::new()).0)
}

pub fn set(games: Vec<CustomGame>) {
    channel().send_if_modified(|old| {
        if *old == games {
            false
        } else {
            *old = games;
            true
        }
    });
}

pub(crate) fn subscribe() -> watch::Receiver<Vec<CustomGame>> {
    channel().subscribe()
}

fn disabled() -> &'static watch::Sender<Vec<String>> {
    static CHANNEL: OnceLock<watch::Sender<Vec<String>>> = OnceLock::new();
    CHANNEL.get_or_init(|| watch::channel(Vec::new()).0)
}

pub fn set_disabled(names: impl IntoIterator<Item = String>) {
    let mut names: Vec<_> = names.into_iter().map(|name| activity_key(&name)).collect();
    names.sort_unstable();
    names.dedup();
    disabled().send_if_modified(|old| {
        if *old == names {
            false
        } else {
            *old = names;
            true
        }
    });
}

pub(crate) fn subscribe_disabled() -> watch::Receiver<Vec<String>> {
    disabled().subscribe()
}

pub(crate) fn is_disabled(disabled: &[String], name: &str) -> bool {
    !disabled.is_empty() && disabled.contains(&activity_key(name))
}

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
