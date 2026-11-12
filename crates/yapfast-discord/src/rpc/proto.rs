//! Shared Discord RPC protocol framing, handshake validation, and command handling.
//!
//! Protocol: https://docs.discord.food/topics/rpc#ipc-transport.
//! Works across Unix domain sockets (Linux/macOS), Windows named pipes, and in-memory streams.

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, watch},
};

use crate::{
    api::Api,
    service::{Snapshot, Status},
};

pub const MAX_PACKET: usize = 64 * 1024;
/// The most image addresses a client's images are kept for: a music player
/// shows a new cover with each song.
pub const KEPT_IMAGES: usize = 32;
pub const MAX_CLIENTS: usize = 16;
pub const WAIT: Duration = Duration::from_secs(15);

/// Connection ID, its current activity, and whether the update only adds artwork.
pub(super) type Update = (u64, Option<Arc<Value>>, bool);

pub type Assets = HashMap<String, String>;

/// Reads an RPC frame: 4-byte little-endian opcode, 4-byte little-endian length,
/// followed by payload bytes. Rejects frames larger than `MAX_PACKET` before allocating.
pub async fn read<S: AsyncReadExt + Unpin>(stream: &mut S) -> Result<(u32, Vec<u8>)> {
    let opcode = stream.read_u32_le().await?;
    let size = stream.read_u32_le().await? as usize;
    ensure!(size <= MAX_PACKET, "RPC packet too large");
    let mut payload = vec![0; size];
    stream.read_exact(&mut payload).await?;
    Ok((opcode, payload))
}

/// Writes an RPC frame: opcode + payload length + payload.
pub async fn write<S: AsyncWriteExt + Unpin>(
    stream: &mut S,
    opcode: u32,
    payload: &[u8],
) -> Result<()> {
    tokio::time::timeout(WAIT, async {
        stream.write_u32_le(opcode).await?;
        stream.write_u32_le(payload.len() as u32).await?;
        stream.write_all(payload).await
    })
    .await??;
    Ok(())
}

/// Serializes a JSON value and sends it with opcode 1 (FRAME).
pub async fn send<S: AsyncWriteExt + Unpin>(stream: &mut S, value: Value) -> Result<()> {
    write(stream, 1, &serde_json::to_vec(&value)?).await
}

/// Validates Discord snowflake ID format (1-20 ASCII digits, non-zero).
pub fn snowflake(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && value.bytes().all(|b| b.is_ascii_digit())
        && value.parse::<u64>().is_ok_and(|id| id > 0)
}

pub fn web_address(text: &str, longest: usize) -> bool {
    text.len() <= longest && (text.starts_with("https://") || text.starts_with("http://"))
}

/// The media proxy's names for image addresses, as `mp:` asset keys.
pub fn external_lookup(response: &Value) -> Result<Assets> {
    Ok(response
        .as_array()
        .context("invalid external assets")?
        .iter()
        .filter_map(|asset| {
            Some((
                asset["url"].as_str()?.to_owned(),
                format!("mp:{}", asset["external_asset_path"].as_str()?),
            ))
        })
        .collect())
}

pub fn asset_lookup(response: &Value) -> Result<Assets> {
    Ok(response
        .as_array()
        .context("invalid application assets")?
        .iter()
        .filter_map(|asset| {
            Some((
                asset["name"].as_str()?.to_owned(),
                asset["id"].as_str()?.to_owned(),
            ))
        })
        .collect())
}

/// RPC uses seconds for timestamps and asset names; the Gateway needs
/// milliseconds and asset ids. Select fields so callers cannot inject a
/// custom status, another application id, or internal gateway metadata.
#[cfg(test)]
fn normalize(
    input: &Value,
    client: &str,
    name: &str,
    assets: Option<&Assets>,
    addresses: &Assets,
) -> Result<Value> {
    normalize_activity(input, client, name, assets, addresses, false)
}

