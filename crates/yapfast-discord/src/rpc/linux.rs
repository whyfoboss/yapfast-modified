//! Linux Discord RPC over Unix domain socket.
//!
//! Protocol: https://docs.discord.food/topics/rpc#ipc-transport.

use std::{
    collections::HashSet,
    fs::{File, Metadata},
    io,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Result, bail};
use serde_json::Value;
use tokio::{
    net::{UnixListener, UnixStream},
    sync::{mpsc, watch},
    task::JoinSet,
};

use crate::{
    api::Api,
    rpc::proto::{self, MAX_CLIENTS},
    service::Snapshot,
};

pub(crate) fn start(
    api: Arc<Api>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
) -> watch::Receiver<Option<Arc<Value>>> {
    let (activity, receiver) = watch::channel(None);
    tokio::spawn(async move {
        // Tests must opt in to avoid advertising their fixture account on
        // the desktop. An explicit directory also helps Wine/sandbox bridges.
        let directory = std::env::var_os("YAPFAST_DISCORD_RPC_DIR").map(PathBuf::from);
        let disabled = std::env::var_os("YAPFAST_DISCORD_RPC").is_some_and(|v| v == "0")
            || (crate::client::test_server().is_some() && directory.is_none());
        if !disabled {
            match Socket::bind(&directory.unwrap_or_else(runtime_directory)).await {
                Ok(socket) => {
                    tracing::info!(path = %socket.path.display(), "Discord RPC listening");
                    serve(socket, api, snapshots, &activity).await;
                    activity.send_replace(None);
                    return;
                }
                Err(error) => tracing::warn!(%error, "Discord RPC unavailable"),
            }
        }
        // Keep the sender alive without work until the service drops its
        // receiver. No retry timer or extra thread when RPC is disabled.
        activity.closed().await;
    });
    receiver
}

fn runtime_directory() -> PathBuf {
    ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .into_iter()
        .filter_map(std::env::var_os)
        .find(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// The lock coordinates Yapfast processes; probing the socket also respects
/// Discord and other RPC servers, which do not take our lock.
struct Socket {
    listener: UnixListener,
    path: PathBuf,
    identity: Metadata,
    _lock: File,
}

impl Socket {
    async fn bind(directory: &Path) -> Result<Self> {
        let uid = std::fs::metadata("/proc/self")?.uid();
        for slot in 0..10 {
            let path = directory.join(format!("discord-ipc-{slot}"));
            let lock_path = directory.join(format!("yapfast-discord-ipc-{slot}.lock"));
            use std::os::unix::fs::OpenOptionsExt;
            let lock = match File::options()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&lock_path)
            {
                Ok(lock) => lock,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    // A replaced lock must neither follow a symlink nor
                    // block opening a FIFO.
                    let Ok(lock) = File::options()
                        .write(true)
                        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                        .open(&lock_path)
                    else {
                        continue;
                    };
                    let metadata = lock.metadata()?;
                    let link = std::fs::symlink_metadata(&lock_path)?;
                    if !link.is_file() || metadata.uid() != uid || !same_file(&metadata, &link) {
                        continue;
                    }
                    lock
                }
                Err(_) => continue,
            };
            if lock.try_lock().is_err() {
                continue;
            }
            if let Ok(metadata) = std::fs::symlink_metadata(&path) {
                if !metadata.file_type().is_socket() || metadata.uid() != uid {
                    continue;
                }
                // A live server with a full backlog must not block a Tokio
                // worker while we look for a free slot.
                match tokio::time::timeout(Duration::from_millis(100), UnixStream::connect(&path))
                    .await
                {
                    Ok(Err(error)) if error.kind() == io::ErrorKind::ConnectionRefused => {
                        if !std::fs::symlink_metadata(&path)
                            .is_ok_and(|now| same_file(&metadata, &now))
                        {
                            continue;
                        }
                        std::fs::remove_file(&path)?;
                    }
                    _ => continue,
                }
            }
            let Ok(listener) = UnixListener::bind(&path) else {
                continue;
            };
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            let identity = std::fs::symlink_metadata(&path)?;
            return Ok(Self {
                listener,
                path,
                identity,
                _lock: lock,
            });
        }
        bail!("no free Discord IPC socket in {}", directory.display())
    }
}

fn same_file(left: &Metadata, right: &Metadata) -> bool {
    left.ino() == right.ino() && left.dev() == right.dev()
}

