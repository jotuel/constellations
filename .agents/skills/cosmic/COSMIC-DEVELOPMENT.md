Cosmic Desktop Development
This skill provides guidelines and best practices for developing applications and applets for the System76 COSMIC Desktop Environment (COSMIC DE).

1. Overview & Technology Stack
COSMIC applications are built using Rust and libcosmic.

Language: Rust (Safe, fast, concurrent)
GUI Toolkit: libcosmic (Built on top of iced, a cross-platform GUI library inspired by Elm)
Design System: COSMIC Design System (Theming, consistent widgets)
2. Project Setup
Always start strict adherence to COSMIC standards by using official templates.

App Template
For standalone applications:

*.bash
Shell
cargo generate --git https://github.com/pop-os/cosmic-app-template
Applet Template
For panel applets:

*.bash
Shell
cargo generate --git https://github.com/pop-os/cosmic-applet-template
3. Core Development Principles
GUI Architecture (The Elm Architecture)
libcosmic (and iced) follows the Model-View-Update (MVU) pattern:

State (Model): The data structure describing the application's state.
Message: Enum variants representing user actions or events.
Update: A pure function (fn update(&mut state, message)) that modifies the state based on a message.
View: A pure function (fn view(&state) -> Element) that renders the UI based on the state.
Best Practices
Use libcosmic Widgets: Always prefer libcosmic widgets over raw iced widgets when available to ensure consistent styling and integration with the desktop theme.
Modular Design: Separate your update, view, and state logic. For complex apps, break components into sub-modules with their own MVU cycles.
Configuration: Integrate with cosmic-config for handling user settings. This ensures your app's settings persist and respect system-wide overrides.
Theming: Do not hardcode colors! Use the semantic colors provided by the cosmic-theme (e.g., theme.palette.primary, theme.palette.background). This ensures your app looks correct in both Light and Dark modes.
List & Sidebar Item Styling: Avoid standard buttons for room lists, settings lists, or navigation sidebars. Instead, use `cosmic::widget::button::custom(...)` styled as `cosmic::theme::Button::ListItem(radii)` and specify the selected state using `.selected(is_selected)`. This removes blocky button backgrounds and uses clean, native selection highlights.
Accessing Theme in View Functions: If your view helper functions do not receive the `Theme` argument directly, you can access the current system theme and properties (like corner radii) via `self.core.system_theme()` from the application's `Core` state structure.
4. Applet Specifics
Composability: Applets often live in the panel. Keep them lightweight.
Popup vs Embedded: Decide if your applet needs a popover menu (like WiFi) or just an icon/text (like a clock).
Responsiveness: Applets must handle panel resizing gracefully.
Wayland Integration: COSMIC is Wayland-first. Ensure your applet interacts correctly with the compositor layers if doing custom windowing.
5. Important Libraries (The "Cosmic Stack")
cosmic-text: Advanced text shaping and rendering.
cosmic-config: Type-safe configuration management.
cosmic-theme: Access to system colors and metrics.
cosmic-comp: The compositor (useful for reference if interacting with window management).
6. Resources
libcosmic Source: https://github.com/pop-os/libcosmic
Official Examples: Check examples/ in the libcosmic repo.
Iced Documentation: https://docs.rs/iced (Foundational knowledge)
System76 Dev Docs: https://github.com/pop-os/cosmic-epoch
7. Troubleshooting
"Component not found": Ensure you have libcosmic features enabled in Cargo.toml.
Theming issues: Verify you are taking Theme as an argument in your view function and passing it correctly.
Decryption Key Mismatches in Lazy Stores: If your app uses multiple encrypted stores (e.g., matrix-rust-sdk SQLite store and search index directory), the SQLite store is typically verified/initialized at startup while other directories may be loaded lazily in background threads or tasks. If the passphrase regenerates or changes (e.g. keyring reset/lock), the main store might initialize from scratch successfully, but lazy stores will fail later with decryption/MAC errors (e.g., `invalid MAC of the store key`). Always ensure that if the main store database is cleared or created fresh, all associated lazy cache directories are cleared alongside it.

8. UI Testing & Automation
For automated testing, screenshotting, or controlling running COSMIC applications programmatically, refer to the detailed [cosmic-automation.md](file:///home/jwnz/.agents/skills/cosmic-automation.md) skill. It includes:
- Non-interactive screenshotting using the native `cosmic-screenshot` portal client.
- Keypress/input simulation under Wayland using `wtype` (virtual keyboard protocol).
- Default tiling/window management shortcuts (like closing with `Super + Q`).
