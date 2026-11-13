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
                # A list is sent as separate writes, 2 ms apart (Den waits 50 ms): escape
                # sequences split the way a busy system or a slow link delivers them.
                for index, chunk in enumerate(keys_to_send if isinstance(keys_to_send, list) else [keys_to_send]):
                    if index:
                        time.sleep(0.002)
                    os.write(master, chunk)
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
            (b"> [widget]", b"\t"),
            ("Agents  › Skills".encode(), b"\t"),
            ("Cloud  › AWS".encode(), b"\t"),
            ("Docker  › Containers".encode(), b"\t"),
            ("Toolchains  › PATH".encode(), b"\t"),
            (b"> [widget]", b"\x1b[Z"),
            (b"Diagnostics", b"\t"),
            ("Agents  › Skills".encode(), b"\t"),
            ("Cloud  › AWS".encode(), b"\t"),
            ("Docker  › Containers".encode(), b"\t"),
            ("Toolchains  › PATH".encode(), b"\t"),
            (b"Diagnostics", b"\x1b[Z"),
            (b"> [widget]", b"q"),
        ])
    assert not calls.exists(), "area switching must preserve snapshot isolation"
    print("PASS: Tab switches areas and Shift+Tab sections, restoring selection at three sizes")
    for size in [(80, 24), (120, 32), (160, 45)]:
        run_pty([binary, "--brew-snapshot", "fixtures/brew.json"], env=isolated_env, size=size, steps=[
            (b"[Tools 3]", b"a"),
            (b"[All 7]", b"\t"),
            ("Agents  › Skills".encode(), b"\t"),
            ("Cloud  › AWS".encode(), b"\t"),
            ("Docker  › Containers".encode(), b"\t"),
            ("Toolchains  › PATH".encode(), b"\t"),
            (b"required by 1", b"/zzz\r"),
            (b"No matches", b"a"),
            (b"No matching tools", b"q"),
        ])
    assert not calls.exists(), "Tools/All view must remain offline"
    print("PASS: Tools default, a toggles All, required-by rows, empty matches and area context at three sizes")
    for size in [(80, 24), (120, 32), (160, 45)]:
        output = run_pty([binary, "--brew-snapshot", "fixtures/brew-dependencies.json"], env=isolated_env, size=size, steps=[
            (b"Inventory", b"a/mid\r"),
            (b"Recorded runtime", b"\x1b[C \x1b[Dq"),
        ])
        text = visible_text(output)
        for label in ["Depends on", "Required by", "leaf", "root-a", "keg 2"]:
            assert label in text, f"passive tree missing {label} at {size}"
        assert "Inspector /" not in text, "passive tree keys must not open an inspector"
    assert not calls.exists(), "passive trees must never execute Homebrew"
    print("PASS: passive dependency trees in both directions with evidence/keg context at three sizes")
    for size in [(80, 24), (120, 32), (160, 45)]:
        output = run_pty([binary, "--brew-snapshot", "fixtures/brew-dependencies.json"], env=isolated_env, size=size, steps=[
            (b"[Tools 7]", b"\r\x1b[Cgvz2a"),
            (b"[All 26]", b"q"),
        ])
        text = visible_text(output)
        assert "Reports" not in text and "Sections" not in text, "packages must not open an inspector or switcher"
    assert not calls.exists(), "package browsing must remain offline"
    print("PASS: packages have no inspector and g opens nothing at three sizes")
    run_pty([binary, "--brew-snapshot", "fixtures/brew-diagnostics.json"], env=isolated_env, steps=[
        (b"Inventory", b"5"),
        (b"Unresolved", b"\r \x7f\r"),
        ("› Packages".encode(), b"\x7f6c"),
        (b"Not checked", b"7c8coq"),
    ])
    assert not calls.exists(), "snapshot diagnostics must never execute Homebrew"
    print("PASS: snapshot diagnostic drill-down, jump/back, and live-check isolation")
    run_pty([binary, "--skills-fixture", "fixtures/skills"], env=isolated_env, steps=[
        # Wait for the end, then one page back: frames between Page Downs may be skipped.
        (b"Findings (", b"\x1b[6~" * 6),
        ("End · PgUp to go back".encode(), b"\x1b[5~"),
        (b"Codex discovery coverage", b"\t"),
        ("Cloud  › AWS".encode(), b"\t"),
        ("Docker  › Containers".encode(), b"\t"),
            ("Toolchains  › PATH".encode(), b"\t"),
        (b"fixture (Homebrew unavailable)", b"\x1b[Z\t"),
        ("Agents  › Skills".encode(), b"\x1b/alpha\r1rq"),
    ])
    assert not calls.exists(), "skills fixtures and module switching must never invoke Homebrew"
    run_pty([binary, "--brew-snapshot", "fixtures/brew.json", "--skills"], env=isolated_env, steps=[
        (b"Codex not collected in this mode", b"rq"),
    ])
    assert not calls.exists(), "snapshot Skills view must remain isolated"
    print("PASS: Skills panel, PgDn to coverage, module switching and snapshot isolation")
    output = run_pty([binary, "--codex-fixture", "fixtures/codex"], env=isolated_env, steps=[
        (b"1 skill", b"\x1bOR"),
        (b"Synthetic global guidance", b"\x1bOR"),
        ("› Settings".encode(), b"s"),
        (b"Raw TOML is never displayed", b"sr"),
        (b" settings", b"q"),
    ])
    assert b"PRIVATE-CONFIG-SENTINEL" not in output
    assert b"PRIVATE-INSTRUCTION-SENTINEL" not in output
    assert not calls.exists(), "Codex fixture must never execute Homebrew"
    run_pty([binary, "--demo", "--skills"], env=isolated_env, steps=[
        (b"Codex not collected in this mode", b"\x1bORr"),
        (b"No instruction files found", b"\x1bORq"),
    ])
    assert not calls.exists(), "snapshot configuration must remain isolated"
    print("PASS: Codex settings sources, redacted instruction previews, refresh and snapshot isolation")
    output = run_pty([binary, "--codex-fixture", "fixtures/mcp"], env=isolated_env, steps=[
        (b" skill", b"\x1bOR\x1bOR\x1bOR"),
        (b"7 servers", b"/docs\r"),
        (b"Launcher: npx", b"\x1b[6~"),
        (b"PgUp", b"q"),
    ])
    assert b"MCP-SECRET-CANARY" not in output
    assert not calls.exists(), "MCP inspection must not invoke Homebrew"
    for size in [(80, 24), (120, 32), (160, 45)]:
        output = run_pty([binary, "--codex-fixture", "fixtures/codex"], env=isolated_env, size=size, steps=[
            (b" skill", b"/example\r"),
            (b"Where: User", b"?"),
            (b"Help", b"\x1b"),
            # Help covers the whole screen, so the list title returns only once it closes.
            (b"> Skills", b"\x1b[Z"),
            ("› Instructions".encode(), b"\x1b[Z\x1b[Z"),
            ("› MCP".encode(), b"q"),
        ])
        assert b"PRIVATE-CONFIG-SENTINEL" not in output
        assert b"PRIVATE-INSTRUCTION-SENTINEL" not in output
    print("PASS: Shift+Tab sections, panel, help and Esc at 80x24, 120x32 and 160x45")
    for size in [(80, 24), (120, 32)]:
        output = run_pty([binary, "--claude-fixture", "fixtures/claude"], env=isolated_env, size=size, steps=[
            (b"1 in both", b"\x1b[Z"),
            (b"In reading order", b"j"),
            (b"Codex: Used", b"\t"),
            ("Cloud  › AWS".encode(), b"\t"),
            ("Docker  › Containers".encode(), b"\t"),
            ("Toolchains  › PATH".encode(), b"\t"),
            ("Homebrew  › Packages".encode(), b"\t"),
            ("Agents  › Instructions".encode(), b"\x1b[Z"),
            ("Agents  › Settings".encode(), b"\x1b[Z"),
            (b"1 differ", b"\x1b[Z"),
            ("Agents  › Hooks".encode(), b"\x1b[Z"),
            ("Agents  › Plugins".encode(), b"q"),
        ])
        assert b"PRIVATE-" not in output
    assert not calls.exists(), "Claude inspection must not invoke Homebrew"
    print("PASS: Agents compares Codex and Claude Code: skills, instructions, settings, MCP differs, hooks, plugins; no private values")
    for size in [(80, 24), (120, 32)]:
        output = run_pty([binary, "--aws-fixture", "fixtures/aws"], env=isolated_env, size=size, steps=[
            (b"12 profiles", b"/broken\r"),
            (b"source_profile 'nowhere'", b"\x15\x1b/\x15\r\x1b[F"),
            (b"SSO session", b"\t"),
            ("Docker  › Containers".encode(), b"\t"),
            ("Toolchains  › PATH".encode(), b"\t"),
            ("Homebrew  › Packages".encode(), b"\t\t"),
            ("Cloud  › AWS".encode(), b"q"),
        ])
        assert b"PRIVATE-" not in output
    assert not calls.exists(), "AWS inspection must not invoke Homebrew"
    print("PASS: Cloud › AWS profiles, problems, search and Tab; no credential values")
    output = run_pty([binary, "--aws-fixture", "fixtures/aws"], env=isolated_env, steps=[
        (b"12 profiles", b"!"),
        (b"Problems", b"\r"),
        ("Cloud  › AWS".encode(), b"q"),
    ])
    assert b"PRIVATE-" not in output
    assert not calls.exists(), "the Problems overview must not invoke Homebrew"
    print("PASS: ! opens the Problems overview and Enter goes to the profile")
    for size in [(80, 24), (120, 32)]:
        output = run_pty([binary, "--kube-fixture", "fixtures/kube"], env=isolated_env, size=size, steps=[
            (b"7 contexts", b"/legacy\r"),
            (b"plain http", b"\x15\x1b/\x15\r\x1b[F"),
            (b"Cluster: staging", b"\x1b[Z"),
            ("Cloud  › AWS".encode(), b"\x1b[Z"),
            ("Cloud  › Kubernetes".encode(), b"q"),
        ])
        assert b"PRIVATE" not in output
    assert not calls.exists(), "Kubernetes inspection must not invoke Homebrew"
    print("PASS: Cloud › Kubernetes contexts, problems, search and Shift+Tab; no secrets")
    for size in [(80, 24), (120, 32)]:
        output = run_pty([binary, "--toolchains-fixture", "fixtures/toolchains"], env=isolated_env, size=size, steps=[
            (b"7 commands on PATH", b"/java\r"),
            (b"no JDK", b"\x15\x1b/\x15\r\x1b[Z"),
            (b"default stable", b"\x1b[Z"),
            (b"go install 1", b"\t"),
            ("Homebrew  › Packages".encode(), b"\t\t\t\t"),
            ("Toolchains  › Packages".encode(), b"q"),
        ])
        assert b"PRIVATE" not in output
    assert not calls.exists(), "Toolchains inspection must not invoke Homebrew"
    print("PASS: Toolchains PATH, Rust and Packages, search and area switching; nothing run")
    for size in [(80, 24), (120, 32)]:
        output = run_pty([binary, "--docker-fixture", "fixtures/docker"], env=isolated_env, size=size, steps=[
            (b"3 containers", b"\x1b[Z"),
            (b"5 images", b"\x1b[Z"),
            (b"3 volumes", b"/unused\r"),
            (b"anonymous", b"\t"),
            ("Toolchains  › PATH".encode(), b"\t"),
            ("Homebrew  › Packages".encode(), b"q"),
        ])
        assert b"PRIVATE-" not in output
    assert not calls.exists(), "Docker fixtures must not invoke Homebrew"
    print("PASS: Docker containers, images, volumes and disk use from fixtures; no private values")
    with tempfile.TemporaryDirectory(prefix="den-docker-") as docker_dir:
        fake_docker = Path(docker_dir) / "docker"
        # Den may run only these four listings; anything else fails the smoke check.
        fake_docker.write_text(f"#!{sys.executable}\n" + """
import os, pathlib, sys
allowed = {
    ('container', 'ls', '--all', '--no-trunc', '--format', '{{json .}}'): 'containers',
    ('image', 'ls', '--no-trunc', '--format', '{{json .}}'): 'images',
    ('volume', 'ls', '--format', '{{json .}}'): 'volumes',
    ('system', 'df', '--format', '{{json .}}'): 'disk',
}
args = tuple(sys.argv[1:])
log = pathlib.Path(__file__).with_name('docker-calls')
# Append: Den runs the four listings in parallel.
with open(log, 'a') as f:
    f.write(' '.join(args) + '\\n')
assert args in allowed, args
assert os.environ.get('DOCKER_CLI_HINTS') == 'false'
if pathlib.Path(__file__).with_name('down').exists():
    print('Cannot connect to the Docker daemon PRIVATE-DOCKER-ERROR', file=sys.stderr)
    sys.exit(1)
print(pathlib.Path('fixtures/docker', allowed[args] + '.jsonl').read_text())
""")
        fake_docker.chmod(0o700)
        docker_env = dict(os.environ, PATH=docker_dir)
        run_pty([binary], env=docker_env, steps=[
            (b"Homebrew unavailable", b"\t\t\t"),
            (b"3 containers", b"q"),
        ])
        log = (Path(docker_dir) / "docker-calls").read_text().splitlines()
        assert sorted(log) == sorted([
            "container ls --all --no-trunc --format {{json .}}",
            "image ls --no-trunc --format {{json .}}",
            "volume ls --format {{json .}}",
            "system df --format {{json .}}",
        ]), log
        (Path(docker_dir) / "down").write_text("")
        output = run_pty([binary], env=docker_env, steps=[
            (b"Homebrew unavailable", b"\t\t\t"),
            (b"Docker daemon not reachable", b"q"),
        ])
        assert b"PRIVATE-DOCKER-ERROR" not in output
    print("PASS: Docker runs only the four listing commands, only when opened; a stopped daemon is explained")
    output = run_pty([binary, "--codex-fixture", "fixtures/mcp"], env=isolated_env, size=(80, 24), steps=[
        (b" skill", b"\x1b[Z\x1b[Z\x1b[Z"),
        (b"7 servers", b"/docs\r"),
        ("› MCP".encode(), b"/\x15absent\r"),
        (b"No matches", b"q"),
    ])
    assert b"MCP-SECRET-CANARY" not in output
    assert not calls.exists(), "source navigation must never invoke Homebrew"
    print("PASS: MCP search and no-match search remain offline")
    run_pty([binary, "--codex-fixture", "fixtures/mcp-empty"], env=isolated_env, steps=[
        # F3 three times with each ESC arriving before the rest of its sequence.
        (b" skill", [b"\x1b", b"OR", b"\x1b", b"OR", b"\x1b", b"OR"]),
        (b"0 servers", [b"\x1b", b"[Z"]),
        ("Agents  › Hooks".encode(), b"q"),
    ])
    print("PASS: escape sequences split by the OS still read as F3 and Shift+Tab")
    run_pty([binary, "--codex-fixture", "fixtures/mcp-empty"], env=isolated_env, resize=True, steps=[
        (b" skill", b"\x1bOR\x1bOR\x1bOR"),
        (b"0 servers", b"?"),
        (b"Den ", b"?q"),
    ])
    run_pty([binary, "--demo", "--skills"], env=isolated_env, steps=[
        (b"Codex not collected in this mode", b"\x1bOR\x1bOR\x1bORr"),
        (b"No MCP servers found", b"q"),
    ])
    assert not calls.exists(), "snapshot MCP must not request live data"
    print("PASS: MCP panel, search, empty/partial isolation, resize and redaction")
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
        ("› Packages".encode(), b"\x7f7co"),
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

    # Write actions: a fake brew records every argv. In den nothing but `info` may run;
    # in dew, x, the dry run and Enter run exactly the chosen command.
    calls.unlink()
    state = Path(temporary) / "state"
    fake.write_text(f"#!{sys.executable}\n" + """
import json, os, pathlib, sys
for name in ['HOMEBREW_NO_AUTO_UPDATE', 'HOMEBREW_NO_ANALYTICS']:
    assert os.environ[name] == '1', name
args = sys.argv[1:]
calls = pathlib.Path(__file__).with_name('calls')
history = json.loads(calls.read_text()) if calls.exists() else []
history.append(args)
calls.write_text(json.dumps(history))
if args == ['info', '--json=v2', '--installed']:
    # After an action Den reloads; a different inventory proves the reload finished.
    acted = ['cleanup'] in history
    print(pathlib.Path('fixtures/brew.json' if acted else 'fixtures/brew-diagnostics.json').read_text())
elif args == ['cleanup', '--dry-run']:
    print('Would remove: old-download.tar.gz (1MB)')
elif args == ['cleanup']:
    print('Removing: old-download.tar.gz')
else:
    sys.exit(3)
""")
    write_env = dict(isolated_env, XDG_STATE_HOME=str(state))
    # den has no actions: x and Enter do nothing.
    output = run_pty([binary], env=write_env, steps=[
        (b"Loaded 8", b"Wx\ryq"),
    ])
    assert b"Actions" not in output and b"x:actions" not in output and b"WRITE" not in output
    assert json.loads(calls.read_text()) == [["info", "--json=v2", "--installed"]]
    assert not state.exists(), "nothing ran, so nothing is logged"
    calls.unlink()
    output = run_pty([str(Path(binary).with_name("dew"))], env=write_env, steps=[
        (b"Loaded 8", b"x"),
        # Uninstall is last; the item above it is cleanup.
        (b"Actions", [b"\x1b[F", b"\x1b[A"]),
        # Enter runs the selected action once its dry run is shown.
        (b"Would remove: old-download", b"\r"),
        (b"Done", b"\r"),
        (b"Loaded 7", b"q"),
    ])
    assert json.loads(calls.read_text()) == [
        ["info", "--json=v2", "--installed"],
        ["cleanup", "--dry-run"],
        ["cleanup"],
        ["info", "--json=v2", "--installed"],
    ], calls.read_text()
    assert b"WRITE" in output, "dew shows WRITE"
    log = (state / "den" / "actions.log").read_text()
    assert log.endswith("\tbrew cleanup\tDone\n") and "old-download" not in log, log
    print("PASS: write actions only in dew; exact argv, dry run first, logged without output")

