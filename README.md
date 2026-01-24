# todo-rust

Minimal, fast command-line todo manager written in Rust.

## Features

- Simple and lightweight
- Fast startup and operation
- Easy to install via Nix

## Quick Install (via Nix)

```bash
# Install latest version from GitHub
nix profile install github:jcaffey/todo-rust
```

After installation, just run:

```bash
todo --help
```

## Development & Building with Nix

This project uses a Nix Flake for reproducible builds and development.

### Enter the development environment

```bash
nix develop
```

This gives you:

- Full Rust toolchain (rustc, cargo, rustfmt, clippy)
- All other development dependencies

### Common commands

```bash
# Build the project (creates ./result/bin/todo)
nix build

# Run the program directly without installing
nix run

# Run with arguments
nix run . -- --help
nix run . -- add "Buy milk" --priority high
```

### Project structure (what the flake provides)

- `buildRustPackage` with `cargoLock` integration  
  → Uses your existing `Cargo.lock` — no manual hash calculation needed
- Clean development shell with modern Rust tools
- Default package output (`nix run`, `nix profile install`, etc.)

## Publishing / Updating

1. Make your changes
2. Update version in `Cargo.toml` (if needed)
3. Commit everything including `Cargo.lock`

```bash
git add .
git commit -m "feat: add priority support"
git push
```

Anyone can now install the updated version with:

```bash
nix profile install github:jcaffey/todo-rust
# or pin a specific commit/branch/tag
nix profile install github:jcaffey/todo-rust?ref=v0.2.1
```

That's it — Nix handles dependency fetching and binary caching automatically. LIKE A BOSS.

## Why cargoLock instead of vendoring/fetchCargoTarball?

- Much simpler maintenance
- No need to regenerate hashes when dependencies change
- Still fully reproducible
- Works great with private / git dependencies (if any)

Happy todo-ing!
