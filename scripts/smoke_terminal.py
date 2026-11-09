#!/usr/bin/env python3
"""Unix PTY smoke checks; uses only Python's standard library. Run from repo root."""
import errno
import fcntl
import json
import os
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import termios
import tempfile
import time
from pathlib import Path

# Keep the controlling session alive until the parent checks terminal settings.
# On macOS, inspecting the slave after its session leader exits returns ENOTTY.
SUPERVISOR = """
import fcntl, os, subprocess, sys, termios
os.setsid()
fcntl.ioctl(0, termios.TIOCSCTTY, 0)
status_fd, release_fd = map(int, sys.argv[1:3])
result = subprocess.call(sys.argv[3:])
os.write(status_fd, str(result).encode())
os.close(status_fd)
os.read(release_fd, 1)
"""


def visible_text(output):
    """Read the small CSI subset emitted by Ratatui, including differential redraws."""
    cells = {}
    x = y = 0
    data = output.decode(errors="replace")
    position = 0
    csi = re.compile(r"\x1b\[([0-9;?]*)([ -/]*)([@-~])")
    while position < len(data):
        match = csi.match(data, position)
        if match:
            raw, _, command = match.groups()
            values = [int(value or 0) for value in raw.lstrip("?").split(";")]
            first = values[0] or 1
            if command in "Hf":
                y, x = first - 1, (values[1] if len(values) > 1 else 1) - 1
            elif command == "G": x = first - 1
            elif command == "A": y = max(0, y - first)
            elif command == "B": y += first
            elif command == "C": x += first
            elif command == "D": x = max(0, x - first)
            elif command == "J" and values[0] in (2, 3): cells.clear()
            position = match.end()
            continue
        char = data[position]
        if char == "\r": x = 0
        elif char == "\n": y += 1
        elif char >= " ":
            cells[(y, x)] = char
            x += 1
        position += 1
    return "\n".join("".join(cells.get((row, col), " ") for col in range(200)) for row in range(80))


def run_pty(command, keys=None, resize=False, expected=0, env=None, steps=None, size=(100, 30)):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", size[1], size[0], 0, 0))
    before = termios.tcgetattr(slave)

    status_read, status_write = os.pipe()
    release_read, release_write = os.pipe()
    child = subprocess.Popen(
        [sys.executable, "-c", SUPERVISOR, str(status_write), str(release_read), *command],
        stdin=slave, stdout=slave, stderr=slave, pass_fds=(status_write, release_read), env=env)
    os.close(status_write)
    os.close(release_read)
    output = bytearray()
    deadline = time.monotonic() + 30
    pending = list(steps if steps is not None else ([(b"Inventory", keys)] if keys is not None else []))
    search_start = 0
    try:
        while time.monotonic() < deadline:
            if select.select([master], [], [], 0.05)[0]:
                try:
                    output.extend(os.read(master, 65536))
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
            if pending and len(output) > search_start and pending[0][0].decode() in visible_text(output):
                if resize:
                    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 20, 45, 0, 0))
                    os.killpg(child.pid, signal.SIGWINCH)
                    resize = False
                    search_start = len(output)
                    continue  # wait for the resized frame before injecting keys
                search_start = len(output)
                _, keys_to_send = pending.pop(0)
                os.write(master, keys_to_send)
            if select.select([status_read], [], [], 0)[0]:
                result = int(os.read(status_read, 32))
                while select.select([master], [], [], 0)[0]:
                    try:
                        chunk = os.read(master, 65536)
                    except OSError as error:
                        if error.errno == errno.EIO:
                            break
                        raise
                    if not chunk:
                        break
                    output.extend(chunk)
                break
        else:
            raise AssertionError(f"PTY timeout: {command}; waiting for {pending[0][0] if pending else None!r}")
        assert result == expected, f"expected exit {expected}, got {result}; output withheld"
        assert not pending, "application exited before expected UI transitions"
        after = termios.tcgetattr(slave)
        # BSD may set PENDIN after tcsetattr: a transient request to retype pending
        # input on the next read, not a changed terminal configuration.
        before[3] &= ~getattr(termios, "PENDIN", 0)
        after[3] &= ~getattr(termios, "PENDIN", 0)
        assert after == before, f"terminal settings were not restored: before={before!r}, after={after!r}"
        if expected == 0:
            assert b"\x1b[?1049h" in output, "alternate screen was not entered"
            assert b"\x1b[?1049l" in output, "alternate screen was not restored"
            assert b"\x1b[?25h" in output, "cursor was not restored"
        else:
            assert b"\x1b[?1049h" not in output, "invalid input entered terminal mode"
        return bytes(output)
    finally:
        os.close(release_write)
        os.close(status_read)
        if child.poll() is None:
            try:
                child.wait(timeout=2)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
        os.close(master)
        os.close(slave)


