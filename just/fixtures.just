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

# Load a Den JSON snapshot, without live collection.
snapshot path="fixtures/demo.json":
    cargo run -- --snapshot "$1"

# Load raw Homebrew JSON, without invoking Homebrew.
brew path="fixtures/brew.json":
    cargo run -- --brew-snapshot "$1"

# Browse isolated synthetic skill roots.
skills path="fixtures/skills":
    cargo run -- --skills-fixture "$1"

# Browse isolated synthetic Codex configuration, skills, instructions and MCP.
codex path="fixtures/codex":
    cargo run -- --codex-fixture "$1"
