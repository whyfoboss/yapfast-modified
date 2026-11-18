//! Shared Discord RPC protocol framing, handshake validation, and command handling.
//!
//! Protocol: https://docs.discord.food/topics/rpc#ipc-transport.
//! Works across Unix domain sockets (Linux/macOS), Windows named pipes, and in-memory streams.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
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
/// shows a new cover with each song, and replays should reuse them without
/// another round trip.
pub const KEPT_IMAGES: usize = 128;
pub const MAX_CLIENTS: usize = 16;
pub const WAIT: Duration = Duration::from_secs(15);
const EXTERNAL_TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProxyProbe {
    Available,
    Missing,
    Transient,
}

#[derive(Clone, Default)]
struct CachedClient {
    name: Option<String>,
    named: Option<Assets>,
    external: Assets,
    order: VecDeque<String>,
    verified: HashSet<String>,
    fallback_urls: HashMap<String, String>,
}

impl CachedClient {
    fn remember_external(&mut self, resolved: Assets) {
        for (url, key) in resolved {
            if let Some(pos) = self.order.iter().position(|existing| existing == &url) {
                self.order.remove(pos);
            }
            self.order.push_back(url.clone());
            self.external.insert(url, key);
            while self.order.len() > KEPT_IMAGES {
                if let Some(evicted) = self.order.pop_front() {
                    if let Some(old_key) = self.external.remove(&evicted) {
                        self.verified.remove(&old_key);
                    }
                }
            }
        }
    }

    fn touch_external(&mut self, url: &str) {
        let target = self.fallback_urls.get(url).map_or(url, String::as_str);
        if self.external.contains_key(target)
            && let Some(pos) = self.order.iter().position(|existing| existing == target)
        {
            let kept = self.order.remove(pos).unwrap();
            self.order.push_back(kept);
        }
    }

    fn effective_external(&self) -> Assets {
        let mut map = self.external.clone();
        for (rewritten, orig) in &self.fallback_urls {
            if let Some(mp_key) = self.external.get(orig) {
                map.insert(rewritten.clone(), mp_key.clone());
            }
        }
        map
    }

    fn record_fallback(&mut self, broken_url: &str, orig_url: String) {
        if let Some(old_key) = self.external.remove(broken_url) {
            self.verified.remove(&old_key);
        }
        self.order.retain(|item| item != broken_url);
        if self.fallback_urls.len() >= KEPT_IMAGES
            && !self.fallback_urls.contains_key(broken_url)
            && let Some(first) = self.fallback_urls.keys().next().cloned()
        {
            self.fallback_urls.remove(&first);
        }
        self.fallback_urls.insert(broken_url.to_owned(), orig_url);
    }

    fn invalidate_mp_key(&mut self, mp_key: &str) {
        self.verified.remove(mp_key);
        let doomed: Vec<String> = self
            .external
            .iter()
            .filter_map(|(url, key)| (key == mp_key).then(|| url.clone()))
            .collect();
        for url in doomed {
            self.external.remove(&url);
            self.order.retain(|item| item != &url);
            if let Some(orig) = soundcloud_original_thumbnail(&url) {
                self.record_fallback(&url, orig);
            }
        }
        self.fallback_urls
            .retain(|_, orig| self.external.contains_key(orig) || !orig.is_empty());
    }
}

/// Derives the producer's original badge URL (`-t50x50.`) when `soundcloud-rs`
/// upgraded a SoundCloud CDN thumbnail to `-t500x500.`.
pub(crate) fn soundcloud_original_thumbnail(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if parsed.scheme() != "https" {
        return None;
    }
    let host = parsed.host_str()?;
    if !host.eq_ignore_ascii_case("sndcdn.com") && !host.to_ascii_lowercase().ends_with(".sndcdn.com")
    {
        return None;
    }
    url.contains("500x500.")
        .then(|| url.replacen("500x500.", "50x50.", 1))
}

fn rpc_cache() -> &'static Mutex<HashMap<(Option<&'static str>, String), CachedClient>> {
    static CACHE: OnceLock<Mutex<HashMap<(Option<&'static str>, String), CachedClient>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn load_cached_client(client: &str) -> CachedClient {
    let key = (crate::client::test_server(), client.to_owned());
    rpc_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&key)
        .cloned()
        .unwrap_or_default()
}

fn update_cached_client(client: &str, update: impl FnOnce(&mut CachedClient)) {
    let key = (crate::client::test_server(), client.to_owned());
    let mut guard = rpc_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    update(guard.entry(key).or_default());
}

/// Evicts any cached `mp:external/...` asset matching a permanently failed
/// Discord media-proxy URL so invalid artwork is never served again on replay/reconnect.
pub(crate) fn invalidate_external_asset(proxy_url: &str) {
    let base = proxy_url.split('?').next().unwrap_or(proxy_url);
    let mp_key = if let Some(rest) = base.strip_prefix(crate::ready::media()) {
        format!("mp:{}", rest.trim_start_matches('/'))
    } else if let Some((_, rest)) = base.split_once("/external/") {
        format!("mp:external/{rest}")
    } else {
        return;
    };
    let server = crate::client::test_server();
    let mut guard = rpc_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for ((entry_server, _), cached) in guard.iter_mut() {
        if *entry_server == server {
            cached.invalidate_mp_key(&mp_key);
        }
    }
}

fn late_soundcloud_channel() -> &'static watch::Sender<Option<Arc<Value>>> {
    static LATE: OnceLock<watch::Sender<Option<Arc<Value>>>> = OnceLock::new();
    LATE.get_or_init(|| watch::channel(None).0)
}

pub(crate) fn subscribe_late_soundcloud_artwork() -> watch::Receiver<Option<Arc<Value>>> {
    late_soundcloud_channel().subscribe()
}