fn normalize_activity(
    input: &Value,
    client: &str,
    name: &str,
    assets: Option<&Assets>,
    addresses: &Assets,
    allow_pending_images: bool,
) -> Result<Value> {
    ensure!(input.is_object(), "activity must be an object");
    let kind = match input.get("type") {
        None => 0,
        Some(value) => value.as_u64().context("invalid activity type")?,
    };
    ensure!(matches!(kind, 0 | 2 | 3 | 5), "unsupported activity type");
    let mut output = json!({"name":name,"type":kind,"application_id":client});
    for key in [
        "state",
        "details",
        "state_url",
        "details_url",
        "party",
        "secrets",
        "instance",
        "status_display_type",
    ] {
        if let Some(value) = input.get(key).filter(|v| !v.is_null()) {
            output[key] = value.clone();
        }
    }
    for key in ["state", "details"] {
        if let Some(value) = output.get(key) {
            ensure!(
                value.as_str().is_some_and(|s| s.chars().count() <= 128),
                "invalid {key}"
            );
        }
    }
    for key in ["state_url", "details_url"] {
        if let Some(value) = output.get(key) {
            ensure!(
                value
                    .as_str()
                    .is_some_and(|s| s.len() <= 256
                        && (s.starts_with("https://") || s.starts_with("http://"))),
                "invalid {key}"
            );
        }
    }
    if let Some(value) = output.get("instance") {
        ensure!(value.is_boolean(), "instance must be a boolean");
    }
    if let Some(value) = output.get("status_display_type") {
        ensure!(
            value.as_u64().is_some_and(|n| n <= 2),
            "invalid status_display_type"
        );
    }
    if let Some(party) = output.get("party") {
        ensure!(party.is_object(), "party must be an object");
        if let Some(id) = party.get("id") {
            ensure!(
                id.as_str().is_some_and(|s| s.chars().count() <= 128),
                "invalid party id"
            );
        }
        if let Some(size) = party.get("size") {
            ensure!(
                size.as_array().is_some_and(|s| s.len() == 2
                    && s[0]
                        .as_u64()
                        .zip(s[1].as_u64())
                        .is_some_and(|(current, max)| max > 0 && current <= max)),
                "invalid party size"
            );
        }
    }
    if let Some(secrets) = output.get("secrets") {
        ensure!(secrets.is_object(), "secrets must be an object");
        for key in ["join", "spectate", "match"] {
            if let Some(value) = secrets.get(key) {
                ensure!(
                    value.as_str().is_some_and(|s| s.chars().count() <= 128),
                    "invalid {key} secret"
                );
            }
        }
    }
    if let Some(timestamps) = input.get("timestamps").filter(|v| !v.is_null()) {
        ensure!(timestamps.is_object(), "invalid timestamps");
        let mut converted = json!({});
        for key in ["start", "end"] {
            if let Some(value) = timestamps.get(key) {
                let seconds = value
                    .as_u64()
                    .context("timestamp must be a positive integer")?;
                // Some modern SDKs already send milliseconds.
                converted[key] = json!(if seconds < 1_000_000_000_000 {
                    seconds.checked_mul(1000).context("timestamp overflow")?
                } else {
                    seconds
                });
            }
        }
        output["timestamps"] = converted;
    }
    if let Some(images) = input.get("assets").filter(|v| !v.is_null()) {
        ensure!(images.is_object(), "invalid assets");
        let mut converted = json!({});
        for key in ["large_image", "small_image", "large_text", "small_text"] {
            if let Some(value) = images.get(key).filter(|v| !v.is_null()) {
                let text = value.as_str().context("asset must be text")?;
                let text = if key.ends_with("_image") && web_address(text, usize::MAX) {
                    ensure!(web_address(text, 512), "invalid external image address");
                    match addresses.get(text) {
                        Some(image) => image.as_str(),
                        None if allow_pending_images => continue,
                        None => bail!("Discord didn't take the image's address"),
                    }
                } else if key.ends_with("_image") {
                    match assets.and_then(|lookup| lookup.get(text)) {
                        Some(image) => image.as_str(),
                        None if snowflake(text) => text,
                        None if allow_pending_images => continue,
                        None => bail!("unknown application asset"),
                    }
                } else {
                    text
                };
                converted[key] = json!(text);
            }
        }
        output["assets"] = converted;
    }
    if let Some(buttons) = input.get("buttons").filter(|v| !v.is_null()) {
        let buttons = buttons.as_array().context("buttons must be an array")?;
        ensure!(buttons.len() <= 2, "at most two buttons");
        let mut labels = Vec::new();
        let mut urls = Vec::new();
        for button in buttons {
            let label = button["label"].as_str().context("button needs a label")?;
            let url = button["url"].as_str().context("button needs a URL")?;
            ensure!(
                !label.is_empty() && label.chars().count() <= 32,
                "invalid button label"
            );
            ensure!(
                url.len() <= 512 && (url.starts_with("https://") || url.starts_with("http://")),
                "invalid button URL"
            );
            labels.push(label);
            urls.push(url);
        }
        output["buttons"] = json!(labels);
        output["metadata"] = json!({"button_urls":urls});
    }
    // Leave room for a custom status and the rest of the Gateway envelope.
    ensure!(
        serde_json::to_vec(&output)?.len() <= 3072,
        "activity is too large"
    );
    Ok(output)
}

