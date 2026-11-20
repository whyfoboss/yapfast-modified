import { createCipheriv } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";

const FIXTURES = join(dirname(import.meta.path), "fixtures");
const PLAYOUT_DELAY = Buffer.from([0xbe, 0xde, 0x00, 0x01, 0x52, 0x00, 0x00, 0x00]);
const OPUS_SILENCE = Buffer.from([0xf8, 0xff, 0xfe]);

function annexB(file: Buffer, hevc = false): { units: Buffer[]; keyframe: boolean }[] {
  const starts: number[] = [];
  for (let at = 0; at + 3 <= file.length; at++) {
    if (file[at] === 0 && file[at + 1] === 0 && file[at + 2] === 1) starts.push(at + 3);
  }
  const units = starts.map((start, index) => {
    let end = index + 1 < starts.length ? starts[index + 1]! - 3 : file.length;
    while (end > start && file[end - 1] === 0) end--;
    return file.subarray(start, end);
  });
  const frames: { units: Buffer[]; keyframe: boolean }[] = [];
  const [delimiter, isKey] = hevc
    ? [35, (type: number) => type >= 16 && type <= 21]
    : [9, (type: number) => type === 5];
  for (const unit of units) {
    if (unit.length === 0) continue;
    const type = hevc ? (unit[0]! >> 1) & 0x3f : unit[0]! & 0x1f;
    if (type === delimiter || frames.length === 0) frames.push({ units: [], keyframe: false });
    if (type === delimiter) continue;
    frames.at(-1)!.units.push(unit);
    if (isKey(type)) frames.at(-1)!.keyframe = true;
  }
  return frames.filter((f) => f.units.length > 0);
}

const rawHevc = Buffer.from(readFileSync(join(FIXTURES, "stream-h265.h265")));
for (let at = 0; at + 32 < rawHevc.length; at++) {
  if (
    rawHevc[at] === 0 &&
    rawHevc[at + 1] === 0 &&
    rawHevc[at + 2] === 1 &&
    ((rawHevc[at + 3]! >> 1) & 0x3f) === 33 &&
    rawHevc[at + 28] === 0xa9
  ) {
    // Patch sps_max_num_reorder_pics[0] = 2 to reproduce the FFmpeg HEVC output_fifo wedge.
    rawHevc[at + 28] = 0x79;
  }
}

const CLIPS = {
  hevcReorder2: annexB(rawHevc, true),
  hevc10Bit: annexB(readFileSync(join(FIXTURES, "stream-h265-10bit.h265")), true),
  h264: annexB(readFileSync(join(FIXTURES, "stream-h264.h264")), false),
};

function sealRtp(packet: Buffer, key: Buffer, counter: number, clear = 16): Buffer {
  const nonce = Buffer.alloc(12);
  nonce.writeUInt32BE(counter >>> 0, 0);
  const cipher = createCipheriv("aes-256-gcm", key, nonce);
  cipher.setAAD(packet.subarray(0, clear));
  const sealed = Buffer.concat([cipher.update(packet.subarray(clear)), cipher.final()]);
  return Buffer.concat([
    packet.subarray(0, clear),
    sealed,
    cipher.getAuthTag(),
    nonce.subarray(0, 4),
  ]);
}

const BOB_STREAM = "guild:100:200:1400000000000000099";
const CAROL_STREAM = "guild:100:200:1400000000000000088";
const DAVE_STREAM = "guild:100:200:1400000000000000077";

const peers = new Map<number, { address: string; port: number }>();
const sessions = new Map<number, any>();
let nextViewerSsrc = 1000;

const udp = await Bun.udpSocket({
  hostname: "127.0.0.1",
  port: 0,
  socket: {
    data(socket, data, port, address) {
      if (data.length === 74 && data.readUInt16BE(0) === 1) {
        const ssrc = data.readUInt32BE(4);
        peers.set(ssrc, { address, port });
        const answer = Buffer.alloc(74);
        answer.writeUInt16BE(2, 0);
        answer.writeUInt16BE(70, 2);
        answer.writeUInt32BE(ssrc, 4);
        answer.write(address, 8, "ascii");
        answer.writeUInt16BE(port, 72);
        socket.send(answer, port, address);
        return;
      }
      if (data.length >= 12 && data[1] === 206 && (data[0]! & 0x1f) === 1) {
        const sess = sessions.get(data.readUInt32BE(4));
        if (sess) {
          sess.plisReceived++;
          sess.forceKeyNext = true;
        }
      }
    },
  },
});

