//! End-to-end verification for incoming Go Live stream resilience (SF-01..SF-03):
//! - SF-01: HEVC repeated IDRs and packet-loss + PLI recovery IDRs with SPS frame reordering
//! - SF-02: Mid-stream SFU simulcast SSRC switch (`5001 -> 5011`) without reopening
//! - SF-03: 10-bit HEVC Main 10 HDR (`-2`) immediate fallback to 8-bit H264 without reopening
//! - Uninterrupted Opus stream audio and concurrent second watched stream throughout.

#![cfg(feature = "discord")]

use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use yapfast_core::{
    decode::{DECODES, Decoder, is_unsupported_format},
    yapfast_discord::{
        opus,
        rtp::Codec,
        stream::StreamServer,
        stream_connection::{Link, StreamReceiver},
    },
};

struct DualStreamServer {
    child: Child,
    port: u16,
}

impl DualStreamServer {
    fn start() -> Self {
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testing/fake_dual_stream_server.ts");
        let mut child = Command::new("bun")
            .arg(&script)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("bun must be on PATH to run fake_dual_stream_server.ts");
        let stdout = child.stdout.take().unwrap();
        let mut line = String::new();
        BufReader::new(stdout)
            .read_line(&mut line)
            .expect("server startup line");
        let url = line
            .trim()
            .strip_prefix("LISTENING ")
            .expect("LISTENING url");
        let port: u16 = url
            .rsplit(':')
            .next()
            .unwrap()
            .parse()
            .expect("server port");
        Self { child, port }
    }

    fn server_for(&self, streamer_id: &str, session: &str) -> StreamServer {
        let stream_key = format!("guild:100:200:{streamer_id}");
        StreamServer {
            token: format!("stream-token-{stream_key}"),
            stream_key,
            user_id: "1400000000000000001".into(),
            session_id: session.into(),
            rtc_server_id: "100".into(),
            endpoint: format!("ws://127.0.0.1:{}/voice", self.port),
        }
    }
}

impl Drop for DualStreamServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct WatchedMetrics {
    pictures_phase1: usize,
    pictures_phase2: usize,
    pictures_phase3: usize,
    pictures_after_rollovers: usize,
    audio_decoded: usize,
    audio_errors: usize,
    max_audio_gap_ms: u128,
    saw_h265: bool,
    saw_h264: bool,
    saw_unsupported_format: bool,
}

