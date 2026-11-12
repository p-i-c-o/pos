#!/usr/bin/env python3
"""Test the real POS server process using only temporary local state."""
import http.client
import json
import os
import re
import select
import signal
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

if len(sys.argv) != 2:
    raise SystemExit("Usage: python3 scripts/server-smoke.py PATH_TO_POS_SERVER")

binary = str(Path(sys.argv[1]).resolve())
if not Path(binary).is_file():
    raise SystemExit(f"Missing server executable: {binary}")


def check(condition, message):
    if not condition:
        raise AssertionError(message)


with tempfile.TemporaryDirectory(prefix="pos-server-smoke-") as temp:
    root = Path(temp)
    home = root / "home"
    home.mkdir()
    env = os.environ.copy()
    env["HOME"] = str(home)
    env["XDG_DATA_HOME"] = str(root / "xdg")
    env["RUST_LOG"] = "pos_server=info"
    env.pop("POS_DB_PATH", None)
    env.pop("POS_SERVER_LISTEN", None)

    def expect_failure(arguments, *, environment=None):
        result = subprocess.run(
            [binary, *arguments],
            capture_output=True,
            text=True,
            env=environment or env,
            timeout=5,
            check=False,
        )
        check(result.returncode != 0, f"Unexpected success: {arguments}")
        return result.stdout + result.stderr

    for address in ("0.0.0.0:0", "[::]:0", "192.168.0.10:0", "[::ffff:127.0.0.1]:0"):
        output = expect_failure(["--listen", address])
        check("refusing non-loopback" in output, f"Unsafe address failure: {address}: {output}")

    output = expect_failure(["--listen", "not-an-address"])
    check("invalid" in output.lower(), f"Bad address error: {output}")

    bad_env = dict(env, POS_SERVER_LISTEN="0.0.0.0:0")
    output = expect_failure([], environment=bad_env)
    check("refusing non-loopback" in output, f"Unsafe env listen was not rejected: {output}")

    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as occupied:
        occupied.bind(("127.0.0.1", 0))
        occupied.listen()
        address = occupied.getsockname()
        output = expect_failure(["--listen", f"{address[0]}:{address[1]}"])
        check("startup failed" in output, "Occupied port did not report startup failure")

    def http_request(port, method, path):
        connection = http.client.HTTPConnection("127.0.0.1", port, timeout=3)
        try:
            connection.request(method, path)
            result = connection.getresponse()
            payload = result.read()
            return result.status, dict(result.getheaders()), payload
        finally:
            connection.close()

    def start_and_stop(shutdown_signal, environment):
        process = subprocess.Popen(
            [binary, "--listen", "127.0.0.1:0"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            env=environment,
        )
        output = b""
        port = None
        try:
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise AssertionError(f"Server exited during startup: {output!r}")
                ready, _, _ = select.select([process.stderr], [], [], 0.2)
                if ready:
                    chunk = os.read(process.stderr.fileno(), 65536)
                    output += chunk
                    match = re.search(rb"address=127\.0\.0\.1:(\d+)", output)
                    if match:
                        port = int(match.group(1))
                        break
            check(port is not None, f"No listening address logged: {output!r}")

            status, headers, body = http_request(port, "GET", "/health")
            check(status == 200, f"GET /health -> {status}")
            check(headers.get("cache-control") == "no-store", "Health cache setting missing")
            check(headers.get("content-type") == "application/json", "Health JSON type missing")
            data = json.loads(body)
            check(data["status"] == "ok", "Health status not ok")
            check(data["version"] == "0.1.0", "Unexpected POS version")
            check(data["api_version"] == "v1", "Unexpected API namespace version")
            check("service" in data, "No service name")

            status, _, body = http_request(port, "GET", "/api/v1/health")
            check(status == 200 and json.loads(body) == data, "Versioned health differs")
            status, _, body = http_request(port, "GET", "/api/v1/tasks")
            check(status == 404 and json.loads(body)["error"]["code"] == "not_found",
                  "Task API should be unavailable without auth")
            status, _, _ = http_request(port, "POST", "/api/v1/health")
            check(status == 405, "Non-GET health request not rejected")

            process.send_signal(shutdown_signal)
            process.wait(timeout=5)
            check(process.returncode == 0, f"Signal shutdown exit code: {process.returncode}")
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
                probe.settimeout(1)
                check(probe.connect_ex(("127.0.0.1", port)) != 0,
                      "Listener still open after shutdown")
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)
            if process.stderr:
                process.stderr.close()

    # Command line should take precedence over a valid or invalid env value.
    start_and_stop(signal.SIGTERM, dict(env, POS_SERVER_LISTEN="0.0.0.0:0"))
    start_and_stop(signal.SIGINT, env)

    check(not (root / "xdg" / "pos").exists(),
          "Health-only server unexpectedly created POS storage")
    print("POS server smoke: PASS (loopback, config, health API, errors, signals, no data)")
