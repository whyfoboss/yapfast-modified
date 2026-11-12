"""Compare native Yapfast stream workers against a loopback Fake Discord."""
import argparse
import ctypes
from ctypes import wintypes
import json
import mmap
import os
from pathlib import Path
import re
import socket
import statistics
import struct
import subprocess
import sys
import time
import urllib.request

ALICE = "1400000000000000001"
ENV_ID = 1450000000000000000
MIB = 1024 * 1024
ROOT = Path(__file__).resolve().parents[1]


class Counters(ctypes.Structure):
    _fields_ = [("cb", wintypes.DWORD), ("faults", wintypes.DWORD)] + [
        (name, ctypes.c_size_t)
        for name in (
            "peak_working", "working", "peak_paged", "paged", "peak_nonpaged",
            "nonpaged", "pagefile", "peak_pagefile", "private", "private_working", "shared_commit",
        )
    ]


def memory(process):
    info = Counters()
    info.cb = ctypes.sizeof(info)
    read = ctypes.windll.kernel32.K32GetProcessMemoryInfo
    read.argtypes = (wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD)
    if not read(int(process._handle), ctypes.byref(info), info.cb):
        raise ctypes.WinError()
    return [info.private_working / MIB, info.private / MIB]


def post(url, data=b""):
    with urllib.request.urlopen(urllib.request.Request(url, data=data), timeout=2):
        pass


def video_stats(origin):
    with urllib.request.urlopen(origin + "/_control/video", timeout=2) as response:
        data = json.load(response)
    return {
        "received": [{k: s[k] for k in ("codec", "frames", "width", "height")}
                     for s in data["received"] if s["frames"]],
        "watchers": [{k: s[k] for k in ("framesSent", "soundsSent", "keyframeRequests")}
                     for s in data["watchers"]],
    }


def test_window():
    """A controlled window to capture, with no desktop or user data."""
    import tkinter

    ctypes.windll.user32.SetProcessDPIAware()
    app = tkinter.Tk()
    app.title("Yapfast stream benchmark")
    app.geometry("1920x1080+0+0")
    app.overrideredirect(True)
    app.attributes("-topmost", True)
    canvas = tkinter.Canvas(app, highlightthickness=0)
    canvas.pack(fill="both", expand=True)
    app.update()
    hwnd = ctypes.windll.user32.GetAncestor(app.winfo_id(), 2)
    print(hwnd, flush=True)
    bars = [canvas.create_rectangle(0, 0, 0, 0, outline="", fill=color)
            for color in ("#003399", "#77bb33", "#ffee11", "#df4433", "#ccccdd", "#7654bc", "#141414")]

    def draw():
        phase = int(time.monotonic() * 180) % 1920
        for i, bar in enumerate(bars):
            x = (phase + i * 280) % 1920
            canvas.coords(bar, x, 0, x + 280, 1080)
        app.after(16, draw)

    draw()
    app.mainloop()


def server(port, guild, channel, user):
    key = f"guild:{guild}:{channel}:{user}"
    return {
        "stream_key": key, "user_id": ALICE, "session_id": "bench-voice",
        "rtc_server_id": guild, "token": f"stream-token-{key}",
        "endpoint": f"ws://127.0.0.1:{port}/voice",
    }


