## 2026-04-10 - Missing ARIA Labels for Icon-Only Buttons
**Learning:** Icon-only buttons (like the device rename '✏️' button) lack accessible text for screen readers and tooltips for mouse users, reducing accessibility and clarity.
**Action:** Always wrap icon-only buttons with `cosmic::widget::tooltip` providing a descriptive `text::body` and position (e.g., `Position::Top`) so users can understand their purpose.
## 2024-04-08 - Adding Helpful Empty States
**Learning:** I learned that an empty main content area when no room is selected leaves users confused about what to do next. Empty states are an important part of the UX to help guide users on how to use the interface.
**Action:** Implemented a friendly and informative empty state in the main content area when no room is selected to improve the overall UX.
## 2023-10-27 - Tooltips for Icon-Only Buttons
**Learning:** Icon-only buttons (like Space Switcher avatars/initials) lack context. Adding tooltips makes the app significantly more accessible and intuitive without cluttering the UI.
**Action:** When creating a list or grid of generic or custom icons, wrap them in `cosmic::widget::tooltip` so users know what they represent.
## 2026-04-12 - Semantic Widget Choices & Destructive Buttons
**Learning:** Using generic text buttons (`button::text`) for destructive actions like delete, leave, or ban fails to warn the user visually, leading to potential misclicks or anxiety. Additionally, icon-only buttons ("✏️") lack accessibility context.
**Action:** Always use libcosmic semantic widgets: `button::destructive` for irreversible or dangerous operations, and wrap any `button::icon` lacking descriptive text inside a `cosmic::widget::tooltip` to ensure clarity and accessibility for all users.
## 2024-05-18 - Replacing Emojis with Symbolic Icons
**Learning:** Using raw emojis for UI actions (like '✏️' or '🗑️') can break visual consistency with the system theme and lacks accessible tooltips if not explicitly wrapped.
**Action:** When designing UI elements in `libcosmic`, prefer semantic widgets over generic text buttons: use `button::icon` with standard freedesktop icons (e.g., `Named::new("document-edit-symbolic")`) and ensure they are wrapped in a `cosmic::widget::tooltip` to provide correct visual affordances and accessibility.
## 2026-04-14 - Disabled States for Primary Actions
**Learning:** Users can accidentally trigger empty or invalid actions (like sending an empty message or creating an unnamed room) if primary action buttons remain enabled when input is missing. This leads to phantom submissions and user confusion.
**Action:** Always disable primary action buttons (like 'Send' or 'Create') and block form submission when the required input fields are empty or contain only whitespace to provide immediate visual feedback and prevent invalid states.
## 2024-04-15 - Replace Text Buttons with Semantic Icons
**Learning:** Found that the "All Rooms" switch in the space switcher uses a text button containing a globe emoji (`"🌐"`). `libcosmic` specifies that semantic widgets should be preferred over generic text buttons with emojis. Using `button::icon` wrapped in `cosmic::widget::tooltip` provides better visual affordance, native styling consistency with the COSMIC desktop environment, and improved accessibility.
**Action:** Replaced the `"🌐"` text button with `button::icon(Named::new("applications-internet-symbolic"))` or `"network-workgroup-symbolic"` to align with the `libcosmic` UI guidelines.
## 2026-05-20 - Tooltips for Disabled Actions
**Learning:** While disabling primary actions (like 'Send' or 'Login') prevents invalid states and phantom submissions, it can leave users confused as to *why* the button is unresponsive if the requirements aren't visually obvious.
**Action:** When conditionally disabling primary action buttons, wrap them in a `cosmic::widget::tooltip` explaining the required state (e.g., "Type a message to send" or "Fill in all fields to login").
## 2026-05-22 - Missing Disabled State on Add Child
**Learning:** The "Add Child" button in the space settings did not have a disabled state when the input field was empty, violating our principles for primary actions.
**Action:** Always conditionally disable primary action buttons like "Add Child" and wrap them in a `cosmic::widget::tooltip` explaining the required state (e.g., "Enter a room or space ID to add").
## 2024-05-25 - Disabled States for Settings Actions
**Learning:** Users can encounter errors or be confused when "Change Password", "Invite", "Mod", or "Admin" buttons in the settings stay enabled despite the required input fields being empty or invalid (e.g., mismatched passwords).
**Action:** Disabled the primary action buttons in `src/settings/user.rs` and `src/settings/room.rs` when inputs are empty or invalid, and wrapped them in a `cosmic::widget::tooltip` explaining the missing required state.
## 2026-04-19 - Tooltips for Disabled Save Buttons
**Learning:** Users can easily become confused when 'Save' or 'Save Changes' buttons are disabled without context. The previous UI did not explicitly communicate that unsaved modifications are required to enable these action buttons.
**Action:** When conditionally disabling save buttons based on pending changes, wrap the disabled button in a `cosmic::widget::tooltip` (e.g., explaining "Make changes to save") to proactively guide the user on why the button is inactive.
## 2026-05-30 - Tooltips for Icon-Only Buttons
**Learning:** Icon-only buttons (like `button::icon` or `button::custom` wrapping icons) are frequently used across the application for actions like search, opening menus, creating spaces, and reacting. However, without text labels, they lack context for screen readers and new users.
**Action:** When creating or modifying icon-only buttons in `libcosmic`, always ensure they are wrapped in a `cosmic::widget::tooltip` with a descriptive `text::body` (e.g., "Search", "User Menu", "Add Reaction") and an appropriate `Position` (like `Position::Bottom` or `Position::Top`) to guarantee accessibility and immediate clarity.
## 2026-05-31 - Tooltips for Disabled Keyword Add Button
**Learning:** Checking for an empty input string to disable the 'Add' keyword button was missing a check for whitespace () and the disabled state was missing a tooltip explaining why it was disabled, which violates our rule on actionable disabled states.
**Action:** Use  to correctly block empty/whitespace input strings and always wrap disabled form submission buttons with an explanatory  (e.g. 'Enter a keyword to add').
## 2026-05-31 - Tooltips for Disabled Keyword Add Button
**Learning:** Checking for an empty input string to disable the 'Add' keyword button was missing a check for whitespace (`.trim()`) and the disabled state was missing a tooltip explaining why it was disabled, which violates our rule on actionable disabled states.
**Action:** Use `.trim().is_empty()` to correctly block empty/whitespace input strings and always wrap disabled form submission buttons with an explanatory `cosmic::widget::tooltip` (e.g. 'Enter a keyword to add').
## 2026-06-05 - Disabled States for Event Pinning
**Learning:** Primary actions like pinning an event can lead to phantom submissions or user confusion if left enabled when input is empty. Additionally, simply disabling the button without context leaves users confused about how to proceed.
**Action:** Always conditionally disable primary action buttons like "Pin" when the required input fields are empty or contain only whitespace. When doing so, wrap the disabled button in a `cosmic::widget::tooltip` explaining the requirement (e.g., "Enter an Event ID to pin") to provide actionable feedback.

