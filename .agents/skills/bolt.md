## 2024-04-24 - [Reduced RwLock contention on `SpaceHierarchy`]
**Learning:** `update_filtered_rooms` heavily iterated through `self.room_list` calling `MatrixEngine::is_in_space_sync` iteratively. The underlying state access acquired an `RwLock` for each query.
**Action:** Created `is_in_space_bulk` to retrieve all boolean checks in a single read lock. This reduces locking overhead N-fold. It is important to structure `bulk` checks so they correctly handle iterators locally while retaining safe short-circuit mechanisms, and only extract string matches or object references once to avoid string parsing deduplications.

## 2024-04-27 - [Optimized Case-Insensitive String Filtering]
**Learning:** In hot loops like `update_filtered_rooms`, calling `.to_lowercase()` unconditionally on every string just to do case-insensitive filtering (`.contains()`) allocates strings on the heap unnecessarily, slowing down the application during searches. We can utilize an ASCII-only fast path by using `haystack.as_bytes().windows(len).any(|w| w.eq_ignore_ascii_case(needle_bytes))` to dramatically decrease allocations.
**Action:** When performing simple case-insensitive searches in loops, check if the query string is ASCII. If it is, use byte-wise windowed comparisons instead of converting strings to lowercase globally, only falling back for non-ASCII text.

## 2024-04-27 - [Correct Fallback Handling in Loop Filtering optimizations]
**Learning:** When trying to prevent `.to_lowercase()` string allocations inside a hot loop (like a case-insensitive search loop), do NOT initialize the fallback string (`search_query_lower`) conditionally to an empty string (`String::new()`). `n.to_lowercase().contains("")` evaluates to true, breaking the filter by matching all items with non-ASCII text.
**Action:** Always eagerly initialize the lowercase fallback string outside the loop (`let search_query_lower = search_query.to_lowercase();`) if you still intend to use `.contains(&search_query_lower)` as the fallback branch within the loop for strings that fail the `.is_ascii()` check.

## 2024-05-30 - [Optimize bulk space filter]
**Learning:** To reduce RwLock contention, batch filtering logic utilizing iterators directly inside the lock method avoids overheads of massive `Vec` capacity preallocations and iterative atomic locks.
**Action:** When migrating N loops on locked traits, use internal iterator callbacks inside a scoped read guard instead of pre-collecting into N-size Vectors to fetch them piecemeal.

## 2024-06-03 - [Refactored `filtered_room_list` to store indices]
**Learning:** In Rust UI apps that sync large lists of data (like Matrix rooms), avoid `Vec::clone` on large structs. We previously kept `filtered_room_list` as `Vec<RoomData>`, which caused massive `O(N)` heap allocations containing string clones every single keystroke.
**Action:** Changed `filtered_room_list` to store indices (`Vec<usize>`) referencing `room_list` instead. When rendering or applying filtering, just lookup `&room_list[idx]`. This reduced search/filter update overhead effectively.

## 2024-06-25 - [Optimize bulk space filter traversals]
**Learning:** Checking Space inclusions via upward `is_child_of_recursive` traversal required parsing string `room.id` into `RoomId` for every room in the app state on every keystroke, leading to `O(N * depth)` overhead and high allocation. Space hierarchy children are a mirror of parents.
**Action:** In `cosmic-ext-constellations`, to optimize bulk Space filtering, avoid iterative upward `SpaceHierarchy` tree traversals and string parsing (`RoomId::parse`) per room. Instead, precompute the descendants of the target space via a downward traversal (`get_descendants_strs`) into a `HashSet<&str>`, enabling `O(1)` allocations-free membership checks.

## 2024-07-28 - [Removed String clone in hot filtering function]
**Learning:** `update_filtered_rooms` used to clone the entire `search_query` String every time it was called. Because it is called on every keystroke during a search to update the UI, this led to frequent unnecessary heap allocations.
**Action:** When working with struct fields in hot functions like UI updates, access the fields by reference (e.g., `self.search_query.is_empty()`, `self.search_query.is_ascii()`, `self.search_query.to_lowercase()`) rather than cloning the entire String unnecessarily just to call getter methods on it.