fn soundcloud_client_channels() -> &'static Mutex<HashMap<Option<&'static str>, watch::Sender<usize>>>
{
    static CHANNELS: OnceLock<Mutex<HashMap<Option<&'static str>, watch::Sender<usize>>>> =
        OnceLock::new();
    CHANNELS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn soundcloud_client_sender(server: Option<&'static str>) -> watch::Sender<usize> {
    let mut guard = soundcloud_client_channels()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard
        .entry(server)
        .or_insert_with(|| watch::channel(0).0)
        .clone()
}

pub(crate) fn subscribe_soundcloud_rpc_clients() -> watch::Receiver<usize> {
    soundcloud_client_sender(crate::client::test_server()).subscribe()
}

struct SoundCloudClientGuard {
    server: Option<&'static str>,
}

impl SoundCloudClientGuard {
    fn enter() -> Self {
        let server = crate::client::test_server();
        soundcloud_client_sender(server).send_modify(|count| *count += 1);
        Self { server }
    }
}

impl Drop for SoundCloudClientGuard {
    fn drop(&mut self) {
        soundcloud_client_sender(self.server).send_modify(|count| *count = count.saturating_sub(1));
    }
}

type InFlightKey = (Option<&'static str>, String, String);
type InFlightMap = HashMap<InFlightKey, watch::Receiver<Option<Option<String>>>>;

fn in_flight_external() -> &'static Mutex<InFlightMap> {
    static MAP: OnceLock<Mutex<InFlightMap>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

struct InFlightGuard {
    keys: Vec<InFlightKey>,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        let mut map = in_flight_external()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for key in &self.keys {
            map.remove(key);
        }
    }
}

/// Resolves external image URLs with process-wide in-flight deduplication and
/// background completion, so slow earlier tracks never serialize newer tracks
/// and concurrent requests for the same URL share one HTTP call.
async fn resolve_external_deduped<R: RpcResolver>(
    api: Arc<R>,
    client: String,
    urls: Vec<String>,
    corr_id: Option<u64>,
    variant: &'static str,
) -> Option<Assets> {
    if urls.is_empty() {
        return None;
    }
    let server = crate::client::test_server();
    let mut receivers = Vec::with_capacity(urls.len());
    let mut to_fetch = Vec::new();
    let mut senders = Vec::new();
    let mut guard_keys = Vec::new();
    {
        let mut map = in_flight_external()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for url in urls {
            let key = (server, client.clone(), url.clone());
            if let Some(rx) = map.get(&key) {
                receivers.push((url, rx.clone(), true));
            } else {
                let (tx, rx) = watch::channel(None);
                map.insert(key.clone(), rx.clone());
                guard_keys.push(key);
                to_fetch.push(url.clone());
                senders.push((url.clone(), tx));
                receivers.push((url, rx, false));
            }
        }
    }

    if !to_fetch.is_empty() {
        let client_clone = client.clone();
        tokio::spawn(async move {
            let _guard = InFlightGuard { keys: guard_keys };
            let started = Instant::now();
            tracing::debug!(
                target: "yapfast_media",
                ?corr_id,
                variant,
                requested = to_fetch.len(),
                "artwork_resolve_start"
            );
            let refs: Vec<&str> = to_fetch.iter().map(String::as_str).collect();
            let mut attempt = tokio::time::timeout(
                EXTERNAL_TIMEOUT,
                api.rpc_external_assets(&client_clone, &refs),
            )
            .await;
            if matches!(attempt, Err(_)) {
                tokio::time::sleep(Duration::from_millis(200)).await;
                attempt = tokio::time::timeout(
                    EXTERNAL_TIMEOUT,
                    api.rpc_external_assets(&client_clone, &refs),
                )
                .await;
            }
            let (outcome, resolved) = match attempt {
                Ok(Ok(value)) => match external_lookup(&value) {
                    Ok(mut assets) => {
                        assets.retain(|url, _| refs.contains(&url.as_str()));
                        ("ok", Some(assets))
                    }
                    Err(_) => ("invalid_response", None),
                },
                Ok(Err(_)) => ("failed", None),
                Err(_) => ("timeout", None),
            };
            tracing::debug!(
                target: "yapfast_media",
                ?corr_id,
                variant,
                requested = to_fetch.len(),
                resolved = resolved.as_ref().map_or(0, Assets::len),
                elapsed_ms = started.elapsed().as_millis() as u64,
                outcome,
                "artwork_resolve_done"
            );
            if let Some(ref batch) = resolved {
                update_cached_client(&client_clone, |entry| entry.remember_external(batch.clone()));
            }
            for (url, tx) in senders {
                let val = resolved.as_ref().and_then(|map| map.get(&url).cloned());
                let _ = tx.send(Some(val));
            }
        });
    } else {
        tracing::debug!(
            target: "yapfast_media",
            ?corr_id,
            variant,
            deduped = true,
            "artwork_resolve_joined_inflight"
        );
    }

    let mut out = Assets::new();
    for (url, mut rx, _) in receivers {
        loop {
            if let Some(result) = rx.borrow_and_update().clone() {
                if let Some(mp_key) = result {
                    out.insert(url, mp_key);
                }
                break;
            }
            if rx.changed().await.is_err() {
                break;
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

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

fn proxy_image(text: &str) -> bool {
    text.starts_with("mp:external/")
        && text.len() <= 512
        && !text.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// The media proxy's names for image addresses, as `mp:` asset keys.
pub fn external_lookup(response: &Value) -> Result<Assets> {
    Ok(response
        .as_array()
        .context("invalid external assets")?
        .iter()
        .filter_map(|asset| {
            let url = asset["url"].as_str()?;
            let key = format!("mp:{}", asset["external_asset_path"].as_str()?);
            proxy_image(&key).then(|| (url.to_owned(), key))
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
        if name.eq_ignore_ascii_case("SoundCloud")
            && let Some(text) = output[key].as_str()
            && let Some(prefix) = text.strip_suffix("...")
            && prefix.chars().count() >= 126
            && text.chars().count() > 128
        {
            // SoundCloud appends its ellipsis after truncating to 128 UTF-16 units.
            let short = prefix.chars().take(127).chain(['…']).collect::<String>();
            output[key] = json!(short);
        }
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
        let mut converted = serde_json::Map::new();
        for key in ["large_image", "small_image", "large_text", "small_text"] {
            if let Some(value) = images.get(key).filter(|v| !v.is_null()) {
                let text = value.as_str().context("asset must be text")?.trim();
                if text.is_empty() {
                    continue;
                }
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
                        None if snowflake(text) || proxy_image(text) => text,
                        None if allow_pending_images => continue,
                        None => bail!("unknown application asset"),
                    }
                } else {
                    text
                };
                converted.insert(key.into(), json!(text));
            }
        }
        if !converted.is_empty() {
            output["assets"] = Value::Object(converted);
        }
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

pub(crate) trait RpcResolver: Send + Sync + 'static {
    fn rpc_application<'a>(&'a self, id: &'a str) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>>;
    fn rpc_assets<'a>(&'a self, id: &'a str) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>>;
    fn rpc_external_assets<'a>(
        &'a self,
        id: &'a str,
        urls: &'a [&'a str],
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>>;
    fn probe_proxy_image<'a>(
        &'a self,
        _mp_key: &'a str,
    ) -> Pin<Box<dyn Future<Output = ProxyProbe> + Send + 'a>> {
        Box::pin(async { ProxyProbe::Available })
    }
}

impl RpcResolver for Api {
    fn rpc_application<'a>(&'a self, id: &'a str) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>> {
        Box::pin(Api::rpc_application(self, id))
    }

    fn rpc_assets<'a>(&'a self, id: &'a str) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>> {
        Box::pin(Api::rpc_assets(self, id))
    }

    fn rpc_external_assets<'a>(
        &'a self,
        id: &'a str,
        urls: &'a [&'a str],
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>> {
        Box::pin(Api::rpc_external_assets(self, id, urls))
    }

    #[cfg(any(target_os = "linux", windows))]
    fn probe_proxy_image<'a>(
        &'a self,
        mp_key: &'a str,
    ) -> Pin<Box<dyn Future<Output = ProxyProbe> + Send + 'a>> {
        Box::pin(Api::probe_proxy_image(self, mp_key))
    }
}