## 2024-05-18 - Semantic Destructive Buttons
**Learning:** Destructive actions like removing attachments should use visually distinct `destructive` buttons rather than standard text buttons. This prevents accidental data loss and communicates the button's purpose clearly to the user, particularly aligning with COSMIC design guidelines.
**Action:** Always prefer semantic button types (`button::destructive`) over generic text buttons for actions that remove or delete data.

## 2024-05-19 - [Visual Feedback for Async Operations]
**Learning:** Loading states on interactive elements like "Load More" pagination buttons provide necessary visual feedback during async operations, preventing confusion and multiple clicks.
**Action:** When adding async action triggers, always consider disabling the button and modifying its text or displaying a spinner to reflect the busy state.

## 2026-06-10 - Tooltips for Disabled Action Buttons
**Learning:** Action buttons like "Ignore User", "Send" or "Add" for forms can lead to user confusion if they are conditionally disabled without a tooltip explaining why.
**Action:** When conditionally disabling primary action buttons based on empty required inputs, always wrap them in a `cosmic::widget::tooltip` explaining the missing requirement (e.g., "Enter an email address to add").

## 2026-06-10 - Avoiding Actionless Buttons for Decorative Icons
**Learning:** Using `button::icon` for purely decorative icons (like the medium icons in a list) creates an unclickable disabled button. This creates a misleading disabled state for screen readers and mouse users.
**Action:** Always use `cosmic::widget::icon::from_name(icon)` instead of an actionless `button::icon` for decorative or static icons to ensure proper accessibility.

## 2024-05-04 - [Empty State Visual Polish]
**Learning:** When lists or detail views are empty (like having no room selected), purely text-based empty states feel barren. Adding a simple, relevant icon makes the empty state much more pleasant and helps anchor the layout visually without distracting from the UI.
**Action:** When creating empty states, include a relevant icon using `cosmic::widget::icon::from_name(...)` sized appropriately (e.g., 64px) above the title to improve the visual hierarchy and overall feel.
## 2024-05-13 - Redesigned Chat Layout for Density and Utility
 **Learning:** When displaying high-density threads in a side-by-side layout, using pure borders/dividers rather than background colors or complex shadows (like Card styles) keeps the design functional and avoids clashing with user themes.
 **Action:** Swapped custom hardcoded emojis for a 20-item standard layout and shifted from vertical flow to a dense three-pane structure (timeline + threads) with rounded message bubbles via `.max_width(600)`. Removed complex `Card` closures to retain maximum compatibility with raw standard styling.