## 2026-05-08 - [Optimization] Case-insensitive child filtering in Space settings
 **Learning:** In Rust UI applications, performing case-insensitive filtering in the view loop can be expensive due to repeated heap allocations from `.to_lowercase()`.
 **Action:** Implemented a fast-path ASCII optimization using byte-level comparison (`eq_ignore_ascii_case`) when both the filter query and the target string are ASCII, significantly reducing allocation overhead in the hot view loop.

## 2024-05-09 - [Reuse `Vec` allocations during list filtering]
**Learning:** In hot functions like `update_filtered_rooms` that run on every keystroke during a search, recreating vectors unconditionally via `Vec::new()` and `.collect()` causes frequent O(N) heap allocations.
**Action:** Use `std::mem::take` to retrieve the existing vectors from the application state, call `.clear()` to retain their capacity, and then populate them using `.extend(...)` instead of `.collect()`. This significantly reduces memory allocations during interactive search filtering.

## 2024-05-10 - [Optimization] Removed unnecessary haystack.is_ascii() check in string filtering
**Learning:** When performing byte-wise windowed substring matching (`windows(len).any(|w| w.eq_ignore_ascii_case(query_bytes))`) with a pure ASCII query, checking if the haystack is also purely ASCII (`haystack.is_ascii()`) is unnecessary. UTF-8 multi-byte characters consist entirely of bytes with the high bit set (>= 128), which can never spuriously match any ASCII byte (0-127).
**Action:** Remove `haystack.is_ascii()` preconditions when the query string is known to be pure ASCII. This allows the zero-allocation fast path to also process haystacks containing emojis or foreign characters without falling back to expensive `.to_lowercase()` heap allocations.

## 2024-05-13 - Redesigned Chat Layout for Density and Utility
**Learning:** Utilizing structural elements like `Row` and `Column` with standard `cosmic::widget::divider::vertical` avoids allocating new containers with custom styles that must be verified at runtime, improving render consistency in iced apps while achieving a "dense" look.

## 2024-05-14 - [Optimization] Avoid `.to_lowercase()` allocations in hot `view()` loops
**Learning:** In immediate-mode GUI frameworks like iced/libcosmic, `view()` methods are called continuously. Calling `.to_lowercase()` unconditionally on search query strings inside these methods causes severe, unnecessary heap allocations on every render frame, even when the search feature is inactive.
**Action:** When filtering views based on state variables, wrap string allocations (like `query.to_lowercase()`) inside a conditional block that verifies the active state first (e.g., `if self.is_search_active && !query.is_empty()`). This ensures the expensive allocation only happens when strictly required.

## 2024-05-14 - [Optimization] Unrolling first-byte match for ASCII substring search
**Learning:** The previous fast-path ASCII optimization using `haystack.as_bytes().windows(len).any(...)` is much faster than `.to_lowercase().contains()`, but still slow on long non-matching strings because `windows()` iterates every possible overlapping slice.
**Action:** For simple byte-wise string searches, manually extracting the first byte of the query, converting it to upper and lower case, and only performing full-slice `eq_ignore_ascii_case` when the first byte matches is significantly faster and removes the iterator overhead of `windows()`.

## 2024-05-15 - [Correct Fallback Initialization in String Filtering loops]
**Learning:** In Rust UI loops, when computing case-insensitive string fallbacks, avoid conditionally initializing to an empty string (`""`) or computing `.to_lowercase()` unconditionally. An empty string evaluates to true in `.contains()` checks, breaking filtering.
**Action:** Instead, use an `Option<String>` pattern like `let fallback = (!is_ascii).then(|| string.to_lowercase())` and pass it via `.as_deref()` to the search method. This ensures the expensive `to_lowercase()` allocation only happens if strictly necessary, without breaking filtering logic with dummy empty string variables.