/// Generic connection handler for any async byte stream (Unix domain socket, Windows named pipe, or duplex).
pub async fn connection<S: AsyncReadExt + AsyncWriteExt + Unpin>(
    mut stream: S,
    api: Arc<Api>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
    updates: mpsc::Sender<Update>,
    id: u64,
) -> Result<()> {
    let (opcode, payload) = tokio::time::timeout(WAIT, read(&mut stream)).await??;
    let handshake: Value = serde_json::from_slice(&payload)?;
    let client = handshake["client_id"].as_str().unwrap_or_default();
    if opcode != 0 || handshake["v"] != 1 || !snowflake(client) {
        write(
            &mut stream,
            2,
            br#"{"code":4000,"message":"Invalid RPC handshake"}"#,
        )
        .await?;
        return Ok(());
    }
    let client = client.to_owned();
    // A connected, idle game must not retain its handshake JSON/packet buffer.
    drop(handshake);
    drop(payload);

    // A game that retries while Discord is offline must not cost an API call each time.
    ensure!(
        snapshots.borrow().status == Status::Online,
        "Discord is not connected"
    );
    let lookup = Instant::now();
    tracing::debug!(target: "yapfast_media", "RPC handshake received");
    let application = tokio::time::timeout(WAIT, api.rpc_application(&client)).await??;
    tracing::debug!(target: "yapfast_media", elapsed_ms = lookup.elapsed().as_millis() as u64, "RPC application ready");
    let name = application["name"]
        .as_str()
        .context("RPC application has no name")?
        .to_owned();
    drop(application);

    let user = {
        let snapshot = snapshots.borrow();
        ensure!(
            snapshot.status == Status::Online,
            "Discord is not connected"
        );
        json!({
            "id": snapshot.user_id,
            "username": snapshot.account.as_ref().map(|a| a.name.as_str()),
            "discriminator": "0",
            "avatar": null,
            "bot": false
        })
    };
    send(
        &mut stream,
        json!({
            "cmd": "DISPATCH",
            "evt": "READY",
            "nonce": null,
            "data": {
                "v": 1,
                "config": {
                    "cdn_host": "cdn.discordapp.com",
                    "api_endpoint": "//discord.com/api",
                    "environment": "production"
                },
                "user": user
            }
        }),
    )
    .await?;

    let mut assets = None;
    let mut images = Assets::new();
    type Artwork = Pin<Box<dyn Future<Output = (Value, Option<Assets>, Option<Assets>)> + Send>>;
    let mut artwork: Option<Artwork> = None;
    loop {
        let (opcode, payload) = {
            let frame = read(&mut stream);
            tokio::pin!(frame);
            loop {
                tokio::select! {
                    biased;
                    result = &mut frame => break result?,
                    (input, named, external) = async { artwork.as_mut().unwrap().await }, if artwork.is_some() => {
                        artwork = None;
                        if let Some(named) = named {
                            assets = Some(named);
                        }
                        if let Some(external) = external {
                            if images.len() + external.len() > KEPT_IMAGES {
                                images.clear();
                            }
                            images.extend(external);
                        }
                        if let Ok(value) = normalize_activity(&input, &client, &name, assets.as_ref(), &images, true) {
                            updates.send((id, Some(Arc::new(value)), true)).await?;
                        }
                    }
                }
            }
        };
        match opcode {
            2 => return Ok(()),
            3 => {
                write(&mut stream, 4, &payload).await?;
                continue;
            }
            4 => continue,
            1 => {}
            _ => bail!("invalid RPC opcode"),
        }
        let packet: Value = serde_json::from_slice(&payload)?;
        drop(payload);
        let cmd = packet["cmd"].as_str().unwrap_or_default();
        let nonce = &packet["nonce"];
        if cmd != "SET_ACTIVITY" {
            send(
                &mut stream,
                json!({
                    "cmd": cmd,
                    "nonce": nonce,
                    "evt": "ERROR",
                    "data": {"code": 4002, "message": "Only SET_ACTIVITY is supported"}
                }),
            )
            .await?;
            continue;
        }
        let received = Instant::now();
        let result = (|| {
            let args = &packet["args"];
            ensure!(
                args["pid"].as_u64().is_some_and(|pid| pid > 0),
                "pid must be a positive integer"
            );
            let input = args
                .get("activity")
                .context("activity is required (null clears it)")?;
            if input.is_null() {
                return Ok(None);
            }
            normalize_activity(input, &client, &name, assets.as_ref(), &images, true)
                .map(|value| Some(Arc::new(value)))
        })();

        match result {
            Ok(value) => {
                // Any pending result belongs to the previous activity.
                artwork = None;
                let eligible = value.as_deref().is_some_and(crate::own_activity::eligible);
                if eligible {
                    let input = &packet["args"]["activity"];
                    let pictures = &input["assets"];
                    let mut addresses = Vec::new();
                    let mut fetch_named = false;
                    for key in ["large_image", "small_image"] {
                        if let Some(image) = pictures[key].as_str() {
                            if web_address(image, 512) {
                                if !images.contains_key(image) {
                                    addresses.push(image.to_owned());
                                }
                            } else if !snowflake(image) && assets.is_none() {
                                fetch_named = true;
                            }
                        }
                    }
                    if fetch_named || !addresses.is_empty() {
                        let input = input.clone();
                        let api = api.clone();
                        let client = client.clone();
                        artwork = Some(Box::pin(async move {
                            let named = async {
                                if !fetch_named {
                                    return None;
                                }
                                let response = tokio::time::timeout(WAIT, api.rpc_assets(&client))
                                    .await
                                    .ok()?
                                    .ok()?;
                                asset_lookup(&response).ok()
                            };
                            let external = async {
                                if addresses.is_empty() {
                                    return None;
                                }
                                let addresses: Vec<_> =
                                    addresses.iter().map(String::as_str).collect();
                                let response = tokio::time::timeout(
                                    WAIT,
                                    api.rpc_external_assets(&client, &addresses),
                                )
                                .await
                                .ok()?
                                .ok()?;
                                external_lookup(&response).ok()
                            };
                            let (named, external) = tokio::join!(named, external);
                            (input, named, external)
                        }));
                    }
                }
                // Bound queued updates even when games publish very frequently.
                updates
                    .send((id, value.clone().filter(|_| eligible), false))
                    .await?;
                let published_at_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                tracing::debug!(target: "yapfast_media", source = "rpc", active = value.is_some(),
                    artwork_pending = artwork.is_some(), elapsed_ms = received.elapsed().as_millis() as u64,
                    published_at_ms, "RPC state published");
                send(
                    &mut stream,
                    json!({"cmd": cmd, "nonce": nonce, "evt": null, "data": value.as_deref()}),
                )
                .await?;
            }
            Err(error) => {
                send(
                    &mut stream,
                    json!({
                        "cmd": cmd,
                        "nonce": nonce,
                        "evt": "ERROR",
                        "data": {"code": 4000, "message": error.to_string()}
                    }),
                )
                .await?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snowflake_validation() {
        assert!(snowflake("123456789012345678"));
        assert!(snowflake("42"));
        assert!(!snowflake(""));
        assert!(!snowflake("0"));
        assert!(!snowflake("abc"));
        assert!(!snowflake("123456789012345678901")); // > 20 digits
    }

    #[test]
    fn converts_sdk_fields_and_uses_the_handshakes_application() {
        let assets = asset_lookup(&json!([{"name":"cover","id":"1234"}])).unwrap();
        let shown = normalize(
            &json!({"name":"Spoof","application_id":"999","state":"In game",
            "timestamps":{"start":1700000000_u64,"end":1700000123000_u64},
            "assets":{"large_image":"cover","small_image":"5678","large_text":"Hover"},
            "buttons":[{"label":"Website","url":"https://example.com"}],
            "metadata":{"injected":true},"id":"spoof"}),
            "42",
            "Game",
            Some(&assets),
            &Assets::new(),
        )
        .unwrap();
        assert_eq!(shown["type"], 0);
        assert_eq!(shown["name"], "Game");
        assert_eq!(shown["application_id"], "42");
        assert_eq!(
            shown["timestamps"],
            json!({"start":1700000000000_u64,"end":1700000123000_u64})
        );
        assert_eq!(shown["assets"]["large_image"], "1234");
        assert_eq!(shown["assets"]["small_image"], "5678");
        assert_eq!(shown["buttons"], json!(["Website"]));
        assert_eq!(
            shown["metadata"],
            json!({"button_urls":["https://example.com"]})
        );
        assert!(shown.get("id").is_none());
    }

    #[test]
    fn shows_images_given_by_address_through_the_media_proxy() {
        let images = external_lookup(&json!([{"url":"https://example.com/cover.png",
            "external_asset_path":"external/abc/https/example.com/cover.png"}]))
        .unwrap();
        let activity = json!({"assets":{"large_image":"https://example.com/cover.png"}});
        let shown = normalize(&activity, "42", "Game", None, &images).unwrap();
        assert_eq!(
            shown["assets"]["large_image"],
            "mp:external/abc/https/example.com/cover.png"
        );
        let other = json!({"assets":{"large_image":"https://example.com/other.png"}});
        assert!(normalize(&other, "42", "Game", None, &images).is_err());
    }

    #[test]
    fn rejects_invalid_activity_without_losing_the_previous_one() {
        for value in [
            json!([]),
            json!({"type":4}),
            json!({"type":"0"}),
            json!({"state":42}),
            json!({"details":"a".repeat(129)}),
            json!({"timestamps":{"start":-1}}),
            json!({"assets":{"large_image":"unknown"}}),
            json!({"buttons":[{"label":"Click","url":"javascript:alert(1)"}]}),
        ] {
            assert!(
                normalize(&value, "42", "Game", None, &Assets::new()).is_err(),
                "{value}"
            );
        }
        for kind in [0, 2, 3, 5] {
            assert!(normalize(&json!({"type":kind}), "42", "Game", None, &Assets::new()).is_ok());
        }
    }

    #[tokio::test]
    async fn framing_partial_reads_and_length_limits() {
        let (mut client, mut server) = tokio::io::duplex(1024);

        let writer = tokio::spawn(async move {
            // Write 8-byte header and payload byte-by-byte
            for byte in [1, 0, 0, 0, 2, 0, 0, 0, b'{', b'}'] {
                client.write_all(&[byte]).await.unwrap();
                tokio::task::yield_now().await;
            }
            // Send oversized packet header
            client.write_u32_le(1).await.unwrap();
            client.write_u32_le(MAX_PACKET as u32 + 1).await.unwrap();
        });

        let (op, payload) = read(&mut server).await.unwrap();
        assert_eq!(op, 1);
        assert_eq!(payload, b"{}");

        let err = read(&mut server).await.unwrap_err();
        assert!(err.to_string().contains("too large"));
        writer.await.unwrap();
    }

    #[tokio::test]
    async fn ping_pong_opcodes() {
        let (mut client, mut server) = tokio::io::duplex(1024);

        let handle = tokio::spawn(async move {
            write(&mut client, 3, b"ping_data").await.unwrap();
            let (op, payload) = read(&mut client).await.unwrap();
            assert_eq!(op, 4); // Pong
            assert_eq!(payload, b"ping_data");
        });

        let (op, payload) = read(&mut server).await.unwrap();
        assert_eq!(op, 3);
        write(&mut server, 4, &payload).await.unwrap();
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn handshake_rejection_on_invalid_credentials() {
        let (mut client, server) = tokio::io::duplex(1024);
        let (tx, _rx) = mpsc::channel(1);
        let api = Arc::new(Api::anonymous().unwrap());
        let (_, snapshots) = watch::channel(Arc::new(Snapshot::default()));

        let conn_task =
            tokio::spawn(async move { connection(server, api, snapshots, tx, 1).await });

        // Client sends handshake with non-snowflake client_id
        let bad_handshake = json!({"v": 1, "client_id": "not-a-snowflake"});
        write(&mut client, 0, &serde_json::to_vec(&bad_handshake).unwrap())
            .await
            .unwrap();

        // Server should respond with opcode 2 (CLOSE) and error JSON
        let (op, payload) = read(&mut client).await.unwrap();
        assert_eq!(op, 2);
        let err: Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(err["code"], 4000);
        assert_eq!(err["message"], "Invalid RPC handshake");

        let _ = conn_task.await;
    }
}
