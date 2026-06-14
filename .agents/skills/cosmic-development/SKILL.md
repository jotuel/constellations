---
name: cosmic-development
description: Best practices, guidelines, Model-View-Update pattern, widget styling, and lazy store troubleshooting for System76 COSMIC desktop development.
---

# COSMIC Desktop Development

This skill provides guidelines and best practices for developing applications and applets for the System76 COSMIC Desktop Environment (COSMIC DE).

## 1. Overview & Technology Stack
COSMIC applications are built using Rust and libcosmic.
* **Language**: Rust (Safe, fast, concurrent)
* **GUI Toolkit**: libcosmic (Built on top of iced, a cross-platform GUI library inspired by Elm)
* **Design System**: COSMIC Design System (Theming, consistent widgets)

## 2. Core Development Principles
### GUI Architecture (The Elm Architecture)
libcosmic (and iced) follows the Model-View-Update (MVU) pattern:
* **State (Model)**: The data structure describing the application's state.
* **Message**: Enum variants representing user actions or events.
* **Update**: A pure function (`fn update(&mut state, message)`) that modifies the state based on a message.
* **View**: A pure function (`fn view(&state) -> Element`) that renders the UI based on the state.

### Best Practices
* **Use libcosmic Widgets**: Always prefer libcosmic widgets over raw iced widgets when available to ensure consistent styling and integration with the desktop theme.
* **Modular Design**: Separate your update, view, and state logic. For complex apps, break components into sub-modules with their own MVU cycles.
* **Configuration**: Integrate with `cosmic-config` for handling user settings. This ensures your app's settings persist and respect system-wide overrides.
* **Theming**: Do not hardcode colors! Use the semantic colors provided by the cosmic theme (e.g., `theme.palette.primary`, `theme.palette.background`). This ensures your app looks correct in both Light and Dark modes.
* **List & Sidebar Item Styling**: Avoid standard buttons for room lists, settings lists, or navigation sidebars. Instead, use `cosmic::widget::button::custom(...)` styled as `cosmic::theme::Button::ListItem(radii)` and specify the selected state using `.selected(is_selected)`. This removes blocky button backgrounds and uses clean, native selection highlights.
* **Accessing Theme in View Functions**: If your view helper functions do not receive the `Theme` argument directly, you can access the current system theme and properties (like corner radii) via `self.core.system_theme()` from the application's `Core` state structure.

## 3. Important Libraries (The "Cosmic Stack")
* **cosmic-text**: Advanced text shaping and rendering.
* **cosmic-config**: Type-safe configuration management.
* **cosmic-theme**: Access to system colors and metrics.
* **cosmic-comp**: The compositor (useful for reference if interacting with window management).

## 4. Troubleshooting
* **"Component not found"**: Ensure you have libcosmic features enabled in `Cargo.toml`.
* **Theming issues**: Verify you are taking Theme as an argument in your view function and passing it correctly.
* **Decryption Key Mismatches in Lazy Stores**: If your app uses multiple encrypted stores (e.g., matrix-rust-sdk SQLite store and search index directory), the SQLite store is typically verified/initialized at startup while other directories may be loaded lazily in background threads or tasks. If the passphrase regenerates or changes (e.g. keyring reset/lock), the main store might initialize from scratch successfully, but lazy stores will fail later with decryption/MAC errors (e.g., `invalid MAC of the store key`). Always ensure that if the main store database is cleared or created fresh, all associated lazy cache directories are cleared alongside it.

## 5. Related Skills
These companion skills extend COSMIC development with specialized guidance. Load them when their domain is relevant to your task:

| Skill | Trigger it when… |
|---|---|
| **bolt** — Performance optimization | You are profiling, reducing allocations, fixing render-loop hotspots, or addressing `RwLock` / async concurrency bottlenecks. |
| **palette** — UI/UX & Accessibility | You are building or reviewing widgets, empty states, tooltips, disabled buttons, destructive actions, or localized copy. |
| **sentinel** — Security & Stability | You are handling user input parsing, cryptographic key/passphrase generation, or blocking I/O inside async tasks. |
| **testing** — Unit Testing | You are writing or debugging tests, verifying state-machine update flows, or isolating compilation issues in test packages. |