## 2026-05-15 - Adding Helpful Empty States
**Learning:** Hardcoded empty states (like 'No room selected') can be visually barren and not fully accessible to international users since they are not localized. It's important to use `crate::fl!()` for translating text.
**Action:** Replaced hardcoded empty state strings and disabled action tooltips with localized strings using `crate::fl!()` to ensure users of all languages receive clear, actionable feedback.
## 2026-06-25 - Localized UI Texts
**Learning:** Hardcoding strings like "Loading keywords..." directly into widget initializations (like `text::body("...")`) causes internationalization issues and accessibility barriers.
**Action:** When adding new UI text, empty states, tools tips, and buttons in libcosmic applications, use the `crate::fl!("key-name")` macro and add the corresponding key/value pair into `res/i18n/en/cosmic_ext_constellations.ftl` instead of hardcoding strings in the UI code.
## 2024-05-18 - Localize Tooltip Strings
**Learning:** Found multiple instances where tooltips, particularly those for UI action buttons like 'User Menu' or 'Room Settings', used hardcoded strings instead of localizable ones. This violates our principle of accessibility and internationalization.
**Action:** When creating or modifying tooltips in the UI, always use `crate::fl!("key")` and define the corresponding translated string in `res/i18n/en/cosmic_ext_constellations.ftl` rather than hardcoding English strings.

## 2024-05-18 - [Add Empty State to Ignored Users]
**Learning:** Empty lists without a "no items" message can confuse users into thinking the list is still loading or broken, especially right after a loading spinner disappears. Adding a localized empty state message provides clear feedback that the action completed successfully but yielded no results.
**Action:** When implementing any list UI (like blocked users, ignored users, rooms), always include a localized empty state message using `crate::fl!("...")` when the underlying data structure is empty and not actively loading.
## 2024-05-19 - Localized UI Fallbacks and Labels
**Learning:** Found that some form labels, fallback UI strings (like 'Unknown Room'), and instructional tooltips (like 'Enter a room name to create') were hardcoded in the Sidebar. Hardcoding UI strings negatively impacts accessibility and user experience for international users relying on translated interfaces.
**Action:** When adding form labels, instructional tooltips, and fallback UI names, always use `crate::fl!("key-name")` and ensure the keys exist in `res/i18n/en/cosmic_ext_constellations.ftl` to properly support localization.

## 2026-06-25 - Extracted Hardcoded Strings to Translations
**Learning:** Found several English strings hardcoded directly into the user settings and chat UI. Hardcoding UI strings negatively impacts accessibility and user experience for international users relying on translated interfaces or screen readers.
**Action:** When adding form labels, status messages, and other UI strings, always use `crate::fl!("key-name")` and ensure the keys exist in `res/i18n/en/cosmic_ext_constellations.ftl` to properly support full localization.
## 2026-05-25 - Tooltip for Icon-Only Close Button\n**Learning:** Icon-only close buttons lacking tooltips can fail to provide necessary context for screen readers and certain users, hurting accessibility.\n**Action:** When adding or modifying a standalone, icon-only button like `button::icon`, always wrap it in a `cosmic::widget::tooltip` with a descriptive `text::body` using localized string keys to ensure accessibility and clarity.
## 2026-05-26 - Icon Button Accessibility
**Learning:** Icon-only buttons without tooltips are a common accessibility issue. Wrapping them in a `cosmic::widget::tooltip` with localized text significantly improves the experience for both mouse and screen reader users.
**Action:** When adding or reviewing icon-only buttons in `libcosmic` interfaces, ensure they are always wrapped in a tooltip using `cosmic::widget::tooltip` and a corresponding translation key.
## 2026-06-25 - Tooltips for Disabled Action Buttons in User Settings
**Learning:** Found several primary action buttons (like linking an email/phone, ignoring a user, or deactivating an account) in User Settings that were left enabled even when the required input fields were empty. This allows for phantom submissions and user confusion.
**Action:** Always conditionally disable primary action buttons when the required input fields are empty or consist only of whitespace. Wrap the conditionally disabled buttons in a `cosmic::widget::tooltip` with an explanatory message (e.g., "Enter a user ID to ignore") to provide context.

## 2026-06-26 - Add Search Icon to Empty States
**Learning:** Empty states, especially those indicating no results (e.g., "No matches found." in search), can look barren if they consist only of text. Adding an associated icon (like `edit-find-symbolic`) and using localized text makes the empty state more appealing and accessible.
**Action:** When designing or updating an empty state, particularly for search results or filtering, wrap the state in a `Column`, add an appropriate icon (e.g., `cosmic::widget::icon::from_name("edit-find-symbolic").size(64)`), and use `crate::fl!("key-name")` for the text to improve the visual hierarchy and internationalization.

## 2026-06-27 - Icon-Only Destructive Buttons
**Learning:** Destructive actions like removing a keyword or leaving a call were using standard `button::icon`, which fails to warn the user visually. Since `button::destructive` expects a string, destructive icon-only buttons need a custom approach.
**Action:** To create a destructive icon-only button, use `button::custom(cosmic::widget::icon::from_name("...")).class(cosmic::theme::Button::Destructive)` instead of `button::icon`.