fn watch_for_duration(server: StreamServer, duration: Duration) -> WatchedMetrics {
    let mut receiver = StreamReceiver::start(server, DECODES.to_vec()).unwrap();
    receiver.track_decoded();
    let sounds = receiver.take_sounds().unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let audio_decoded = Arc::new(AtomicUsize::new(0));
    let audio_errors = Arc::new(AtomicUsize::new(0));
    let max_gap_ms = Arc::new(AtomicUsize::new(0));

    let audio_thread = {
        let stop = stop.clone();
        let audio_decoded = audio_decoded.clone();
        let audio_errors = audio_errors.clone();
        let max_gap_ms = max_gap_ms.clone();
        std::thread::spawn(move || {
            let mut opus_dec = opus::Decoder::new(2).unwrap();
            let mut pcm = vec![0i16; 48 * 120 * 2];
            let mut last_packet_at: Option<Instant> = None;
            while !stop.load(Ordering::Relaxed) {
                match sounds.recv_timeout(Duration::from_millis(50)) {
                    Ok(packet) => {
                        let now = Instant::now();
                        if let Some(prev) = last_packet_at {
                            let gap = now.duration_since(prev).as_millis() as usize;
                            max_gap_ms.fetch_max(gap, Ordering::Relaxed);
                        }
                        last_packet_at = Some(now);
                        match opus_dec.decode(&packet.data, &mut pcm, false) {
                            Ok(samples) if samples > 0 => {
                                audio_decoded.fetch_add(1, Ordering::Relaxed);
                            }
                            _ => {
                                audio_errors.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
    };

    let started = Instant::now();
    let mut decoder: Option<Decoder> = None;
    let mut waiting = true;
    let mut consecutive_failures = 0u32;
    let mut metrics = WatchedMetrics {
        pictures_phase1: 0,
        pictures_phase2: 0,
        pictures_phase3: 0,
        pictures_after_rollovers: 0,
        audio_decoded: 0,
        audio_errors: 0,
        max_audio_gap_ms: 0,
        saw_h265: false,
        saw_h264: false,
        saw_unsupported_format: false,
    };

    while started.elapsed() < duration {
        if let Link::Ended(reason) = receiver.link() {
            panic!("stream connection ended unexpectedly: {reason}");
        }
        let Some(frame) = receiver.next(Duration::from_millis(50)) else {
            continue;
        };
        let codec_changed = decoder
            .as_ref()
            .is_none_or(|current| current.codec() != frame.codec);
        if (waiting || codec_changed) && !frame.keyframe {
            continue;
        }
        if codec_changed {
            decoder = Some(Decoder::new(frame.codec).unwrap());
        }
        waiting = false;
        match decoder.as_mut().unwrap().decode(&frame.data) {
            Ok(Some(picture)) => {
                assert!(picture.width >= 160 && picture.height >= 90);
                consecutive_failures = 0;
                receiver.note_decoded();
                match frame.codec {
                    Codec::H265 => metrics.saw_h265 = true,
                    Codec::H264 => metrics.saw_h264 = true,
                    _ => {}
                }
                let elapsed = started.elapsed();
                if elapsed >= Duration::from_millis(10_900) {
                    metrics.pictures_after_rollovers += 1;
                }
                if elapsed < Duration::from_millis(1200) {
                    metrics.pictures_phase1 += 1;
                } else if elapsed < Duration::from_millis(3400) {
                    metrics.pictures_phase2 += 1;
                } else {
                    metrics.pictures_phase3 += 1;
                }
            }
            Ok(None) => {}
            Err(error) => {
                consecutive_failures += 1;
                if is_unsupported_format(&error) {
                    metrics.saw_unsupported_format = true;
                    decoder = None;
                    waiting = true;
                    receiver.reject_codec();
                    continue;
                }
                if frame.codec != Codec::H264 || consecutive_failures >= 3 {
                    decoder = None;
                    waiting = true;
                }
                receiver.request_keyframe();
            }
        }
    }

    stop.store(true, Ordering::Relaxed);
    audio_thread.join().unwrap();
    metrics.audio_decoded = audio_decoded.load(Ordering::Relaxed);
    metrics.audio_errors = audio_errors.load(Ordering::Relaxed);
    metrics.max_audio_gap_ms = max_gap_ms.load(Ordering::Relaxed) as u128;
    metrics
}

#[test]
fn watched_streams_survive_hevc_idrs_loss_simulcast_switch_and_10bit_fallback_without_rejoin() {
    let fake = DualStreamServer::start();

    // Part A: Bob (HEVC 2nd IDR + packet loss + PLI recovery + simulcast SSRC switch 5001->5011)
    // watched concurrently alongside Carol (second independent stream) on a single session!
    let bob_server = fake.server_for("1400000000000000099", "sess-bob");
    let carol_server = fake.server_for("1400000000000000088", "sess-carol");

    let bob_handle =
        std::thread::spawn(move || watch_for_duration(bob_server, Duration::from_millis(12_300)));
    let carol_handle =
        std::thread::spawn(move || watch_for_duration(carol_server, Duration::from_millis(12_300)));

    let bob = bob_handle.join().unwrap();
    let carol = carol_handle.join().unwrap();

    eprintln!(
        "Bob (HEVC+loss+simulcast): p1={} p2={} p3={} last={} audio={} max_gap={}ms",
        bob.pictures_phase1,
        bob.pictures_phase2,
        bob.pictures_phase3,
        bob.pictures_after_rollovers,
        bob.audio_decoded,
        bob.max_audio_gap_ms
    );
    eprintln!(
        "Carol (2nd stream):        p1={} p2={} p3={} last={} audio={} max_gap={}ms",
        carol.pictures_phase1,
        carol.pictures_phase2,
        carol.pictures_phase3,
        carol.pictures_after_rollovers,
        carol.audio_decoded,
        carol.max_audio_gap_ms
    );

    assert!(bob.saw_h265, "Bob streamed in HEVC");
    assert!(bob.pictures_phase1 >= 15, "Bob decoded initial HEVC frames");
    assert!(
        bob.pictures_phase2 >= 25,
        "Bob kept advancing past 2nd HEVC IDR and packet-loss PLI recovery ({})",
        bob.pictures_phase2
    );
    assert!(
        bob.pictures_phase3 >= 30,
        "Bob kept advancing after simulcast SSRC switch 5001->5011 without rejoin ({})",
        bob.pictures_phase3
    );
    // The last unannounced layer is selected only after one second of silence
    // on its predecessor, leaving about 400 ms at 25 FPS in this window.
    assert!(
        bob.pictures_after_rollovers >= 6 && !bob.saw_h264 && carol.pictures_after_rollovers >= 20,
        "picture progression after five SSRC changes (Bob={}, Carol={})",
        bob.pictures_after_rollovers,
        carol.pictures_after_rollovers
    );
    assert!(
        bob.audio_decoded >= 220 && bob.audio_errors == 0 && bob.max_audio_gap_ms < 250,
        "Bob stream audio stayed uninterrupted (decoded={}, errors={}, max_gap={}ms)",
        bob.audio_decoded,
        bob.audio_errors,
        bob.max_audio_gap_ms
    );

    assert!(
        carol.pictures_phase1 >= 15 && carol.pictures_phase2 >= 35 && carol.pictures_phase3 >= 40,
        "Second concurrent stream (Carol) kept advancing throughout"
    );
    assert!(
        carol.audio_decoded >= 220 && carol.audio_errors == 0 && carol.max_audio_gap_ms < 250,
        "Second concurrent stream audio stayed uninterrupted"
    );

    // Part B: Dave (10-bit HEVC Main 10 HDR -> immediate in-place fallback to 8-bit H264)
    // watched concurrently alongside Carol2 without recreating StreamReceiver!
    let dave_server = fake.server_for("1400000000000000077", "sess-dave");
    let carol2_server = fake.server_for("1400000000000000088", "sess-carol2");

    let dave_handle =
        std::thread::spawn(move || watch_for_duration(dave_server, Duration::from_millis(3200)));
    let carol2_handle =
        std::thread::spawn(move || watch_for_duration(carol2_server, Duration::from_millis(3200)));

    let dave = dave_handle.join().unwrap();
    let carol2 = carol2_handle.join().unwrap();

    let dave_total = dave.pictures_phase1 + dave.pictures_phase2 + dave.pictures_phase3;
    let carol2_total = carol2.pictures_phase1 + carol2.pictures_phase2 + carol2.pictures_phase3;
    eprintln!(
        "Dave (10-bit HDR->H264):   total={} unsupported={} h264={} audio={} max_gap={}ms",
        dave_total,
        dave.saw_unsupported_format,
        dave.saw_h264,
        dave.audio_decoded,
        dave.max_audio_gap_ms
    );

    assert!(
        dave.saw_unsupported_format,
        "Dave rejected 10-bit HEVC Main 10 frame"
    );
    assert!(
        dave.saw_h264 && dave_total >= 45,
        "Dave fell back to 8-bit H264 in-place and kept advancing ({dave_total} pictures)"
    );
    assert!(
        dave.audio_decoded >= 110 && dave.audio_errors == 0 && dave.max_audio_gap_ms < 350,
        "Dave audio continued across H264 fallback (decoded={}, max_gap={}ms)",
        dave.audio_decoded,
        dave.max_audio_gap_ms
    );
    assert!(
        carol2_total >= 55 && carol2.audio_decoded >= 110,
        "Second stream worked alongside 10-bit fallback"
    );
}