## 2026-05-18 - [Safe Vec reuse with std::mem::take]
**Learning:** In Rust UI state management, when passing a reused `Vec` buffer via `std::mem::take` into a bulk update method that acquires a lock (e.g., `filter_in_space_bulk_sync`), clearing the buffer *before* acquiring the lock results in UI flicker and lost data if the lock fails.
**Action:** Always call `.clear()` on the buffer *inside* the success branch of the lock. This ensures that if the lock fails, the unmodified original `Vec` can be safely restored via reassignment, preventing UI flickering and preserving memory capacity.

## 2024-08-15 - [Avoid Message cloning in view loop]
**Learning:** `src/view/chat.rs` was constructing `Message::StartReply` and `Message::StartEdit` by cloning the entire `ConstellationsItem` struct for every message in the timeline on every render frame. Since `ConstellationsItem` contains vectors of markdown events and long strings, this resulted in severe `O(N)` heap allocation bottlenecks.
**Action:** In `iced` or `libcosmic` UI applications, avoid instantiating `Message` variants with fully cloned data structs (e.g., `Message::StartReply(item.clone())`) inside the `view()` render loop. Instead, pass a lightweight identifier (like `TimelineEventItemId`) and retrieve the full data from the application state during the `update()` phase to eliminate `O(N)` heap allocation bottlenecks per frame.

## 2024-08-15 - [Pass UserIds by reference in view loop]
**Learning:** `view_sender_info` previously accepted `Option<matrix_sdk::ruma::OwnedUserId>`, causing a `.clone()` allocation on `item.sender_id` for every message on every render frame.
**Action:** In Rust UI render methods (`iced`/`libcosmic`), when passing identifiers like `matrix_sdk::ruma::OwnedUserId` to helper functions, pass them as references (e.g., `Option<&UserId>`) rather than owned copies. If an owned copy is needed for a `Message` variant inside an `on_press` handler, call `.to_owned()` at the exact site of variant construction to defer allocation until interaction.

## 2024-05-18 - [Optimization] Avoid `to_lowercase` inside emoji filtering loop
**Learning:** `view/chat.rs` was calling `emoji.name().to_lowercase()` and `s.to_lowercase()` continuously inside a loop when searching for emojis. Since this UI rendering method is called often, this leads to heavy heap allocation overhead when filtering emojis via search.
**Action:** Replaced `.to_lowercase().contains(&query)` with `crate::contains_ignore_ascii_case()` helper for both emoji names and shortcodes. This drastically reduces heap allocations and provides a fast ASCII path without sacrificing user experience.

## 2024-05-20 - [Optimize Chat String Allocations]
**Learning:** In `src/view/chat.rs`, the rendering hot loop (specifically `view_item` and `view_thread_summary`) was unnecessarily allocating large `String` objects via `.to_string()`, `.clone()`, and `.truncate()` byte boundary methods for every single message on screen, every single frame. `let_chains` is also not stabilized so must be avoided.
**Action:** Pre-allocate fixed-size buffers (`String::with_capacity`), utilize `.as_str()` heavily for passing references rather than owned objects, and utilize `char_indices` for properly truncating unicode strings without O(N) operations. Use fallbacks or manual buffers if you need to extend lifetimes of references taken out of temporary inner scopes (like `thread_summary`).

## 2024-05-23 - [Optimization] Avoid `.to_string()` and `.clone()` allocations in emoji picker loops
**Learning:** In Rust UI applications, when mapping over static data sets (like the `emojis` crate) to generate a list of buttons in the `view()` render loop, dynamically allocating strings (e.g. `emoji.as_str().to_string()`) and cloning identifiers for the resulting `Message` variants introduces severe `O(N)` heap allocation overhead on every single frame.
**Action:** Use a unified `Message` variant that accepts `&'static str` references to the static data (e.g. `Message::EmojiPickerSelected(&'static str)`) inside the `view()` loop. Resolve context and perform actual `.to_string()` allocation inside the `update()` handler strictly upon interaction (`on_press`), completely removing per-frame heap allocations during rendering.