const server = Bun.serve<{ ssrc?: number; streamKey?: string; sess?: any }>({
  hostname: "127.0.0.1",
  port: 0,
  fetch(req, srv) {
    if (srv.upgrade(req, { data: {} })) return;
    return Response.json({ ok: true });
  },
  websocket: {
    open(ws) {
      ws.send(JSON.stringify({ op: 8, d: { heartbeat_interval: 10000 } }));
    },
    message(ws, raw) {
      const msg = JSON.parse(String(raw));
      const d = msg.d ?? {};
      const reply = (op: number, payload: unknown) => ws.send(JSON.stringify({ op, d: payload }));

      if (msg.op === 0) {
        const ssrc = (nextViewerSsrc += 10);
        ws.data.ssrc = ssrc;
        ws.data.streamKey = String(d.token ?? "").replace(/^stream-token-/, "");
        reply(2, {
          ssrc,
          ip: "127.0.0.1",
          port: udp.port,
          modes: ["aead_aes256_gcm_rtpsize"],
        });
      } else if (msg.op === 1) {
        const streamKey = ws.data.streamKey!;
        const codecs = (d.codecs ?? []) as { name: string; decode?: boolean }[];
        const decodesH265 = codecs.some(
          (c) => c.name.toUpperCase() === "H265" && c.decode !== false,
        );

        let chosenCodec: "H265" | "H264" = "H264";
        let clip = CLIPS.h264;
        let baseSsrc = 6000;
        let userId = "1400000000000000088";

        if (streamKey === BOB_STREAM) {
          baseSsrc = 5000;
          userId = "1400000000000000099";
          chosenCodec = decodesH265 ? "H265" : "H264";
          clip = chosenCodec === "H265" ? CLIPS.hevcReorder2 : CLIPS.h264;
        } else if (streamKey === DAVE_STREAM) {
          baseSsrc = 7000;
          userId = "1400000000000000077";
          chosenCodec = decodesH265 ? "H265" : "H264";
          clip = chosenCodec === "H265" ? CLIPS.hevc10Bit : CLIPS.h264;
        }

        const key = Buffer.from(crypto.getRandomValues(new Uint8Array(32)));
        const sess = {
          viewerSsrc: ws.data.ssrc!,
          streamKey,
          chosenCodec,
          key,
          clip,
          baseSsrc,
          plisReceived: 0,
          forceKeyNext: false,
          idr2Sent: false,
          lossInjected: false,
          switchedSimulcast: false,
          videoTimer: null as any,
          audioTimer: null as any,
        };
        sessions.set(sess.viewerSsrc, sess);
        ws.data.sess = sess;

        reply(4, {
          mode: "aead_aes256_gcm_rtpsize",
          secret_key: [...key],
          dave_protocol_version: 0,
          audio_codec: "opus",
          video_codec: chosenCodec,
        });

        const streams =
          streamKey === BOB_STREAM
            ? [
                {
                  type: "video",
                  rid: "100",
                  ssrc: baseSsrc + 1,
                  rtx_ssrc: baseSsrc + 2,
                  active: true,
                  quality: 100,
                },
                {
                  type: "video",
                  rid: "50",
                  ssrc: baseSsrc + 11,
                  rtx_ssrc: baseSsrc + 12,
                  active: true,
                  quality: 50,
                },
              ]
            : [
                {
                  type: "video",
                  rid: "100",
                  ssrc: baseSsrc + 1,
                  rtx_ssrc: baseSsrc + 2,
                  active: true,
                  quality: 100,
                },
              ];

        // Also send a stray op 12 for an unrelated user_id to verify Streamer ignores it.
        reply(12, {
          user_id: "1499999999999999999",
          audio_ssrc: 9900,
          video_ssrc: 9901,
          rtx_ssrc: 9902,
        });
        reply(12, {
          user_id: userId,
          audio_ssrc: baseSsrc,
          video_ssrc: baseSsrc + 1,
          rtx_ssrc: baseSsrc + 2,
          streams,
        });
      } else if (msg.op === 15) {
        const sess = ws.data.sess!;
        if (!sess.videoTimer) {
          let frameStep = 0;
          let clipCursor = 0;
          let seq = 1000;
          let soundSeq = streamKeySeed(sess.streamKey, sess.chosenCodec);
          let counter = 1;
          const startedAt = Date.now();

          const sendMedia = (
            payload: Buffer,
            marker: boolean,
            ts: number,
            pt: number,
            ssrc: number,
            drop = false,
          ) => {
            const thisSeq = seq++ & 0xffff;
            if (drop) return;
            const header = Buffer.alloc(12);
            header[0] = 0x90;
            header[1] = (marker ? 0x80 : 0) | pt;
            header.writeUInt16BE(thisSeq, 2);
            header.writeUInt32BE(ts >>> 0, 4);
            header.writeUInt32BE(ssrc >>> 0, 8);
            const peer = peers.get(sess.viewerSsrc);
            if (peer) {
              try {
                udp.send(
                  sealRtp(Buffer.concat([header, PLAYOUT_DELAY, payload]), sess.key, counter++),
                  peer.port,
                  peer.address,
                );
              } catch {}
            }
          };

          sess.audioTimer = setInterval(() => {
            const peer = peers.get(sess.viewerSsrc);
            if (!peer) return;
            const ah = Buffer.alloc(12);
            ah[0] = 0x80;
            ah[1] = 120;
            ah.writeUInt16BE(soundSeq & 0xffff, 2);
            ah.writeUInt32BE((soundSeq * 960) >>> 0, 4);
            ah.writeUInt32BE(sess.baseSsrc, 8);
            soundSeq++;
            try {
              udp.send(
                sealRtp(Buffer.concat([ah, OPUS_SILENCE]), sess.key, counter++, 12),
                peer.port,
                peer.address,
              );
            } catch {}
          }, 20);

          sess.videoTimer = setInterval(() => {
            const elapsed = Date.now() - startedAt;
            let ssrc = sess.baseSsrc + 1;
            let dropPacket = false;

            if (sess.streamKey === BOB_STREAM) {
              // Fault 1 (1200ms): second HEVC IDR with sps_max_num_reorder_pics=2 (SF-01)
              if (!sess.idr2Sent && elapsed >= 1200) {
                sess.idr2Sent = true;
                clipCursor = 0;
              }
              // Fault 2 (2200ms): packet loss followed by RTCP PLI recovery IDR (SF-01)
              if (!sess.lossInjected && elapsed >= 2200) {
                sess.lossInjected = true;
                dropPacket = true;
              }
              // Fault 3 (3400ms): SFU switches RTP forwarding from 5001 to simulcast layer 5011 (SF-02)
              if (elapsed >= 3400) {
                ssrc = sess.baseSsrc + 11 + Math.floor((elapsed - 3400) / 1500);
                if (!sess.switchedSimulcast) {
                  sess.switchedSimulcast = true;
                  clipCursor = 0;
                }
              }
            }

            if (sess.forceKeyNext) {
              sess.forceKeyNext = false;
              clipCursor = 0;
            }

            const sourceFrame = sess.clip[clipCursor % sess.clip.length]!;
            clipCursor++;
            const ts = frameStep * 3600;
            frameStep++;

            let dropped = false;
            const maybeDrop = () => {
              if (dropPacket && !dropped) {
                dropped = true;
                return true;
              }
              return false;
            };

            const pt = sess.chosenCodec === "H265" ? 103 : 101;
            sourceFrame.units.forEach((unit: Buffer, idx: number) => {
              const last = idx === sourceFrame.units.length - 1;
              if (unit.length <= 1100) {
                sendMedia(unit, last, ts, pt, ssrc, maybeDrop());
                return;
              }
              if (sess.chosenCodec === "H265") {
                const nalType = (unit[0]! >> 1) & 0x3f;
                for (let at = 2; at < unit.length; at += 1100) {
                  const start = at === 2 ? 0x80 : 0;
                  const end = at + 1100 >= unit.length ? 0x40 : 0;
                  const hdr = Buffer.from([
                    (unit[0]! & 0x81) | (49 << 1),
                    unit[1]!,
                    start | end | nalType,
                  ]);
                  sendMedia(
                    Buffer.concat([hdr, unit.subarray(at, at + 1100)]),
                    last && end !== 0,
                    ts,
                    pt,
                    ssrc,
                    maybeDrop(),
                  );
                }
              } else {
                for (let at = 1; at < unit.length; at += 1100) {
                  const start = at === 1 ? 0x80 : 0;
                  const end = at + 1100 >= unit.length ? 0x40 : 0;
                  const hdr = Buffer.from([
                    (unit[0]! & 0xe0) | 28,
                    start | end | (unit[0]! & 0x1f),
                  ]);
                  sendMedia(
                    Buffer.concat([hdr, unit.subarray(at, at + 1100)]),
                    last && end !== 0,
                    ts,
                    pt,
                    ssrc,
                    maybeDrop(),
                  );
                }
              }
            });
          }, 40); // 25 fps
        }
      } else if (msg.op === 3) {
        reply(6, d);
      }
    },
    close(ws) {
      if (ws.data.sess?.videoTimer) clearInterval(ws.data.sess.videoTimer);
      if (ws.data.sess?.audioTimer) clearInterval(ws.data.sess.audioTimer);
    },
  },
});

function streamKeySeed(streamKey: string, codec: string): number {
  // Deliberately jump RTP sequence backwards by >10000 on Dave's H264 fallback session
  // to verify audio timeline continuity across SF-03 fallback reconnects.
  if (streamKey === DAVE_STREAM) {
    return codec === "H265" ? 30000 : 12000;
  }
  return 20000;
}

console.log(`LISTENING http://127.0.0.1:${server.port}`);
