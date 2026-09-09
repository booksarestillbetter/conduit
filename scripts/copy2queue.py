#!/usr/bin/env python3
"""
Conduit Transmission Post-Download Hook (copy2queue.py)
---------------------------------------------------
Executed by Transmission's `script-torrent-done-filename` or invoked via CLI.

Environment variables passed by Transmission:
  TR_TORRENT_ID      - Torrent ID
  TR_TORRENT_HASH    - Info Hash
  TR_TORRENT_NAME    - Torrent Name
  TR_TORRENT_DIR     - Download directory

Usage via CLI:
  python3 copy2queue.py --id 123 --hash abcdef... --name "Show.S01E01.mkv" --dir "/media/downloads"
"""

import os
import sys
import json
import argparse
import subprocess
import urllib.request
import urllib.error

CONDUIT_API_URL = os.environ.get("CONDUIT_API_URL", "http://127.0.0.1:4242")
CONDUIT_API_TOKEN = os.environ.get("CONDUIT_API_TOKEN", "")
CACHE_FILE = os.environ.get("CONDUIT_CACHE_FILE", "/tmp/conduit_queue_routes.cache.json")
LOG_FILE = os.environ.get("CONDUIT_HOOK_LOG", "/tmp/conduit_copy2queue.log")


def log(msg: str):
    line = f"[copy2queue] {msg}"
    print(line)
    try:
        with open(LOG_FILE, "a", encoding="utf-8") as f:
            f.write(line + "\n")
    except Exception:
        pass


def http_post(endpoint: str, payload: dict, timeout=5) -> dict:
    url = f"{CONDUIT_API_URL.rstrip('/')}/{endpoint.lstrip('/')}"
    headers = {"Content-Type": "application/json"}
    if CONDUIT_API_TOKEN:
        headers["Authorization"] = f"Bearer {CONDUIT_API_TOKEN}"

    data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(url, data=data, headers=headers, method="POST")
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def classify_file(name: str, info_hash: str, directory: str, node: str = "", tracker: str = "") -> dict:
    """Hits Conduit's /api/sync/classify with local cache fallback."""
    payload = {
        "name": name,
        "hash": info_hash,
        "dir": directory,
        "node": node,
        "tracker": tracker,
    }

    try:
        res = http_post("/api/sync/classify", payload, timeout=5)
        # Update local cache
        try:
            with open(CACHE_FILE, "w", encoding="utf-8") as f:
                json.dump(res, f)
        except Exception:
            pass
        return res
    except Exception as e:
        log(f"Warning: Failed to reach Conduit API ({e}), attempting fallback cache...")
        if os.path.exists(CACHE_FILE):
            try:
                with open(CACHE_FILE, "r", encoding="utf-8") as f:
                    return json.load(f)
            except Exception:
                pass

        # Built-in heuristic fallback
        final_queue = "tv" if any(s in name for s in ["S0", "S1", "S2", "E0", "E1", "E2"]) else "movie"
        if "2160p" in name or "UHD" in name or "4K" in name:
            final_queue += "UHD"
        return {
            "media_type": final_queue,
            "queue": final_queue,
            "target_dir": f"/media/queue/{final_queue}Queue/",
            "post_cmd": "cp -alv",
            "notify": True,
            "match_source": "offline_fallback",
            "decision_trace": ["Offline heuristic fallback applied"],
        }


def main():
    parser = argparse.ArgumentParser(description="Conduit Transmission Queue Staging Hook")
    parser.add_argument("--id", type=int, default=os.environ.get("TR_TORRENT_ID"))
    parser.add_argument("--hash", type=str, default=os.environ.get("TR_TORRENT_HASH"))
    parser.add_argument("--name", type=str, default=os.environ.get("TR_TORRENT_NAME"))
    parser.add_argument("--dir", type=str, default=os.environ.get("TR_TORRENT_DIR"))
    parser.add_argument("--node", type=str, default=os.environ.get("CONDUIT_NODE_NAME") or os.environ.get("TR_NODE_NAME"))
    parser.add_argument("--tracker", type=str, default=os.environ.get("TR_TORRENT_TRACKERS") or os.environ.get("TR_TORRENT_TRACKER") or "")
    args = parser.parse_args()

    torrent_id = args.id
    torrent_hash = args.hash or ""
    torrent_name = args.name or ""
    torrent_dir = args.dir or ""
    node_name = args.node or "Transmission"
    tracker_info = args.tracker or ""

    if not torrent_name:
        log("Error: No torrent name provided (via --name or TR_TORRENT_NAME).")
        sys.exit(1)

    source_path = os.path.join(torrent_dir, torrent_name) if torrent_dir else torrent_name
    log(f"Processing finished torrent on [{node_name}]: '{torrent_name}' (Hash: {torrent_hash})")

    # 1. Classify file destination via Conduit 4-tier API
    classification = classify_file(torrent_name, torrent_hash, torrent_dir, node=node_name, tracker=tracker_info)
    target_dir = classification.get("target_dir", "/media/queue/miscQueue/")
    post_cmd = classification.get("post_cmd", "cp -alv")
    queue_name = classification.get("queue", "misc")
    match_source = classification.get("match_source", "unknown")
    trace = classification.get("decision_trace", [])

    log(f"Classified '{torrent_name}' -> Queue '{queue_name}' ({target_dir}) [Source: {match_source}]")
    for step in trace:
        log(f"  -> {step}")

    # 2. Ensure target queue directory exists
    os.makedirs(target_dir, exist_ok=True)

    # 3. Execute staging hardlink/copy
    import shlex
    cmd_parts = shlex.split(post_cmd) if post_cmd else ["cp", "-alv"]
    exec_args = cmd_parts + [source_path, target_dir]
    log(f"Running staging command: {' '.join(shlex.quote(a) for a in exec_args)}")
    try:
        result = subprocess.run(exec_args, shell=False, capture_output=True, text=True)
        if result.returncode == 0:
            log(f"Successfully staged payload to {target_dir}")
        else:
            log(f"Warning: Staging command exited with code {result.returncode}: {result.stderr.strip()}")
    except Exception as ex:
        log(f"Error executing command: {ex}")

    # 4. Notify Conduit of completed download & staging
    try:
        notify_payload = {
            "hash": torrent_hash,
            "name": torrent_name,
            "node": node_name,
            "path": source_path,
            "queue": queue_name,
            "target_dir": target_dir,
        }
        http_post("/api/sync/notify-download", notify_payload, timeout=5)
        log("Sent download & staging confirmation to Conduit API.")
    except Exception as e:
        log(f"Warning: Could not send notify-download to Conduit: {e}")


if __name__ == "__main__":
    main()
