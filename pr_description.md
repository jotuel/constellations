🧹 [Code Health] Refactor Timeline Scrolled Logic in handle_update

🎯 **What:** Extracted the massive duplicated logic inside the `Message::TimelineScrolled(viewport, is_thread)` match arm of `Constellations::handle_update` into a dedicated helper method `handle_timeline_scrolled`.
💡 **Why:** To significantly improve maintainability and readability by reducing the size of the 1400+ line `handle_update` method. The original logic contained ~100 lines of identical duplicated behavior between the threaded and non-threaded branches. The new helper unifies these paths, conditionally mapping the target state variables via the `is_thread` flag, reducing code size by ~70 lines.
✅ **Verification:** Verified by compiling the application (`cargo check`), successfully passing the test suite (`cargo test --bin constellations`), and running clippy without related warnings. Code review feedback addressed the initial return type mismatch (`Task<Action<Message>>` fixed to `Task<Message>`).
✨ **Result:** Improved modularity and easier comprehension of the `TimelineScrolled` handling. Reduced codebase redundancy without changing existing application behavior.
