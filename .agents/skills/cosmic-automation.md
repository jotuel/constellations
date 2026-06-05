# COSMIC Desktop Automation and Screenshotting Skill

This skill provides the guidelines, commands, and tools needed to programmatically control and capture running applications in the COSMIC Desktop Environment (under Wayland).

## 1. Environment Verification
Before attempting automation, verify that the environment is indeed COSMIC running under Wayland:
```bash
echo "Desktop: $XDG_CURRENT_DESKTOP"  # Should output: COSMIC
echo "Session: $XDG_SESSION_TYPE"      # Should output: wayland
```

## 2. Screenshotting
COSMIC provides a native screenshot utility called `cosmic-screenshot`. To capture the screen programmatically without prompting the user or showing system notifications:

```bash
cosmic-screenshot --interactive=false --notify=false --save-dir /path/to/save
```
* Note: The utility will automatically name the screenshot file based on the current timestamp (e.g., `Screenshot_2026-06-05_13-55-11.png`) and save it to the specified `--save-dir`.

## 3. Input Emulation and Control (wtype)
Standard X11 tools (like `xdotool` or `import`) do not work on Wayland. Instead, COSMIC's compositor (`cosmic-comp`) implements the Wayland `virtual-keyboard-unstable-v1` protocol.

We use **`wtype`** to simulate keyboard presses, releases, modifiers, and character typing.

### Compiling `wtype` Locally
If `wtype` is not pre-installed and passwordless `sudo` is unavailable, you can compile it from source into a local directory:

```bash
# Clone the repository
git clone https://github.com/atx/wtype.git
cd wtype

# Generate Wayland protocol files
wayland-scanner client-header protocol/virtual-keyboard-unstable-v1.xml virtual-keyboard-unstable-v1-client-protocol.h
wayland-scanner private-code protocol/virtual-keyboard-unstable-v1.xml virtual-keyboard-unstable-v1-client-protocol.c

# Compile with gcc, linking Wayland client and xkbcommon libraries
gcc -O3 -o wtype main.c virtual-keyboard-unstable-v1-client-protocol.c $(pkg-config --cflags --libs wayland-client xkbcommon)
```

### Essential `wtype` Commands
* **Type text:**
  ```bash
  ./wtype "Hello, World!"
  ```
* **Simulate keypress and release (e.g. Enter):**
  ```bash
  ./wtype -P Return -p Return
  ```
* **Simulate a modifier combination (e.g. Copy with Ctrl + C):**
  ```bash
  ./wtype -M ctrl c -m ctrl
  ```
* **Specify custom delays (in milliseconds) between key events:**
  ```bash
  ./wtype -d 20 "Typing slowly..."
  ```

### Pointer Control and Scrolling (wlrctl)
If `wlrctl` is installed, it can be used to simulate pointer actions (clicks, movement, and scrolling) under Wayland.

* **Left click at current pointer position:**
  ```bash
  wlrctl pointer click left
  ```
* **Scroll vertically (positive value to scroll down, negative to scroll up):**
  ```bash
  wlrctl pointer scroll 150 0
  ```
* **Scroll horizontally:**
  ```bash
  wlrctl pointer scroll 0 150
  ```

## 4. Window Management (Closing Windows)
COSMIC DE uses a tiling/windowing model where `Super + Q` (or `Logo + Q`) is the default keyboard shortcut to close the active focused window.
To close the currently focused window programmatically:

```bash
# Press Logo/Super (-M logo), press Q (-P q), release Q (-p q), release Logo (-m logo)
./wtype -M logo -P q -p q -m logo
```

## 5. Standard Automation Pipeline Template
To run an automated test against a COSMIC GUI application:
1. Export environment variables:
   ```bash
   export WAYLAND_DISPLAY=wayland-1
   export DISPLAY=:1
   export XDG_RUNTIME_DIR=/run/user/1000
   ```
2. Spawn the application in the background (e.g., `cosmic-edit >/dev/null 2>&1 &`).
3. Pause (e.g. `sleep 2`) to let the window render and gain active focus.
4. Run `./wtype` commands to input data, navigate menus, or interact with components.
5. Capture the result using `cosmic-screenshot`.
6. Send `Super + Q` via `./wtype` to clean up and close the application.