class Worker:
    def __init__(self, exe, directory, name, job, size, port, outgoing=None, packed=False):
        self.name = name
        self.log = directory / f"{name}.log"
        self.reports = directory / f"{name}.jsonl"
        path = directory / f"{name}.frames"
        capacity = size[0] * size[1] * (3 if packed else 8) // 2
        self.mapping_bytes = 64 + 3 * capacity
        with path.open("w+b") as f:
            f.write(struct.pack("<4sIII", b"TKFR", capacity, 1, int(packed)))
            f.truncate(self.mapping_bytes)
            self.map = mmap.mmap(f.fileno(), 0)
        job["frames"] = str(path)
        env = dict(os.environ, YAPFAST_DISCORD_TEST_SERVER=f"http://127.0.0.1:{port}",
                   YAPFAST_STREAM_LOG="1")
        with self.reports.open("wb") as reports, self.log.open("wb") as log:
            self.process = subprocess.Popen(
                [str(exe), "--stream-worker"], env=env, stdin=subprocess.PIPE,
                stdout=reports, stderr=log, creationflags=subprocess.CREATE_NO_WINDOW,
            )
        self.send({name.split("-")[0]: job})
        if outgoing:
            self.send({"send": outgoing})

    def send(self, message):
        self.process.stdin.write((json.dumps(message) + "\n").encode())
        self.process.stdin.flush()

    def dimensions(self):
        middle, = struct.unpack_from("<I", self.map, 8)
        return struct.unpack_from("<II", self.map, 16 + (middle & 3) * 8)

    def close(self):
        if self.process.poll() is None:
            self.process.stdin.close()
            try:
                self.process.wait(5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        self.map.close()

    def counters(self):
        if self.process.poll() is not None:
            raise RuntimeError(f"{self.name} exited; see {self.reports}")
        return memory(self.process)

    def published(self):
        return [
            (int(at), int(count))
            for at, count in re.findall(r"elapsed_ms=(\d+) input=\d+ published=(\d+)",
                                       self.log.read_text(errors="replace"))
        ]


def run(args):
    args.output.mkdir(parents=True, exist_ok=True)
    folder = args.output
    clip = folder / "pattern.mp4"
    if not clip.exists():
        subprocess.run(
            ["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=1920x1080:rate=30",
             "-t", "3", "-c:v", "libx264", "-preset", "ultrafast", "-y", str(clip)],
            check=True,
        )
    people = [{"key": f"viewer{i}", "avatar": None} for i in range(3)]
    fixture = folder / "fake.json"
    fixture.write_text(json.dumps({
        "me": {"avatar": None}, "people": people,
        "servers": [{"name": "Benchmark", "channels": [{"name": "Bench", "type": "voice",
                      "people": [{"who": person["key"], "stream": "pattern.mp4"} for person in people]}]}],
    }))
    with socket.socket() as candidate:
        candidate.bind(("127.0.0.1", 0))
        port = candidate.getsockname()[1]
    origin = f"http://127.0.0.1:{port}"
    guild, channel = str(ENV_ID + 3), str(ENV_ID + 4)
    with (folder / "fake.log").open("wb") as log:
        fake = subprocess.Popen(
            ["bun", str(ROOT / "testing/fake_discord.ts"), "--port", str(port), "--env", str(fixture)],
            stdout=log, stderr=log, creationflags=subprocess.CREATE_NO_WINDOW,
        )
    workers = []
    idle = None
    window = None
    try:
        for _ in range(300):
            if fake.poll() is not None:
                raise RuntimeError("Fake Discord exited")
            try:
                post(origin + "/_control/stream-codec", args.codec.encode())
                break
            except OSError:
                time.sleep(0.2)
        else:
            raise RuntimeError("Fake Discord did not start")
        if not args.skip_idle:
            idle = subprocess.Popen(
                [str(args.exe), "--preview", "--data-dir", str(folder / "preview")],
                env=dict(os.environ, YAPFAST_DISCORD_TEST_SERVER=origin),
            )
            time.sleep(12)
            print("idle preview private working set / private commit MiB:", memory(idle), flush=True)
        source = "test_pattern"
        if args.capture_window:
            window = subprocess.Popen([sys.executable, __file__, "--test-window"],
                                      stdout=subprocess.PIPE, creationflags=subprocess.CREATE_NO_WINDOW)
            source = {"window": int(window.stdout.readline())}
        share = Worker(args.exe, folder, "share", {
            "source": source, "width": 1920, "height": 1080, "fps": 30, "hardware": True,
        }, (1280, 720), port, server(port, guild, channel, ALICE), packed=args.packed)
        workers.append(share)

        results = {}
        def measure(label, seconds):
            initial = video_stats(origin)
            samples = []
            for _ in range(seconds):
                time.sleep(1)
                samples.append([worker.counters() for worker in workers])
            selected = samples[len(samples) // 2:]
            totals = [[sum(values[k] for values in sample) for k in range(2)] for sample in selected]
            result = {
                "private_working_mib": round(statistics.median(row[0] for row in totals), 2),
                "private_commit_mib": round(statistics.median(row[1] for row in totals), 2),
                "peak_private_working_mib": round(max(row[0] for row in totals), 2),
                "mapped_capacity_mib": round(sum(w.mapping_bytes for w in workers) / MIB, 2),
                "workers": {worker.name: {
                    "size": worker.dimensions(),
                    "memory": [round(statistics.median(sample[i][k] for sample in selected), 2) for k in range(2)],
                } for i, worker in enumerate(workers)},
                "samples": samples,
                "video_start": initial,
                "video_end": video_stats(origin),
            }
            results[label] = result
            print(label, {k: v for k, v in result.items() if k not in ("samples", "video_start", "video_end")}, flush=True)

        measure("share", args.seconds)
        for i in range(3):
            watch = Worker(args.exe, folder, f"watch-{i}", {
                "server": server(port, guild, channel, str(ENV_ID + i)),
                "width": 2560, "height": 1440, "deafened": False, "volume": 0,
            }, (2560, 1440), port, packed=args.packed)
            workers.append(watch)
            if i == 0:
                measure("share_plus_1", args.seconds)
        measure("share_plus_3", args.seconds)
        expected = (1920, 1080) if args.codec == "h264" else (320, 180)
        minimum_frames = 100 if args.codec == "h264" else 35
        for worker in workers[1:]:
            assert worker.dimensions() == expected, f"{worker.name} received {worker.dimensions()}"
        if args.freeze:
            before = {worker.name: worker.published()[-1][0] for worker in workers[1:]}
            layer = "announceLow" if args.low_layer else "announceInactive"
            post(origin + "/_control/stream-video?" + layer)
            measure("after_inactive_layer", args.recovery_seconds or args.seconds)
            if args.require_recovery:
                for worker in workers[1:]:
                    after = [sample for sample in worker.published() if sample[0] > before[worker.name] + 5000]
                    assert len(after) >= 2 and all(
                        new[1] - old[1] >= minimum_frames for old, new in zip(after, after[1:])
                    ), f"{worker.name}: video stopped advancing after the layer update"
                stats = results["after_inactive_layer"]
                assert all(
                    new["soundsSent"] > old["soundsSent"]
                    for old, new in zip(stats["video_start"]["watchers"], stats["video_end"]["watchers"])
                ), "audio stopped advancing"
        if args.check_resume:
            post(origin + "/_control/stream-video?paused=1")
            measure("audio_only", 6)
            post(origin + "/_control/stream-video")
            measure("video_resumed", 12)
            for worker in workers[1:]:
                counts = worker.published()
                assert counts[-1][1] - counts[-2][1] >= minimum_frames, f"{worker.name}: video did not resume"
        results["published_over_time"] = {worker.name: worker.published() for worker in workers}
        if idle:
            results["preview_with_fake"] = memory(idle)
        (folder / "measurements.json").write_text(json.dumps(results, indent=2))
    finally:
        for worker in reversed(workers):
            worker.close()
        for process in (window, idle, fake):
            if process and process.poll() is None:
                process.terminate()
                process.wait(timeout=5)


if __name__ == "__main__":
    if sys.argv[1:] == ["--test-window"]:
        test_window()
        sys.exit()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("exe", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--seconds", type=int, default=20)
    parser.add_argument("--skip-idle", action="store_true")
    parser.add_argument("--packed", action="store_true", help="use packed I420 with a supporting executable")
    parser.add_argument("--freeze", action="store_true", help="announce an inactive layer after warmup")
    parser.add_argument("--recovery-seconds", type=int, help="how long to run after announcing an inactive layer")
    parser.add_argument("--require-recovery", action="store_true", help="fail if video stalls after announcing a layer")
    parser.add_argument("--low-layer", action="store_true", help="announce a lower quality layer as the default during --freeze")
    parser.add_argument("--check-resume", action="store_true", help="pause video while audio continues, then resume")
    parser.add_argument("--capture-window", action="store_true", help="share a generated 1080p desktop window")
    parser.add_argument("--codec", choices=("h264", "av1"), default="h264",
                        help="H264 uses the generated 1080p clip; AV1 uses the bundled 320x180 fixture")
    run(parser.parse_args())