build = subprocess.run(["cargo", "test", "--lib", "--no-run", "--message-format=json"],
                       check=True, capture_output=True, text=True)
artifacts = [json.loads(line) for line in build.stdout.splitlines()]
test_binary = next(item["executable"] for item in artifacts
                   if item.get("reason") == "compiler-artifact" and item.get("executable")
                   and item.get("profile", {}).get("test"))
for test in ["restores_after_io_error", "restores_after_panic"]:
    run_pty([test_binary, f"tui::terminal::tests::{test}", "--exact", "--ignored", "--nocapture"])
print("PASS: injected I/O error and panic restore terminal")

if "--live" in sys.argv[2:]:
    run_pty([binary], steps=[(b"Loaded ", b"8co"), (b"Original Homebrew stdout", b"q")])
    print("PASS: real Homebrew inventory and on-demand cleanup preview render and exit cleanly")
    run_pty([binary, "--project", str(Path.cwd()), "--skills"], steps=[
        (b" skill", b"\x1b[6~" * 40),
        ("End · PgUp to go back".encode(), b"\x1b[5~"),
        (b"Codex discovery coverage", b"q"),
    ])
    print("PASS: real Skills discovery, instructions, coverage and terminal restoration")

if "--live" in sys.argv[2:]:
    run_pty([binary, "--skills"], steps=[
        (b" skill", b"\x1bOR"),
        (b" folder", b"\x1bOR"),
        (b" settings", b"\x1bOR"),
        (b" servers", b"\x1b[6~q"),
    ])
    print("PASS: live Codex configuration, instructions and MCP render without server execution")
