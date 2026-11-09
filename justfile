set positional-arguments

default:
    @just --list

build:
    cargo build --release --locked

test:
    cargo test --locked

check:
    cargo fmt --check
    cargo clippy --locked --all-targets -- -D warnings

# Default prefix is ~/.local; accepts paths containing spaces.
install prefix=(env_var("HOME") / ".local"): build
    python3 scripts/install.py "$1"

# Prints configuration to merge into the MCP client's settings.
mcp-config:
    cargo run --locked -- mcp-config

# Register the installed server in Codex or Claude Code's user configuration.
install-mcp client prefix=(env_var("HOME") / ".local"):
    python3 scripts/install_mcp.py "$1" --prefix "$2"

# Verify the installed CLI and MCP in a temporary prefix (does not change client settings).
test-install:
    python3 scripts/smoke_install.py