fn same_track_target(a: &Value, b: &Value) -> bool {
    a["details"] == b["details"] && a["state"] == b["state"] && a["assets"] == b["assets"]
}

fn start_soundcloud_proxy_probe<R: RpcResolver>(
    api: Arc<R>,
    client: &str,
    name: &str,
    target: &Value,
    normalized: &Value,
) -> Option<Pin<Box<dyn Future<Output = (Value, Option<Assets>)> + Send>>> {
    if !name.eq_ignore_ascii_case("SoundCloud") || normalized["type"] != 2 {
        return None;
    }
    let raw_url = target["assets"]["large_image"].as_str()?.trim();
    let orig_url = soundcloud_original_thumbnail(raw_url)?;
    let mp_key = normalized["assets"]["large_image"].as_str()?.to_owned();
    let cached = load_cached_client(client);
    if cached.verified.contains(&mp_key) || cached.fallback_urls.contains_key(raw_url) {
        return None;
    }
    let raw_url = raw_url.to_owned();
    let client = client.to_owned();
    let target = target.clone();
    let corr_id = target["details"]
        .as_str()
        .map(crate::own_activity::trace_track);
    Some(Box::pin(async move {
        let started = Instant::now();
        let probe = api.probe_proxy_image(&mp_key).await;
        let elapsed_ms = started.elapsed().as_millis() as u64;
        tracing::debug!(
            target: "yapfast_media",
            ?corr_id,
            available = (probe == ProxyProbe::Available),
            missing = (probe == ProxyProbe::Missing),
            elapsed_ms,
            "artwork_proxy_probe"
        );
        match probe {
            ProxyProbe::Available => {
                update_cached_client(&client, |entry| {
                    entry.verified.insert(mp_key);
                });
                (target, None)
            }
            ProxyProbe::Missing => {
                update_cached_client(&client, |entry| {
                    entry.record_fallback(&raw_url, orig_url.clone());
                });
                let fallback = resolve_external_deduped(
                    api,
                    client,
                    vec![orig_url],
                    corr_id,
                    "fallback_50x50",
                )
                .await;
                (target, fallback)
            }
            ProxyProbe::Transient => (target, None),
        }
    }))
}

/// Generic connection handler for any async byte stream (Unix domain socket, Windows named pipe, or duplex).
pub async fn connection<S: AsyncReadExt + AsyncWriteExt + Unpin>(
    stream: S,
    api: Arc<Api>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
    updates: mpsc::Sender<Update>,
    id: u64,
) -> Result<()> {
    connection_with(stream, api, snapshots, updates, id).await
}

