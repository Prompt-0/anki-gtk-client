# Anki GTK Client (Rookery)

[![Rust](https://img.shields.io/badge/rust-2021%20workspace-orange?style=for-the-badge&logo=rust)](https://github.com/Prompt-0/anki-gtk-client)
[![GTK4](https://img.shields.io/badge/gui-GTK4%20•%20Libadwaita-red?style=for-the-badge&logo=gnome)](https://github.com/Prompt-0/anki-gtk-client)
[![SQLite](https://img.shields.io/badge/storage-SQLite-blue?style=for-the-badge&logo=sqlite)](https://github.com/Prompt-0/anki-gtk-client)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=for-the-badge)](LICENSE)

A high-performance, native desktop Anki flashcard client built with Rust, GTK4, Libadwaita, and SQLite. Architected as a modular Cargo workspace providing fast, local spaced-repetition card reviews with native GNOME look and feel.

---

## 🏛️ Workspace Architecture

The codebase is split into modular, decoupled crates:

```
crates/
├── rookery-core/     # Pure spaced-repetition scheduling algorithms (SM-2, FSRS)
├── rookery-proto/    # Anki collection schema & protobuf message definitions
├── rslib-shim/       # Direct bindings and compatibility shims for Anki SQLite databases
└── rookery-gtk/      # Native GTK4 / Libadwaita desktop application UI
```

---

## ⚡ Features

- **Native Linux Experience**: Styled with modern Libadwaita widgets, dark mode support, and Wayland compliance.
- **Sub-Millisecond Card Reviews**: Direct SQLite integration reading standard `.anki2` collection databases with zero Python runtime overhead.
- **Custom Scheduling Algorithms**: Clean interfaces for SuperMemo SM-2 and FSRS spaced repetition algorithms.
- **Hardware Acceleration**: GPU-rendered typography and card flipping animations via GTK4 GSK renderer.

---

## 🚀 Building & Running

### Prerequisites
- Rust 1.80+
- GTK4 & Libadwaita development libraries (`gtk4-devel`, `libadwaita-devel`)
- Meson & Ninja (optional for system packaging)

```bash
# Clone the repository
git clone https://github.com/Prompt-0/anki-gtk-client.git
cd anki-gtk-client

# Build release workspace
cargo build --release

# Run the GTK client
cargo run --release -p rookery-gtk
```

---

## 📄 License
Licensed under the MIT License.
