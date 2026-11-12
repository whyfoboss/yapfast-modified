//! Discord RPC over IPC (Unix domain sockets on Linux, Named Pipes on Windows).
//! Protocol: https://docs.discord.food/topics/rpc#ipc-transport.
//! Only local Rich Presence is exposed; account/voice control needs OAuth2
//! and is deliberately unsupported here.

pub(crate) mod proto;

#[cfg(windows)]
pub(crate) mod windows;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub(crate) use linux::start;

#[cfg(windows)]
pub(crate) use windows::start;
