#!/usr/bin/env bash
# Takes ScreenshotTest's pictures on the one device adb sees, and copies them
# to <out>/android-phone or <out>/android-tablet (screenshots.yml, and
# tools/screenshot-compare to set them beside the iPhone's and iPad's).
#
#   ./gradlew :app:assembleDebug :app:assembleDebugAndroidTest -Ptill.rust.abis=x86_64
#   DEVICE_NAME="Pixel 7" scripts/screenshots.sh <out> [--tablet]
#
# Run from android/. A tablet is shot standing up and on its side.
# DEVICE_NAME names the device on the page; else Android's model name.
set -euo pipefail

out="${1:?the folder for the screenshots}"
tablet=false
[ "${2:-}" = "--tablet" ] && tablet=true
adb="${ADB:-adb}"
app=com.monokulo.till.debug

"$adb" wait-for-device
"$adb" install -r -g app/build/outputs/apk/debug/app-debug.apk
"$adb" install -r -g app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk

# No other app's "keeps stopping" over the pictures: a fresh emulator's Digital Wellbeing often crashes.
"$adb" shell pm disable-user --user 0 com.google.android.apps.wellbeing > /dev/null 2>&1 || true
"$adb" shell settings put global show_first_crash_dialog 0
"$adb" shell settings put secure show_first_crash_dialog_dev_option 0
"$adb" shell am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS > /dev/null 2>&1 || true
"$adb" shell input keyevent KEYCODE_HOME

# The same status bar in every picture: 9:41, full battery and signal, no notifications.
"$adb" shell settings put global sysui_demo_allowed 1
demo() { "$adb" shell am broadcast -a com.android.systemui.demo -e command "$@" > /dev/null; }
demo enter
demo clock -e hhmm 0941
demo battery -e level 100 -e plugged false
demo network -e wifi show -e level 4 -e mobile show -e datatype none -e level 4
demo notifications -e visible false

shoot() {
    "$adb" shell settings put system accelerometer_rotation 0
    "$adb" shell settings put system user_rotation "$1"
    sleep 3
    local result
    result="$("$adb" shell am instrument -w -e screenshots on -e device "'${DEVICE_NAME:-}'" \
        -e class com.monokulo.till.ScreenshotTest \
        "$app.test/androidx.test.runner.AndroidJUnitRunner")"
    echo "$result"
    grep -q '^OK (' <<< "$result"
}

# The test reads which way up the device is and shoots that orientation's screens.
shoot 0
if $tablet; then
    shoot 1
fi

mkdir -p "$out"
"$adb" exec-out run-as "$app" tar -cf - -C files/screenshots . | tar -xf - -C "$out"
find "$out" -name '*.png' | wc -l | xargs echo "screenshots:"