impl Drop for Socket {
    fn drop(&mut self) {
        if std::fs::symlink_metadata(&self.path).is_ok_and(|now| same_file(&self.identity, &now)) {
            let _ = std::fs::remove_file(&self.path);
        }
        // Leave the lock inode in place: unlinking a locked file would let
        // another process create a second independent lock for the same slot.
    }
}

async fn serve(
    socket: Socket,
    api: Arc<Api>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
    activity: &watch::Sender<Option<Arc<Value>>>,
) {
    let (updates, mut incoming) = mpsc::channel(MAX_CLIENTS);
    let mut clients = JoinSet::new();
    let mut connected = HashSet::new();
    let mut next = 0_u64;
    let mut active = Vec::<(u64, Arc<Value>)>::new();
    let mut disabled = crate::game_detection::custom_games::subscribe_disabled();
    loop {
        let removed = tokio::select! {
            biased;
            () = activity.closed() => break,
            accepted = socket.listener.accept(), if clients.len() < MAX_CLIENTS => {
                let Ok((stream, _)) = accepted else {
                    // Out of descriptors, say: wait rather than end RPC for good.
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue;
                };
                if !stream.peer_cred().is_ok_and(|peer| peer.uid() == socket.identity.uid()) {
                    continue;
                }
                next += 1;
                let id = next;
                connected.insert(id);
                let api = api.clone();
                let snapshots = snapshots.clone();
                let updates = updates.clone();
                clients.spawn(async move {
                    if let Err(error) = proto::connection(stream, api, snapshots, updates, id).await {
                        tracing::debug!(%error, "Discord RPC client ended");
                    }
                    id
                });
                continue;
            }
            Some((id, value, artwork_only)) = incoming.recv() => {
                // A client may finish between polling this queue and polling
                // JoinSet. Its queued final update must not resurrect a game.
                if !connected.contains(&id) {
                    continue;
                }
                if artwork_only {
                    if let Some((_, shown)) = active.iter_mut().find(|(other, _)| *other == id)
                        && let Some(value) = value
                    {
                        *shown = value;
                    }
                } else {
                    active.retain(|(other, _)| *other != id);
                    if let Some(value) = value {
                        active.push((id, value));
                    }
                }
                None
            }
            Some(done) = clients.join_next(), if !clients.is_empty() => done.ok(),
            _ = disabled.changed() => None,
        };
        if let Some(id) = removed {
            connected.remove(&id);
            active.retain(|(other, _)| *other != id);
        }
        activity.send_if_modified(|shown| {
            let disabled = disabled.borrow_and_update();
            let next = active.iter().rev().find_map(|(_, value)| {
                let name = value["name"].as_str()?;
                (!crate::game_detection::custom_games::is_disabled(&disabled, name))
                    .then_some(value)
            });
            if shown.as_ref() == next {
                false
            } else {
                *shown = next.cloned();
                true
            }
        });
    }
    // Dropping JoinSet aborts clients, and Socket removes only our own inode.
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "yapfast-rpc-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn leaves_live_sockets_alone_and_reclaims_stale_ones() {
        let directory = Directory::new();
        let existing = UnixListener::bind(directory.0.join("discord-ipc-0")).unwrap();
        let first = Socket::bind(&directory.0).await.unwrap();
        assert!(first.path.ends_with("discord-ipc-1"));
        let second = Socket::bind(&directory.0).await.unwrap();
        assert!(second.path.ends_with("discord-ipc-2"));
        assert_eq!(
            std::fs::metadata(&first.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(first);
        assert!(!directory.0.join("discord-ipc-1").exists());
        drop(existing);
        let reclaimed = Socket::bind(&directory.0).await.unwrap();
        assert!(reclaimed.path.ends_with("discord-ipc-0"));
    }

    #[tokio::test]
    async fn does_not_remove_a_replacement_or_follow_a_lock_symlink() {
        let directory = Directory::new();
        let target = directory.0.join("keep");
        std::fs::write(&target, b"keep me").unwrap();
        std::os::unix::fs::symlink(&target, directory.0.join("yapfast-discord-ipc-0.lock"))
            .unwrap();
        let socket = Socket::bind(&directory.0).await.unwrap();
        assert!(socket.path.ends_with("discord-ipc-1"));
        std::fs::remove_file(&socket.path).unwrap();
        std::fs::write(&socket.path, b"replacement").unwrap();
        let path = socket.path.clone();
        drop(socket);
        assert_eq!(std::fs::read(path).unwrap(), b"replacement");
        assert_eq!(std::fs::read(target).unwrap(), b"keep me");
    }
}
