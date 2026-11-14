# Fixture recipes always use repository-relative paths, including from subdirectories.
set positional-arguments
set working-directory := '..'

# List the offline launchers; never launch the TUI by default.
[private]
default:
    @just --list fixtures

# Launch the embedded synthetic inventory.
demo:
    cargo run -- --demo

# Load a saved Den JSON inventory, without live collection.
inventory path="fixtures/demo.json":
    cargo run -- --inventory "$1"

# Load raw Homebrew JSON, without invoking Homebrew.
brew path="fixtures/brew.json":
    cargo run -- --brew-snapshot "$1"

# Browse isolated synthetic skill roots.
skills path="fixtures/skills":
    cargo run -- --skills-fixture "$1"

# Browse isolated synthetic Codex configuration, skills, instructions and MCP.
codex path="fixtures/codex":
    cargo run -- --codex-fixture "$1"

# Browse isolated synthetic Codex and Claude Code side by side in Agents.
claude path="fixtures/claude":
    cargo run -- --claude-fixture "$1"

# Browse isolated synthetic AWS profiles in Cloud › AWS.
aws path="fixtures/aws":
    cargo run -- --aws-fixture "$1"

# Browse saved synthetic Docker listings without running docker.
docker path="fixtures/docker":
    cargo run -- --docker-fixture "$1"

# Browse isolated synthetic kubeconfig contexts in Cloud › Kubernetes.
kube path="fixtures/kube":
    cargo run -- --kube-fixture "$1"

# Browse an isolated synthetic PATH, Rust and global packages in Toolchains.
toolchains path="fixtures/toolchains":
    cargo run -- --toolchains-fixture "$1"