binary = sys.argv[1] if len(sys.argv) > 1 else "target/release/den"
run_pty([binary, "--demo"], b"j\t\x1b[B\t/rust\r??q", resize=True)
print("PASS: demo navigation, search, help, resize and q restore terminal")
run_pty([binary, "--snapshot", "fixtures/demo.json"], b"/rust\x03")
print("PASS: snapshot and Ctrl-C during search restore terminal")
run_pty([binary, "--snapshot", "Cargo.toml"], expected=1)
print("PASS: invalid snapshot fails before terminal setup")

with tempfile.TemporaryDirectory(prefix="den-smoke-") as temporary:
    isolated_env = dict(os.environ, PATH=temporary)
    run_pty([binary], env=isolated_env, steps=[(b"Homebrew unavailable", b"q")])
    print("PASS: missing Homebrew is visible and exits cleanly")
    calls = Path(temporary) / "calls"
    fake = Path(temporary) / "brew"
    # A fixture-only executable verifies the production argv/environment contract.
    fake.write_text(f"#!{sys.executable}\n" + """
import os, pathlib, sys, time
assert sys.argv[1:] == ['info', '--json=v2', '--installed']
assert os.environ['HOMEBREW_NO_AUTO_UPDATE'] == '1'
assert os.environ['HOMEBREW_NO_ANALYTICS'] == '1'
calls = pathlib.Path(__file__).with_name('calls')
count = int(calls.read_text()) + 1 if calls.exists() else 1
calls.write_text(str(count))
time.sleep(0.2)
if count > 1:
    print('private diagnostic must not reach the screen', file=sys.stderr)
    sys.exit(7)
print(pathlib.Path('fixtures/brew.json').read_text())
""")
    fake.chmod(0o700)
    run_pty([binary, "--brew-snapshot", "fixtures/brew.json"], b"fri/ripgrep\r\tq", env=isolated_env)
    assert not calls.exists(), "snapshot mode must not execute Homebrew"
    for size in [(80, 24), (120, 32), (160, 45)]:
        run_pty([binary, "--brew-snapshot", "fixtures/brew.json"], env=isolated_env, size=size, steps=[
            (b"Inventory", b"j"),
            (b"> pcre2", b"\t"),
            (b"Skills inventory", b"\x1b[Z"),
            (b"> pcre2", b"\r"),
            (b"Inspector / Overview", b"\t"),
            (b"Skills inventory", b"\x1b[Z"),
            (b"Inspector / Overview", b"q"),
        ])
    assert not calls.exists(), "area switching must preserve snapshot isolation"
    print("PASS: Tab/Shift-Tab areas restore selection and explicit inspectors at three sizes")
    for size in [(80, 24), (120, 32), (160, 45)]:
        run_pty([binary, "--brew-snapshot", "fixtures/brew.json"], env=isolated_env, size=size, steps=[
            (b"[All 7]", b"u"),
            (b"Used by others 1 [ON]", b"i"),
            (b"No matches", b"i"),
            (b"> pcre2", b"\t"),
            (b"Skills inventory", b"\x1b[Z"),
            (b"Used by others 1 [ON]", b"u"),
            (b"Used by others 1 [off]", b"q"),
        ])
    assert not calls.exists(), "intent and usage filtering must remain offline"
    print("PASS: visible intent counts, separate recorded-used filter, empty matches and area context at three sizes")
    for size in [(80, 24), (120, 32), (160, 45)]:
        output = run_pty([binary, "--brew-snapshot", "fixtures/brew-dependencies.json"], env=isolated_env, size=size, steps=[
            (b"Inventory", b"/mid\r"),
            (b"Recorded runtime", b"\x1b[C \x1b[Dq"),
        ])
        text = visible_text(output)
        for label in ["Depends on", "Required by", "leaf", "root-a", "keg 2"]:
            assert label in text, f"passive tree missing {label} at {size}"
        assert "Inspector /" not in text, "passive tree keys must not open an inspector"
    assert not calls.exists(), "passive trees must never execute Homebrew"
    print("PASS: passive dependency trees in both directions with evidence/keg context at three sizes")
    for size in [(80, 24), (120, 32), (160, 45)]:
        run_pty([binary, "--brew-snapshot", "fixtures/brew-dependencies.json"], env=isolated_env, size=size, steps=[
            (b"Inventory", b"\r/dependencies\r\r"),
            (b"[+] mid", b"\x1b[C"),
            (b"[-] mid", b"\x1b[C"),
            (b"dependency recorded version 1", b"\x1b[D"),
            (b"dependency recorded version 2", b"\x1b[D"),
            (b"[+] mid", b"\r"),
            (b"1 Overview", b"\x7f"),
            (b"[+] mid", b"3"),
            (b"Required by", b"4"),
            (b"Dependency paths", b"s"),
            (b"Current declarations", b"q"),
        ])
    assert not calls.exists(), "dependency exploration must remain offline"
    print("PASS: dependency trees, expand/child/collapse/parent, inspect/back and evidence at three sizes")
    run_pty([binary, "--brew-snapshot", "fixtures/brew-diagnostics.json"], env=isolated_env, steps=[
        (b"Inventory", b"5"),
        (b"Unresolved", b"\r \x7f\r"),
        (b"1 Overview", b"\x7f6c"),
        (b"Not checked", b"7c8coq"),
    ])
    assert not calls.exists(), "snapshot diagnostics must never execute Homebrew"
    print("PASS: snapshot diagnostic drill-down, jump/back, and live-check isolation")
    run_pty([binary, "--skills-fixture", "fixtures/skills"], env=isolated_env, steps=[
        (b"4 discovered skills", b"f2"),
        (b"Alpha instructions", b"3"),
        (b"3 Files", b"\r"),
        (b"Instructions", b"\x7f4"),
        (b"4 Validation", b"5"),
        (b"Discovery coverage", b"\t"),
        (b"fixture (Homebrew unavailable)", b"\x1b[Z"),
        (b"Codex / Skills", b"\x1b/alpha\r1rq"),
    ])
    assert not calls.exists(), "skills fixtures and module switching must never invoke Homebrew"
    run_pty([binary, "--brew-snapshot", "fixtures/brew.json", "--skills"], env=isolated_env, steps=[
        (b"no filesystem discovery", b"rq"),
    ])
    assert not calls.exists(), "snapshot Skills view must remain isolated"
    print("PASS: Skills fixture navigation, files, findings, coverage, module switching and snapshot isolation")
    output = run_pty([binary, "--codex-fixture", "fixtures/codex"], env=isolated_env, steps=[
        (b"1 discovered skills", b"\x1bOR"),
        (b"Codex / Configuration", b"/model\r\re"),
        (b"Raw TOML is never displayed", b"\x7f\x1bOR"),
        (b"Codex / Instructions", b"/Selected\r\r/preview\r\r"),
        (b"Synthetic global guidance", b"\x7fr"),
        (b"Locally resolved", b"q"),
    ])
    assert b"PRIVATE-CONFIG-SENTINEL" not in output
    assert b"PRIVATE-INSTRUCTION-SENTINEL" not in output
    assert not calls.exists(), "Codex fixture must never execute Homebrew"
    run_pty([binary, "--demo", "--skills"], env=isolated_env, steps=[
        (b"no filesystem discovery", b"\x1bORr"),
        (b"Unavailable in snapshot", b"\x1bORq"),
    ])
    assert not calls.exists(), "snapshot configuration must remain isolated"
    print("PASS: Codex configuration sources, redacted instruction previews, refresh and snapshot isolation")
    output = run_pty([binary, "--codex-fixture", "fixtures/mcp"], env=isolated_env, steps=[
        (b"discovered skills", b"\x1bOR\x1bOR\x1bOR"),
        (b"7 MCP servers", b"/docs\r2"),
        (b"Launcher: npx", b"3"),
        (b"Section 3: Authentication", b"4"),
        (b"Allowlist first", b"5e"),
        (b"Codex / Configuration", b"\x7f6"),
        (b"Section 6: Findings", b"q"),
    ])
    assert b"MCP-SECRET-CANARY" not in output
    assert not calls.exists(), "MCP inspection must not invoke Homebrew"
    for size in [(80, 24), (120, 32), (160, 45)]:
        output = run_pty([binary, "--codex-fixture", "fixtures/codex"], env=isolated_env, size=size, steps=[
            (b"discovered skills", b"/example\r\r"),
            (b"Inspector / Overview", b"/evidence\r\r"),
            (b"Inspector / Evidence", b"?"),
            (b"Help", b"\x1b"),
            (b"Inspector / Evidence", b"\x1b"),
            (b"Inspector / Overview", b"\x1b"),
            (b"Skills inventory", b"g"),
            (b"Sections", b"/configuration\r"),
            (b"Codex / Configuration", b"g/mcp\r"),
            (b"Codex / MCP", b"q"),
        ])
        assert b"PRIVATE-CONFIG-SENTINEL" not in output
        assert b"PRIVATE-INSTRUCTION-SENTINEL" not in output
    print("PASS: section switcher, inspector Evidence and Esc history at 80x24, 120x32 and 160x45")
    output = run_pty([binary, "--codex-fixture", "fixtures/mcp"], env=isolated_env, size=(80, 24), steps=[
        (b"discovered skills", b"g/mcp\r"),
        (b"7 MCP servers", b"/docs\r\r/sources\r\r"),
        (b"Codex / Configuration", b"\x1b"),
        (b"Codex / MCP", b"\x1b"),
        (b"MCP inventory", b"/\x15absent\r"),
        (b"No matching entries", b"q"),
    ])
    assert b"MCP-SECRET-CANARY" not in output
    assert not calls.exists(), "source navigation must never invoke Homebrew"
    print("PASS: source inspector Back restores MCP selection; no-match search remains offline")
    run_pty([binary, "--codex-fixture", "fixtures/mcp-empty"], env=isolated_env, resize=True, steps=[
        (b"discovered skills", b"\x1bOR\x1bOR\x1bOR"),
        (b"0 MCP servers", b"?"),
        (b"Den ", b"?q"),
    ])
    run_pty([binary, "--demo", "--skills"], env=isolated_env, steps=[
        (b"no filesystem discovery", b"\x1bOR\x1bOR\x1bORr"),
        (b"Unavailable in snapshot", b"q"),
    ])
    assert not calls.exists(), "snapshot MCP must not request live data"
    print("PASS: MCP sections, source jump/back, search, empty/partial isolation, resize and redaction")
    output = run_pty([binary], env=isolated_env, steps=[
        (b"Loading Homebrew", b"?"),
        (b"Help", b"?"),
        (b"Loaded 7", b"r"),
        (b"STALE", b"q"),
    ])
    assert calls.read_text() == "2", "one brew invocation per collection"
    assert b"private diagnostic" not in output
    assert "code 7" in visible_text(output)
    print("PASS: snapshot stays offline; loading accepts keys; refresh failure keeps stale data")

    calls.unlink()
    fake.write_text(f"#!{sys.executable}\n" + """
import json, os, pathlib, sys, time
assert os.environ['HOMEBREW_NO_AUTO_UPDATE'] == '1'
assert os.environ['HOMEBREW_NO_ANALYTICS'] == '1'
args = sys.argv[1:]
assert args in [['info', '--json=v2', '--installed'], ['missing'], ['leaves'], ['autoremove', '--dry-run']]
calls = pathlib.Path(__file__).with_name('calls')
history = json.loads(calls.read_text()) if calls.exists() else []
history.append(args)
calls.write_text(json.dumps(history))
time.sleep(0.3)
if args[0] == 'info':
    print(pathlib.Path('fixtures/brew-diagnostics.json').read_text())
else:
    reports = json.loads(pathlib.Path('fixtures/diagnostic-output.json').read_text())
    name = {'missing':'missing', 'leaves':'leaves', 'autoremove':'cleanup_empty'}[args[0]]
    if args[0] == 'autoremove' and history.count(args) > 1:
        name = 'failure'
    report = reports[name]
    sys.stdout.write(report['stdout'])
    sys.stderr.write(report['stderr'])
    sys.exit(report['code'])
""")
    output = run_pty([binary], env=isolated_env, steps=[
        (b"Loaded 8", b"8c"),
        (b"Checking Homebrew", b"?"),
        (b"Help", b"?"),
        (b"Homebrew found no", b"o"),
        (b"Original Homebrew stdout", b"\x7f6c"),
        (b"Confirmed finding", b"\r"),
        (b"1 Overview", b"\x7f7co"),
        (b"Homebrew reports 2", b"8c"),
        (b"Check failed", b"or"),
        (b"STALE", b""),
        (b"Loaded 8", b"q"),
    ])
    history = json.loads(calls.read_text())
    assert history.count(['autoremove', '--dry-run']) == 2
    assert history.count(['missing']) == 1
    assert history.count(['leaves']) == 1
    assert history.count(['info', '--json=v2', '--installed']) == 2
    print("PASS: responsive on-demand diagnostics, expected missing exit, cleanup failure, freshness and exact dry-run argv")

