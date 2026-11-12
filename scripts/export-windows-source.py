#!/usr/bin/env python3
"""Safe, non-destructive export of the current TASK 9 working tree to Windows.

Creates an archive or directory copy containing:
- All tracked repository files with their uncommitted modifications.
- All untracked TASK 9 source files (activity.rs, windows_smtc.rs, rpc/proto.rs, rpc/windows.rs).
- All necessary vendored crates and configuration files.

Explicitly EXCLUDES:
- `target/` and build caches
- `.git/` history and objects
- `.system_generated/` and temporary artifacts
- Build artifacts in `work/windows-verify/target/`

Usage:
    python3 scripts/export-windows-source.py [--zip OUTPUT.zip] [--dir OUTPUT_DIR] [--overwrite]
"""

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import zipfile

REPO_ROOT = Path(__file__).resolve().parents[1]

CRITICAL_TASK9_FILES = [
    "crates/yapfast-core/src/activity.rs",
    "crates/yapfast-core/src/activity/windows_smtc.rs",
    "crates/yapfast-discord/src/rpc/proto.rs",
    "crates/yapfast-discord/src/rpc/windows.rs",
    "crates/yapfast-discord/src/rpc/linux.rs",
    "src/ui/games.rs",
    "ui/settings.slint",
]

EXCLUDE_PATTERNS = [
    "/target/",
    "/.git/",
    "/.system_generated/",
    "/.local/",
    "/work/windows-verify/target/",
    "CACHEDIR.TAG",
    "__pycache__",
]

def is_excluded(path_str: str) -> bool:
    normalized = "/" + path_str.replace("\\", "/").strip("/") + "/"
    for pat in EXCLUDE_PATTERNS:
        if pat.endswith("/"):
            if pat in normalized or normalized.startswith(pat[1:]):
                return True
        elif pat in path_str:
            return True
    return False

def collect_source_files(root: Path):
    """Collects all source files that should be exported."""
    # 1. Start with git tracked files
    tracked_raw = subprocess.check_output(
        ["git", "ls-files"], cwd=root, text=True
    ).splitlines()

    # 2. Add untracked files
    untracked_raw = subprocess.check_output(
        ["git", "status", "--porcelain", "-uall"], cwd=root, text=True
    ).splitlines()

    untracked_files = []
    for line in untracked_raw:
        if line.startswith("?? "):
            rel_path = line[3:].strip()
            full_p = root / rel_path
            if full_p.is_file():
                untracked_files.append(rel_path)
            elif full_p.is_dir():
                for sub in full_p.rglob("*"):
                    if sub.is_file():
                        untracked_files.append(str(sub.relative_to(root)))

    all_rel_paths = set(tracked_raw + untracked_files)

    # Filter out excluded
    export_files = []
    for rel in sorted(all_rel_paths):
        rel_norm = rel.replace("\\", "/")
        if is_excluded(rel_norm):
            continue
        full_path = root / rel
        if full_path.is_file():
            export_files.append(rel)

    # Verify critical TASK 9 files exist in export list
    missing = [f for f in CRITICAL_TASK9_FILES if f not in export_files]
    if missing:
        print(f"ERROR: Missing critical TASK 9 files in export: {missing}", file=sys.stderr)
        sys.exit(1)

    return export_files

def export_zip(files, root: Path, out_zip: Path, overwrite: bool):
    if out_zip.exists() and not overwrite:
        print(f"ERROR: Target file '{out_zip}' already exists. Use --overwrite to replace it.", file=sys.stderr)
        sys.exit(1)

    out_zip.parent.mkdir(parents=True, exist_ok=True)
    print(f"Creating Windows export archive: {out_zip} ({len(files)} files)...")
    with zipfile.ZipFile(out_zip, "w", zipfile.ZIP_DEFLATED) as zf:
        for rel in files:
            full = root / rel
            zf.write(full, arcname=rel)

    size_mb = out_zip.stat().st_size / (1024 * 1024)
    print(f"SUCCESS: Exported {len(files)} files to '{out_zip}' ({size_mb:.2f} MB).")

def export_dir(files, root: Path, out_dir: Path, overwrite: bool):
    if out_dir.exists():
        if any(out_dir.iterdir()) and not overwrite:
            print(f"ERROR: Target directory '{out_dir}' exists and is not empty. Use --overwrite to continue.", file=sys.stderr)
            sys.exit(1)
    out_dir.mkdir(parents=True, exist_ok=True)

    print(f"Copying {len(files)} source files to '{out_dir}'...")
    count = 0
    for rel in files:
        src = root / rel
        dst = out_dir / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
        count += 1

    print(f"SUCCESS: Exported {count} files to '{out_dir}'.")

def main():
    parser = argparse.ArgumentParser(description="Export TASK 9 working tree for native Windows build.")
    parser.add_argument("--zip", type=Path, help="Path for output .zip archive (e.g. /mnt/c/yapfast-task9.zip)")
    parser.add_argument("--dir", type=Path, help="Directory for output copy (e.g. /mnt/c/yapfast-task9)")
    parser.add_argument("--overwrite", action="store_true", help="Allow overwriting existing output destination")

    args = parser.parse_args()

    if not args.zip and not args.dir:
        # Default to a safe zip archive in current directory if nothing specified
        args.zip = REPO_ROOT / "work/yapfast-task9-windows-export.zip"

    files = collect_source_files(REPO_ROOT)
    print(f"Collected {len(files)} source files from TASK 9 working tree.")

    if args.zip:
        export_zip(files, REPO_ROOT, args.zip, args.overwrite)
    if args.dir:
        export_dir(files, REPO_ROOT, args.dir, args.overwrite)

if __name__ == "__main__":
    main()
