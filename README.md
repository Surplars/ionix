# Ionix

An interactive TUI for configuring kernel and RTOS options via TOML schemas, with type-safe Rust code generation.

![Rust](https://img.shields.io/badge/Rust-1.70+-dea584?style=flat-square&logo=rust)
![License](https://img.shields.io/badge/License-MIT-blue?style=flat-square)

## Overview

Ionix is a configuration tool that provides an interactive terminal UI for editing kernel options defined in TOML schema files. It generates type-safe Rust code from your configuration, making it ideal for embedded systems and kernels like OpenIon.

## Features

- **Interactive TUI** — Browse, search, and modify kernel options with a keyboard-driven interface
- **Fuzzy Search** — Quickly find configuration items by name or key
- **Type Safety** — Generate type-checked Rust code from your configuration
- **Dependency Resolution** — Understand config relationships (depends_on)
- **Batch Mode** — Generate configs without TUI for CI/CD pipelines
- **Diff Mode** — Compare configs with schemas and annotate differences

## Installation

```bash
cargo install ionix
```

Or build from source:

```bash
git clone https://github.com/yourusername/ionix.git
cd ionix
cargo build --release
```

## Quick Start

### 1. Create a Schema

Define your kernel options in a TOML file:

```toml
# kernel.kconfig
[[items]]
name = "Symmetric Multiprocessing"
key = "SMP"
type = "bool"
default = true
help = "Enable SMP support for multi-core systems."

[[items]]
name = "Maximum CPUs"
key = "MAX_CPUS"
type = "u64"
default = 256
depends_on = ["SMP"]
```

### 2. Interactive Mode

```bash
ionix --schema kernel.kconfig
```

### 3. Batch Mode (CI/CD)

```bash
ionix --schema kernel.kconfig --batch
```

### 4. Generate with Existing Config

```bash
ionix --schema kernel.kconfig --config my_config.toml --batch
```

### 5. Diff Mode

```bash
ionix --schema kernel.kconfig --config my_config.toml --diff
```

## CLI Options

| Flag | Description |
|------|-------------|
| `-s, --schema <file>` | Path to TOML schema file (required) |
| `-c, --config <file>` | Path to existing config file (optional) |
| `-e, --export <file>` | Output path for generated Rust code |
| `--batch` | Non-interactive batch mode |
| `-d, --diff` | Diff config with schema |

## Library Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
ionix = "0.1"
```

In your `build.rs`:

```rust
fn main() {
    ionix::generate("kernel.kconfig", "generated_config.rs")
        .expect("failed to generate config");
}
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `↑`/`↓` | Navigate options |
| `Enter` | Toggle/Edit value |
| `/` | Fuzzy search |
| `Esc` | Clear search / Go back |
| `g` | Go to top |
| `G` | Go to bottom |
| `e` | Toggle expert mode |
| `h` | Show help |
| `s` | Save config |
| `Ctrl+q` | Quit |

## Configuration Types

| Type | Example | Generated |
|------|---------|-----------|
| `bool` | `true`/`false` | `const CONFIG_SMP: bool = true;` |
| `u64` | `256` | `const CONFIG_MAX_CPUS: u64 = 256;` |
| `string` | `"eth0"` | `const CONFIG_NET_IF: &str = "eth0";` |

## Project Structure

```
ionix/
├── src/
│   ├── schema/       # TOML schema parsing
│   ├── config/       # Config loading & saving
│   └── ui/           # TUI components
├── examples/
│   └── example.toml  # Example schema
└── Cargo.toml
```

## License

MIT