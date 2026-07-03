#!/bin/bash
# 1. Start target application in the background
cargo build
./target/debug/constellations &
APP_PID=$!
echo "Started app with PID $APP_PID"

# 2. Wait for application loading
sleep 15

# 3. Create screenshot storage
SCREENS_DIR="./scratch/screens"
mkdir -p "$SCREENS_DIR"
rm -f "$SCREENS_DIR"/*.png

# 4. Synthesize input to reach target UI state
for i in {1..6}; do
    wtype -P Tab -p Tab
    sleep 0.2
done
wtype -P Return -p Return
sleep 3

# 5. Capture screenshot of target state
cosmic-screenshot --interactive=false --notify=false --save-dir "$SCREENS_DIR"
mv "$SCREENS_DIR"/Screenshot_*.png "$SCREENS_DIR"/ui_state_target.png

# 6. Cleanup
kill $APP_PID
echo "Terminated application"
