//! Windows Discord RPC Named Pipe server.
//!
//! Listens on `\\.\pipe\discord-ipc-0` through `\\.\pipe\discord-ipc-9`.
//! Safe against official Discord client: never hijacks, terminates, or interferes
//! with existing named pipes.

use std::{collections::HashSet, sync::Arc, time::Duration};

use anyhow::{Result, bail};
use serde_json::Value;
use tokio::{
    net::windows::named_pipe::{NamedPipeServer, ServerOptions},
    sync::{mpsc, watch},
    task::JoinSet,
};

use crate::{
    api::Api,
    rpc::proto::{self, MAX_CLIENTS},
    service::Snapshot,
};

/// Selects a free named pipe slot from 0 to 9 without interfering with
/// existing servers (such as the official Discord client).
fn select_pipe_slot(prefix: &str) -> Result<(String, NamedPipeServer)> {
    for slot in 0..10 {
        let pipe_name = format!(r"\\.\pipe\{prefix}-{slot}");
        // first_pipe_instance(true) ensures that if another process (like official Discord)
        // has already created this pipe, Windows rejects the call with an error.
        // We do not hijack or replace it; we gracefully proceed to the next slot.
        let server = match ServerOptions::new()
            .first_pipe_instance(true)
            .max_instances(MAX_CLIENTS)
            .create(&pipe_name)
        {
            Ok(s) => s,
            Err(_) => continue,
        };
        return Ok((pipe_name, server));
    }
    bail!("no free Discord IPC pipe in slots 0..9")
}

/// Runs the Windows Named Pipe server and manages connected clients.
async fn serve_windows(
    pipe_name: &str,
    initial_server: NamedPipeServer,
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
    let mut current_server = initial_server;

    loop {
        let removed = tokio::select! {
            biased;
            () = activity.closed() => break,
            connected_client = current_server.connect(), if clients.len() < MAX_CLIENTS => {
                if connected_client.is_ok() {
                    // Client connected. Create next pipe instance to accept subsequent clients.
                    let next_server = match ServerOptions::new().create(pipe_name) {
                        Ok(s) => s,
                        Err(error) => {
                            tracing::warn!(%error, "could not create next named pipe instance");
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            continue;
                        }
                    };
                    let stream = std::mem::replace(&mut current_server, next_server);
                    next += 1;
                    let id = next;
                    connected.insert(id);
                    let api = api.clone();
                    let snapshots = snapshots.clone();
                    let updates = updates.clone();
                    clients.spawn(async move {
                        if let Err(error) = proto::connection(stream, api, snapshots, updates, id).await {
                            tracing::debug!(%error, "Windows Discord RPC client ended");
                        }
                        id
                    });
                }
                None
            }
            Some((id, value, artwork_only)) = incoming.recv() => {
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
}

/// Spawns the Windows Named Pipe Discord RPC receiver.
pub(crate) fn start(
    api: Arc<Api>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
) -> watch::Receiver<Option<Arc<Value>>> {
    let (activity, receiver) = watch::channel(None);
    tokio::spawn(async move {
        let disabled = std::env::var_os("YAPFAST_DISCORD_RPC").is_some_and(|v| v == "0");
        let prefix = if crate::client::test_server().is_some() {
            std::env::var_os("YAPFAST_DISCORD_RPC_DIR").and_then(|path| {
                std::path::Path::new(&path)
                    .file_name()?
                    .to_str()
                    .map(str::to_owned)
            })
        } else {
            Some("discord-ipc".to_owned())
        };
        if !disabled && let Some(prefix) = prefix {
            match select_pipe_slot(&prefix) {
                Ok((pipe_name, server)) => {
                    tracing::info!("Windows Discord RPC listening on named pipe");
                    serve_windows(&pipe_name, server, api, snapshots, &activity).await;
                    activity.send_replace(None);
                    return;
                }
                Err(error) => tracing::warn!(%error, "Windows Discord RPC named pipe unavailable"),
            }
        }
        activity.closed().await;
    });
    receiver
}