build = subprocess.run(["cargo", "test", "--lib", "--no-run", "--message-format=json"],
                       check=True, capture_output=True, text=True)
artifacts = [json.loads(line) for line in build.stdout.splitlines()]
test_binary = next(item["executable"] for item in artifacts
                   if item.get("reason") == "compiler-artifact" and item.get("executable")
                   and item.get("profile", {}).get("test"))
for test in ["restores_after_io_error", "restores_after_panic"]:
    run_pty([test_binary, f"terminal::tests::{test}", "--exact", "--ignored", "--nocapture"])
print("PASS: injected I/O error and panic restore terminal")

if "--live" in sys.argv[2:]:
    run_pty([binary], steps=[(b"Loaded ", b"8co"), (b"Original Homebrew stdout", b"q")])
    print("PASS: real Homebrew inventory and on-demand cleanup preview render and exit cleanly")
    run_pty([binary, "--project", str(Path.cwd()), "--skills"], steps=[
        (b"discovered skills", b"2"),
        (b"Instructions (data)", b"5"),
        (b"Discovery coverage", b"q"),
    ])
    print("PASS: real Skills discovery, instructions, coverage and terminal restoration")

if "--live" in sys.argv[2:]:
    run_pty([binary, "--skills"], steps=[
        (b"discovered skills", b"\x1bOR"),
        (b"Locally resolved", b"\x1bOR"),
        (b"Codex / Instructions", b"\x1bOR"),
        (b"MCP servers", b"123456q"),
    ])
    print("PASS: live Codex configuration, instructions and MCP render without server execution")