async fn connection_with<S: AsyncReadExt + AsyncWriteExt + Unpin, R: RpcResolver>(
    mut stream: S,
    api: Arc<R>,
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
    let mut cached = load_cached_client(&client);
    let lookup = Instant::now();
    tracing::debug!(target: "yapfast_media", cached = cached.name.is_some(), "RPC handshake received");
    let name = match cached.name.clone() {
        Some(name) => name,
        None => {
            let application = tokio::time::timeout(WAIT, api.rpc_application(&client)).await??;
            tracing::debug!(target: "yapfast_media", elapsed_ms = lookup.elapsed().as_millis() as u64, "RPC application ready");
            let name = application["name"]
                .as_str()
                .context("RPC application has no name")?
                .to_owned();
            update_cached_client(&client, |entry| entry.name = Some(name.clone()));
            cached.name = Some(name.clone());
            name
        }
    };

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

    let _soundcloud_client = name
        .eq_ignore_ascii_case("SoundCloud")
        .then(SoundCloudClientGuard::enter);
    let mut assets = cached.named.take();
    let mut images = cached.effective_external();
    type ExternalFuture = Pin<Box<dyn Future<Output = (Value, Option<Assets>)> + Send>>;
    type NamedFuture = Pin<Box<dyn Future<Output = (Value, Option<Assets>)> + Send>>;
    let mut external_task: Option<ExternalFuture> = None;
    let mut named_task: Option<NamedFuture> = None;
    let mut probe_task: Option<ExternalFuture> = None;
    let mut artwork_input = None::<Value>;
    let mut active_input = None::<Value>;
    let mut last_soundcloud_input = None::<Value>;
    let mut previous_cover = None;

    loop {
        let (opcode, payload) = {
            let frame = read(&mut stream);
            tokio::pin!(frame);
            loop {
                tokio::select! {
                    biased;
                    result = &mut frame => break result?,
                    (resolved_for, external) = async { external_task.as_mut().unwrap().await }, if external_task.is_some() => {
                        external_task = None;
                        let target = artwork_input.clone().unwrap_or(resolved_for);
                        if external.is_some() {
                            images = load_cached_client(&client).effective_external();
                        }
                        if named_task.is_none() {
                            artwork_input = None;
                        }
                        let still_current = active_input.as_ref().is_some_and(|cur| same_track_target(cur, &target));
                        let paused_current = active_input.is_none()
                            && last_soundcloud_input.as_ref().is_some_and(|last| same_track_target(last, &target));
                        if still_current
                            && let Some(current) = active_input.as_ref()
                            && let Ok(value) = normalize_activity(current, &client, &name, assets.as_ref(), &images, true)
                        {
                            if probe_task.is_none() {
                                probe_task = start_soundcloud_proxy_probe(api.clone(), &client, &name, current, &value);
                            }
                            let mut marked = value;
                            if named_task.is_some() {
                                marked["_artwork_pending"] = json!(true);
                            }
                            // Emit even when `external` failed so Gateway clears `_artwork_pending` immediately.
                            updates.send((id, Some(Arc::new(marked)), true)).await?;
                        } else if paused_current
                            && let Some(paused_in) = last_soundcloud_input.as_ref()
                            && let Ok(value) = normalize_activity(paused_in, &client, &name, assets.as_ref(), &images, true)
                            && (value["assets"]["large_image"].is_string() || value["assets"]["small_image"].is_string())
                        {
                            if probe_task.is_none() {
                                probe_task = start_soundcloud_proxy_probe(api.clone(), &client, &name, paused_in, &value);
                            }
                            let _ = late_soundcloud_channel().send(Some(Arc::new(value)));
                        }
                    }
                    (resolved_for, named) = async { named_task.as_mut().unwrap().await }, if named_task.is_some() => {
                        named_task = None;
                        let target = artwork_input.clone().unwrap_or(resolved_for);
                        if external_task.is_none() {
                            artwork_input = None;
                        }
                        if let Some(named) = named {
                            update_cached_client(&client, |entry| entry.named = Some(named.clone()));
                            assets = Some(named);
                        }
                        let still_current = active_input.as_ref().is_some_and(|cur| same_track_target(cur, &target));
                        if still_current
                            && let Some(current) = active_input.as_ref()
                            && let Ok(value) = normalize_activity(current, &client, &name, assets.as_ref(), &images, true)
                        {
                            let mut marked = value;
                            if external_task.is_some() {
                                marked["_artwork_pending"] = json!(true);
                            }
                            updates.send((id, Some(Arc::new(marked)), true)).await?;
                        }
                    }
                    (probed_for, fallback_external) = async { probe_task.as_mut().unwrap().await }, if probe_task.is_some() => {
                        probe_task = None;
                        if fallback_external.is_some() {
                            images = load_cached_client(&client).effective_external();
                            let still_current = active_input.as_ref().is_some_and(|cur| same_track_target(cur, &probed_for));
                            let paused_current = active_input.is_none()
                                && last_soundcloud_input.as_ref().is_some_and(|last| same_track_target(last, &probed_for));
                            if still_current
                                && let Some(current) = active_input.as_ref()
                                && let Ok(value) = normalize_activity(current, &client, &name, assets.as_ref(), &images, true)
                            {
                                updates.send((id, Some(Arc::new(value)), true)).await?;
                            } else if paused_current
                                && let Some(paused_in) = last_soundcloud_input.as_ref()
                                && let Ok(value) = normalize_activity(paused_in, &client, &name, assets.as_ref(), &images, true)
                                && value["assets"]["large_image"].is_string()
                            {
                                let _ = late_soundcloud_channel().send(Some(Arc::new(value)));
                            }
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
        images = load_cached_client(&client).effective_external();
        let result = (|| {
            let args = &packet["args"];
            ensure!(
                args["pid"].as_u64().is_some_and(|pid| pid > 0),
                "pid must be a positive integer"
            );
            // Both `"activity": null` and omitting `"activity"` (`{"pid": pid}`, as sent by
            // `soundcloud-rs` and `@xhayper/discord-rpc` on `clearActivity`) clear the activity.
            let Some(input) = args.get("activity").filter(|v| !v.is_null()) else {
                return Ok(None);
            };
            normalize_activity(input, &client, &name, assets.as_ref(), &images, true)
                .map(|value| Some(Arc::new(value)))
        })();

        match result {
            Ok(value) => {
                let is_clear = value.is_none();
                let eligible = value.as_deref().is_some_and(crate::own_activity::eligible);
                let input = &packet["args"]["activity"];
                active_input = eligible.then(|| input.clone());
                if eligible
                    && name.eq_ignore_ascii_case("SoundCloud")
                    && value.as_ref().is_some_and(|v| v["type"] == 2)
                {
                    last_soundcloud_input = Some(input.clone());
                } else if !is_clear {
                    last_soundcloud_input = None;
                }

                // Keep an in-flight artwork lookup/probe running across a pause clear (`is_clear`)
                // so a track paused before its cover finishes still receives its cover on pause.
                let same_artwork = eligible
                    && (external_task.is_some() || named_task.is_some())
                    && artwork_input
                        .as_ref()
                        .is_some_and(|old| old["assets"] == input["assets"]);
                if same_artwork {
                    artwork_input = Some(input.clone());
                } else if !is_clear {
                    external_task = None;
                    named_task = None;
                    probe_task = None;
                    artwork_input = None;
                }

                if !same_artwork && eligible {
                    let corr_id = input["details"]
                        .as_str()
                        .map(crate::own_activity::trace_track);
                    if name == "SoundCloud" {
                        let image = input["assets"]["large_image"]
                            .as_str()
                            .unwrap_or_default()
                            .trim();
                        let cache = if web_address(image, 512) {
                            if images.contains_key(image) {
                                update_cached_client(&client, |entry| entry.touch_external(image));
                                "hit"
                            } else {
                                "miss"
                            }
                        } else if image.is_empty() {
                            "absent"
                        } else {
                            "asset_key"
                        };
                        let key = (corr_id, crate::own_activity::trace_track(image), cache);
                        if previous_cover != Some(key) {
                            previous_cover = Some(key);
                            let arrival_ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64;
                            tracing::debug!(
                                target: "yapfast_media",
                                ?corr_id,
                                artwork_cache = cache,
                                arrival_ms,
                                "rpc_track_received"
                            );
                        }
                    }
                    let pictures = &input["assets"];
                    let mut addresses = Vec::new();
                    let mut fetch_named = false;
                    for key in ["large_image", "small_image"] {
                        if let Some(image) = pictures[key].as_str().map(str::trim).filter(|s| !s.is_empty()) {
                            if web_address(image, 512) {
                                if !images.contains_key(image) {
                                    addresses.push(image.to_owned());
                                }
                            } else if !snowflake(image) && !proxy_image(image) && assets.is_none() {
                                fetch_named = true;
                            }
                        }
                    }
                    if fetch_named || !addresses.is_empty() {
                        artwork_input = Some(input.clone());
                    }
                    if !addresses.is_empty() {
                        let input_for_ext = input.clone();
                        let api_ext = api.clone();
                        let client_ext = client.clone();
                        external_task = Some(Box::pin(async move {
                            let resolved = resolve_external_deduped(
                                api_ext,
                                client_ext,
                                addresses,
                                corr_id,
                                "primary",
                            )
                            .await;
                            (input_for_ext, resolved)
                        }));
                    }
                    if fetch_named {
                        let input_for_named = input.clone();
                        let api_named = api.clone();
                        let client_named = client.clone();
                        named_task = Some(Box::pin(async move {
                            let response = tokio::time::timeout(
                                EXTERNAL_TIMEOUT,
                                api_named.rpc_assets(&client_named),
                            )
                            .await
                            .ok()
                            .and_then(Result::ok);
                            (input_for_named, response.and_then(|v| asset_lookup(&v).ok()))
                        }));
                    }
                    if external_task.is_none()
                        && let Some(ref norm) = value
                    {
                        probe_task = start_soundcloud_proxy_probe(
                            api.clone(),
                            &client,
                            &name,
                            input,
                            norm,
                        );
                    }
                }
                let artwork_in_flight = external_task.is_some() || named_task.is_some();
                let published = value.clone().filter(|_| eligible).map(|arc| {
                    if artwork_in_flight {
                        let mut marked = (*arc).clone();
                        marked["_artwork_pending"] = json!(true);
                        Arc::new(marked)
                    } else {
                        arc
                    }
                });
                // Bound queued updates even when games publish very frequently.
                updates.send((id, published, false)).await?;
                let published_at_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                tracing::debug!(
                    target: "yapfast_media",
                    source = "rpc",
                    active = value.is_some(),
                    has_cover = value.as_ref().is_some_and(|v| v["assets"]["large_image"].is_string()),
                    artwork_pending = artwork_in_flight,
                    elapsed_ms = received.elapsed().as_millis() as u64,
                    published_at_ms,
                    "RPC state published"
                );
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
    fn soundcloud_accepts_its_overlong_ellipsis_and_keeps_the_cover() {
        let input = json!({
            "type": 2, "details": format!("{}...", "a".repeat(128)),
            "assets": {"large_image":"mp:external/abc/https/example.com/cover.png"}
        });
        let activity = normalize(&input, "42", "SoundCloud", None, &Assets::new()).unwrap();
        assert_eq!(activity["details"], format!("{}…", "a".repeat(127)));
        assert_eq!(activity["assets"], input["assets"]);
        assert!(normalize(&input, "42", "Game", None, &Assets::new()).is_err());
    }

    #[test]
    fn preserves_resolved_cover_from_rpc() {
        let input = json!({"assets":{"large_image":"mp:external/abc/https/example.com/cover.png"}});
        let activity = normalize(&input, "42", "SoundCloud", None, &Assets::new()).unwrap();
        assert_eq!(
            activity["assets"]["large_image"],
            input["assets"]["large_image"]
        );
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

    #[test]
    fn ignores_empty_optional_assets_from_soundcloud_rpc_and_evicts_lru_gradually() {
        let images = external_lookup(&json!([{
            "url": "https://i1.sndcdn.com/artworks-test-t500x500.jpg",
            "external_asset_path": "external/sig/https/i1.sndcdn.com/artworks-test-t500x500.jpg"
        }]))
        .unwrap();
        let input = json!({
            "type": 2,
            "details": "Archangel",
            "state": "Burial",
            "assets": {
                "large_image": "https://i1.sndcdn.com/artworks-test-t500x500.jpg",
                "small_image": "",
                "small_text": ""
            }
        });
        let shown = normalize(&input, "1090770350251458592", "SoundCloud", None, &images).unwrap();
        assert_eq!(
            shown["assets"],
            json!({"large_image": "mp:external/sig/https/i1.sndcdn.com/artworks-test-t500x500.jpg"})
        );

        let empty_only = json!({
            "type": 2,
            "details": "No cover",
            "state": "Artist",
            "assets": {"large_image": "  ", "small_image": ""}
        });
        let without =
            normalize(&empty_only, "1090770350251458592", "SoundCloud", None, &Assets::new())
                .unwrap();
        assert!(without.get("assets").is_none());

        let mut cached = CachedClient::default();
        for index in 0..(KEPT_IMAGES + 5) {
            let mut batch = Assets::new();
            batch.insert(
                format!("https://i1.sndcdn.com/artworks-{index}.jpg"),
                format!("mp:external/sig/{index}"),
            );
            cached.remember_external(batch);
            if index == 2 {
                cached.touch_external("https://i1.sndcdn.com/artworks-0.jpg");
            }
        }
        assert_eq!(cached.external.len(), KEPT_IMAGES);
        assert!(
            cached
                .external
                .contains_key("https://i1.sndcdn.com/artworks-132.jpg")
        );
        assert!(
            !cached
                .external
                .contains_key("https://i1.sndcdn.com/artworks-1.jpg")
        );
    }

    #[tokio::test]
    async fn fast_and_slow_artwork_cache_reuse_rapid_switches_and_stale_cover_guard() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct MockRpcApi {
            app_calls: AtomicUsize,
            asset_calls: AtomicUsize,
            external_calls: AtomicUsize,
        }

        impl RpcResolver for MockRpcApi {
            fn rpc_application<'a>(
                &'a self,
                _id: &'a str,
            ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>> {
                self.app_calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(json!({"name": "SoundCloud"})) })
            }

            fn rpc_assets<'a>(
                &'a self,
                _id: &'a str,
            ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>> {
                self.asset_calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(json!([{"name": "soundcloud-logo", "id": "777"}])) })
            }

            fn rpc_external_assets<'a>(
                &'a self,
                _id: &'a str,
                urls: &'a [&'a str],
            ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>> {
                self.external_calls.fetch_add(1, Ordering::SeqCst);
                let urls: Vec<String> = urls.iter().map(|u| (*u).to_owned()).collect();
                Box::pin(async move {
                    let mut out = Vec::new();
                    for url in urls {
                        if url.contains("slow") {
                            tokio::time::sleep(Duration::from_millis(320)).await;
                        } else if url.contains("fail") {
                            tokio::time::sleep(Duration::from_millis(25)).await;
                            bail!("503 external-assets failed");
                        } else {
                            tokio::time::sleep(Duration::from_millis(25)).await;
                        }
                        let tail = url.rsplit('/').next().unwrap_or("art.jpg");
                        out.push(json!({
                            "url": url,
                            "external_asset_path": format!("external/mock/https/i1.sndcdn.com/{tail}")
                        }));
                    }
                    Ok(Value::Array(out))
                })
            }
        }

        let client_id = "990770350251458592";
        let resolver = Arc::new(MockRpcApi {
            app_calls: AtomicUsize::new(0),
            asset_calls: AtomicUsize::new(0),
            external_calls: AtomicUsize::new(0),
        });
        let snapshot = Snapshot {
            status: Status::Online,
            user_id: Some("1".into()),
            ..Default::default()
        };
        let (_snap_tx, snap_rx) = watch::channel(Arc::new(snapshot));
        let (updates_tx, mut updates_rx) = mpsc::channel(32);

        let (mut ipc, server) = tokio::io::duplex(8192);
        let resolver_clone = resolver.clone();
        let snap_rx_clone = snap_rx.clone();
        let updates_tx_clone = updates_tx.clone();
        let task = tokio::spawn(async move {
            connection_with(server, resolver_clone, snap_rx_clone, updates_tx_clone, 1).await
        });

        write(
            &mut ipc,
            0,
            &serde_json::to_vec(&json!({"v": 1, "client_id": client_id})).unwrap(),
        )
        .await
        .unwrap();
        let (_, ready_bytes) = read(&mut ipc).await.unwrap();
        let ready: Value = serde_json::from_slice(&ready_bytes).unwrap();
        assert_eq!(ready["evt"], "READY");

        let send_track = |nonce: &str, title: &str, art: &str| {
            json!({
                "cmd": "SET_ACTIVITY",
                "nonce": nonce,
                "args": {
                    "pid": 100,
                    "activity": {
                        "type": 2,
                        "details": title,
                        "state": "Burial",
                        "assets": {
                            "large_image": art,
                            "small_image": ""
                        }
                    }
                }
            })
        };

        // 1. Cold fast track (25ms external-assets): ACK and pending update arrive in <5ms, cover resolves at ~25ms.
        let t0 = Instant::now();
        write(
            &mut ipc,
            1,
            &serde_json::to_vec(&send_track(
                "fast-1",
                "Fast Track",
                "https://i1.sndcdn.com/fast-cover.jpg",
            ))
            .unwrap(),
        )
        .await
        .unwrap();
        let (_, ack_bytes) = read(&mut ipc).await.unwrap();
        let ack_ms = t0.elapsed().as_millis();
        let ack: Value = serde_json::from_slice(&ack_bytes).unwrap();
        assert_eq!(ack["nonce"], "fast-1");

        let (_, initial, _) = updates_rx.recv().await.unwrap();
        let initial = initial.unwrap();
        assert_eq!(initial["details"], "Fast Track");
        assert_eq!(initial["_artwork_pending"], true);
        assert!(initial.get("assets").is_none());

        let (_, resolved, _) = updates_rx.recv().await.unwrap();
        let fast_cover_ms = t0.elapsed().as_millis();
        let resolved = resolved.unwrap();
        assert!(resolved.get("_artwork_pending").is_none());
        assert_eq!(
            resolved["assets"]["large_image"],
            "mp:external/mock/https/i1.sndcdn.com/fast-cover.jpg"
        );
        eprintln!(
            "[proto] cold fast track: IPC ACK={}ms, cover resolved={}ms",
            ack_ms, fast_cover_ms
        );
        assert_eq!(resolver.asset_calls.load(Ordering::SeqCst), 0);
        assert_eq!(resolver.external_calls.load(Ordering::SeqCst), 1);

        // 2. Rapid track switches over slow/failed covers: stale slow cover must not leak onto final track.
        let switch_t0 = Instant::now();
        for (nonce, title, art) in [
            ("rapid-1", "Slow One", "https://i1.sndcdn.com/slow-one.jpg"),
            ("rapid-2", "Failed Two", "https://i1.sndcdn.com/fail-two.jpg"),
            ("rapid-3", "Final Three", "https://i1.sndcdn.com/fast-three.jpg"),
        ] {
            write(
                &mut ipc,
                1,
                &serde_json::to_vec(&send_track(nonce, title, art)).unwrap(),
            )
            .await
            .unwrap();
            let (_, reply) = read(&mut ipc).await.unwrap();
            let reply: Value = serde_json::from_slice(&reply).unwrap();
            assert_eq!(reply["nonce"], nonce);
        }
        let mut last_final = None;
        while let Ok(Some((_, Some(update), _))) =
            tokio::time::timeout(Duration::from_millis(420), updates_rx.recv()).await
        {
            if update["details"] == "Final Three" && update["assets"]["large_image"].is_string() {
                last_final = Some(update);
            } else {
                assert_ne!(
                    update["assets"]["large_image"],
                    "mp:external/mock/https/i1.sndcdn.com/slow-one.jpg",
                    "stale cover from Slow One must never attach to a newer track"
                );
            }
        }
        let last_final = last_final.expect("Final Three must receive its own resolved cover");
        assert_eq!(
            last_final["assets"]["large_image"],
            "mp:external/mock/https/i1.sndcdn.com/fast-three.jpg"
        );
        eprintln!(
            "[proto] rapid 3-track burst settled cleanly in {}ms",
            switch_t0.elapsed().as_millis()
        );

        // 3. Reconnect RPC pipe and replay Fast Track: warm LRU cache resolves immediately in 0 HTTP calls!
        drop(ipc);
        let _ = task.await;
        let external_before_reconnect = resolver.external_calls.load(Ordering::SeqCst);
        let app_before_reconnect = resolver.app_calls.load(Ordering::SeqCst);

        let (mut ipc2, server2) = tokio::io::duplex(8192);
        let resolver_clone = resolver.clone();
        let task2 = tokio::spawn(async move {
            connection_with(server2, resolver_clone, snap_rx, updates_tx, 2).await
        });
        write(
            &mut ipc2,
            0,
            &serde_json::to_vec(&json!({"v": 1, "client_id": client_id})).unwrap(),
        )
        .await
        .unwrap();
        let _ = read(&mut ipc2).await.unwrap();

        let warm_t0 = Instant::now();
        write(
            &mut ipc2,
            1,
            &serde_json::to_vec(&send_track(
                "warm-1",
                "Fast Track",
                "https://i1.sndcdn.com/fast-cover.jpg",
            ))
            .unwrap(),
        )
        .await
        .unwrap();
        let (_, warm_ack_bytes) = read(&mut ipc2).await.unwrap();
        let warm_ack: Value = serde_json::from_slice(&warm_ack_bytes).unwrap();
        assert_eq!(
            warm_ack["data"]["assets"]["large_image"],
            "mp:external/mock/https/i1.sndcdn.com/fast-cover.jpg"
        );
        let (_, warm_update, _) = updates_rx.recv().await.unwrap();
        let warm_ms = warm_t0.elapsed().as_millis();
        let warm_update = warm_update.unwrap();
        assert!(warm_update.get("_artwork_pending").is_none());
        assert_eq!(
            warm_update["assets"]["large_image"],
            "mp:external/mock/https/i1.sndcdn.com/fast-cover.jpg"
        );
        assert_eq!(
            resolver.external_calls.load(Ordering::SeqCst),
            external_before_reconnect,
            "warm cache across RPC reconnect must make 0 external-assets calls"
        );
        assert_eq!(
            resolver.app_calls.load(Ordering::SeqCst),
            app_before_reconnect,
            "cached application name across RPC reconnect must make 0 app calls"
        );
        eprintln!("[proto] warm cached track across reconnect: {}ms", warm_ms);

        drop(ipc2);
        let _ = task2.await;
    }

    #[tokio::test]
    async fn set_activity_clear_without_activity_field_and_delayed_artwork_across_pause() {
        struct DelayedCoverApi;

        impl RpcResolver for DelayedCoverApi {
            fn rpc_application<'a>(
                &'a self,
                _id: &'a str,
            ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>> {
                Box::pin(async { Ok(json!({"name": "SoundCloud"})) })
            }

            fn rpc_assets<'a>(
                &'a self,
                _id: &'a str,
            ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>> {
                Box::pin(async { Ok(json!([])) })
            }

            fn rpc_external_assets<'a>(
                &'a self,
                _id: &'a str,
                urls: &'a [&'a str],
            ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>> {
                let urls: Vec<String> = urls.iter().map(|u| (*u).to_owned()).collect();
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_millis(180)).await;
                    Ok(Value::Array(
                        urls.into_iter()
                            .map(|url| {
                                let tail = url.rsplit('/').next().unwrap_or("art.jpg");
                                json!({
                                    "url": url,
                                    "external_asset_path": format!("external/pause/https/i1.sndcdn.com/{tail}")
                                })
                            })
                            .collect(),
                    ))
                })
            }
        }

        let client_id = "880770350251458592";
        let resolver = Arc::new(DelayedCoverApi);
        let snapshot = Snapshot {
            status: Status::Online,
            user_id: Some("1".into()),
            ..Default::default()
        };
        let (_snap_tx, snap_rx) = watch::channel(Arc::new(snapshot));
        let (updates_tx, mut updates_rx) = mpsc::channel(16);
        let mut late_rx = subscribe_late_soundcloud_artwork();
        let _ = late_rx.borrow_and_update();

        let (mut ipc, server) = tokio::io::duplex(8192);
        let task = tokio::spawn(async move {
            connection_with(server, resolver, snap_rx, updates_tx, 7).await
        });

        write(
            &mut ipc,
            0,
            &serde_json::to_vec(&json!({"v": 1, "client_id": client_id})).unwrap(),
        )
        .await
        .unwrap();
        let _ = read(&mut ipc).await.unwrap();

        // Start playing track with slow 180ms artwork:
        write(
            &mut ipc,
            1,
            &serde_json::to_vec(&json!({
                "cmd": "SET_ACTIVITY",
                "nonce": "play-1",
                "args": {
                    "pid": 6796,
                    "activity": {
                        "type": 2,
                        "details": "Untrue",
                        "state": "Burial",
                        "timestamps": {"start": 1_800_000_000_000_u64, "end": 1_800_000_376_000_u64},
                        "assets": {"large_image": "https://i1.sndcdn.com/artworks-untrue-t500x500.jpg"}
                    }
                }
            }))
            .unwrap(),
        )
        .await
        .unwrap();
        let (_, play_ack) = read(&mut ipc).await.unwrap();
        let play_ack: Value = serde_json::from_slice(&play_ack).unwrap();
        assert!(play_ack["evt"].is_null());
        let (_, first_pub, _) = updates_rx.recv().await.unwrap();
        assert_eq!(first_pub.unwrap()["details"], "Untrue");

        // Pause 30ms later: `soundcloud-rs.exe` sends `{"pid": 6796}` with NO `"activity"` field!
        tokio::time::sleep(Duration::from_millis(30)).await;
        write(
            &mut ipc,
            1,
            &serde_json::to_vec(&json!({
                "cmd": "SET_ACTIVITY",
                "nonce": "pause-clear",
                "args": {"pid": 6796}
            }))
            .unwrap(),
        )
        .await
        .unwrap();
        let (_, clear_ack) = read(&mut ipc).await.unwrap();
        let clear_ack: Value = serde_json::from_slice(&clear_ack).unwrap();
        assert!(
            clear_ack["evt"].is_null() && clear_ack["data"].is_null(),
            "omitted activity field must clear cleanly instead of returning ERROR: {clear_ack}"
        );
        let (_, cleared_pub, artwork_only) = updates_rx.recv().await.unwrap();
        assert!(cleared_pub.is_none() && !artwork_only);

        // Delayed artwork completes ~150ms later into `late_soundcloud_artwork` without resurrecting active RPC!
        tokio::time::timeout(Duration::from_millis(400), async {
            loop {
                late_rx.changed().await.unwrap();
                if let Some(card) = late_rx.borrow_and_update().clone()
                    && card["details"] == "Untrue"
                {
                    assert_eq!(
                        card["assets"]["large_image"],
                        "mp:external/pause/https/i1.sndcdn.com/artworks-untrue-t500x500.jpg"
                    );
                    break;
                }
            }
        })
        .await
        .expect("delayed cover must still resolve across pause-clear");

        drop(ipc);
        let _ = task.await;
    }

    #[tokio::test]
    async fn rewritten_soundcloud_500x500_falls_back_to_50x50_deduplicates_inflight_and_invalidates()
    {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct FallbackApi {
            external_calls: AtomicUsize,
            probe_calls: AtomicUsize,
        }

        impl RpcResolver for FallbackApi {
            fn rpc_application<'a>(
                &'a self,
                _id: &'a str,
            ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>> {
                Box::pin(async { Ok(json!({"name": "SoundCloud"})) })
            }

            fn rpc_assets<'a>(
                &'a self,
                _id: &'a str,
            ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>> {
                Box::pin(async { Ok(json!([])) })
            }

            fn rpc_external_assets<'a>(
                &'a self,
                _id: &'a str,
                urls: &'a [&'a str],
            ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>> {
                self.external_calls.fetch_add(1, Ordering::SeqCst);
                let urls: Vec<String> = urls.iter().map(|u| (*u).to_owned()).collect();
                Box::pin(async move {
                    if urls.iter().any(|u| u.contains("dedup")) {
                        tokio::time::sleep(Duration::from_millis(80)).await;
                    } else {
                        tokio::time::sleep(Duration::from_millis(15)).await;
                    }
                    Ok(Value::Array(
                        urls.into_iter()
                            .map(|url| {
                                let tail = url.rsplit('/').next().unwrap_or("art.jpg");
                                json!({
                                    "url": url,
                                    "external_asset_path": format!("external/fb/https/i1.sndcdn.com/{tail}")
                                })
                            })
                            .collect(),
                    ))
                })
            }

            fn probe_proxy_image<'a>(
                &'a self,
                mp_key: &'a str,
            ) -> Pin<Box<dyn Future<Output = ProxyProbe> + Send + 'a>> {
                self.probe_calls.fetch_add(1, Ordering::SeqCst);
                let missing = mp_key.contains("legacy-t500x500.jpg");
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_millis(15)).await;
                    if missing {
                        ProxyProbe::Missing
                    } else {
                        ProxyProbe::Available
                    }
                })
            }
        }

        let client_id = "770770350251458592";
        let resolver = Arc::new(FallbackApi {
            external_calls: AtomicUsize::new(0),
            probe_calls: AtomicUsize::new(0),
        });

        // 1. Concurrent in-flight deduplication: two simultaneous lookups for the same URL make 1 HTTP call.
        let (r1, r2) = tokio::join!(
            resolve_external_deduped(
                resolver.clone(),
                client_id.into(),
                vec!["https://i1.sndcdn.com/artworks-dedup-t500x500.jpg".into()],
                Some(1),
                "primary"
            ),
            resolve_external_deduped(
                resolver.clone(),
                client_id.into(),
                vec!["https://i1.sndcdn.com/artworks-dedup-t500x500.jpg".into()],
                Some(1),
                "primary"
            )
        );
        assert_eq!(r1, r2);
        assert_eq!(resolver.external_calls.load(Ordering::SeqCst), 1);

        let snapshot = Snapshot {
            status: Status::Online,
            user_id: Some("1".into()),
            ..Default::default()
        };
        let (_snap_tx, snap_rx) = watch::channel(Arc::new(snapshot));
        let (updates_tx, mut updates_rx) = mpsc::channel(32);
        let (mut ipc, server) = tokio::io::duplex(8192);
        let resolver_clone = resolver.clone();
        let snap_clone = snap_rx.clone();
        let updates_clone = updates_tx.clone();
        let task = tokio::spawn(async move {
            connection_with(server, resolver_clone, snap_clone, updates_clone, 9).await
        });

        write(
            &mut ipc,
            0,
            &serde_json::to_vec(&json!({"v": 1, "client_id": client_id})).unwrap(),
        )
        .await
        .unwrap();
        let _ = read(&mut ipc).await.unwrap();

        // 2. Rewritten 500x500 URL whose proxy image returns 404 -> automatically falls back to 50x50!
        write(
            &mut ipc,
            1,
            &serde_json::to_vec(&json!({
                "cmd": "SET_ACTIVITY",
                "nonce": "fb-1",
                "args": {
                    "pid": 100,
                    "activity": {
                        "type": 2,
                        "details": "Legacy Upload",
                        "state": "Artist",
                        "assets": {"large_image": "https://i1.sndcdn.com/artworks-legacy-t500x500.jpg"}
                    }
                }
            }))
            .unwrap(),
        )
        .await
        .unwrap();
        let _ = read(&mut ipc).await.unwrap();

        let mut saw_fallback_50 = false;
        while let Ok(Some((_, Some(update), _))) =
            tokio::time::timeout(Duration::from_millis(250), updates_rx.recv()).await
        {
            if update["assets"]["large_image"]
                == "mp:external/fb/https/i1.sndcdn.com/artworks-legacy-t50x50.jpg"
            {
                saw_fallback_50 = true;
                break;
            }
        }
        assert!(saw_fallback_50, "must replace broken 500x500 with resolved 50x50 cover");

        // 3. Reconnect RPC pipe & send same 500x500 URL again -> immediate 0-HTTP-call cache hit to 50x50!
        drop(ipc);
        let _ = task.await;
        let ext_before = resolver.external_calls.load(Ordering::SeqCst);
        let (mut ipc2, server2) = tokio::io::duplex(8192);
        let resolver_clone = resolver.clone();
        let task2 = tokio::spawn(async move {
            connection_with(server2, resolver_clone, snap_rx, updates_tx, 10).await
        });
        write(
            &mut ipc2,
            0,
            &serde_json::to_vec(&json!({"v": 1, "client_id": client_id})).unwrap(),
        )
        .await
        .unwrap();
        let _ = read(&mut ipc2).await.unwrap();
        write(
            &mut ipc2,
            1,
            &serde_json::to_vec(&json!({
                "cmd": "SET_ACTIVITY",
                "nonce": "fb-replay",
                "args": {
                    "pid": 100,
                    "activity": {
                        "type": 2,
                        "details": "Legacy Upload",
                        "state": "Artist",
                        "assets": {"large_image": "https://i1.sndcdn.com/artworks-legacy-t500x500.jpg"}
                    }
                }
            }))
            .unwrap(),
        )
        .await
        .unwrap();
        let (_, ack2) = read(&mut ipc2).await.unwrap();
        let ack2: Value = serde_json::from_slice(&ack2).unwrap();
        assert_eq!(
            ack2["data"]["assets"]["large_image"],
            "mp:external/fb/https/i1.sndcdn.com/artworks-legacy-t50x50.jpg"
        );
        assert_eq!(resolver.external_calls.load(Ordering::SeqCst), ext_before);

        // 4. Invalidating an external proxy URL evicts it from CachedClient.
        invalidate_external_asset(
            "https://media.discordapp.net/external/fb/https/i1.sndcdn.com/artworks-dedup-t500x500.jpg?width=96&height=96",
        );
        assert!(
            !load_cached_client(client_id)
                .external
                .contains_key("https://i1.sndcdn.com/artworks-dedup-t500x500.jpg")
        );

        drop(ipc2);
        let _ = task2.await;
    }
}
