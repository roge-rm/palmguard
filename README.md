# PalmGuard

Palm and stray-touch rejection for passive-stylus phones, built for the
Moto G Stylus 2024 (`goodix_ts` touchscreen, `pen_detect` silo switch).

## How it works

The Goodix driver reports the pen as a finger, but its contact size gives it
away: the pen measures 30-60 `TOUCH_MAJOR`, a fingertip about 480, and the
side of a hand 400-600. PalmGuard grabs the real touchscreen and re-emits a
filtered copy through a virtual `palmguard_ts` device:

- Pen contacts are labelled `MT_TOOL_PEN`, so apps see a real stylus.
- While the pen is touching, every other contact is ignored.
- Contacts that start within `grace_ms` after the pen lifts are ignored.
- Non-pen contacts are held back `holdoff_ms`, so a hand that lands just
  before the pen never reaches the app.
- A touch that already reached an app when the pen lands is cancelled
  (relabelled palm, so Android sends ACTION_CANCEL instead of a tap).

Filtering is only active while the pen is out of its silo. Docking the pen
releases the real touchscreen, so a docked pen always means stock touch.

## Build

    ./build.sh

On a PC this uses `cargo-ndk` (needs the Android NDK). In Termux
(`pkg install rust zip`) it builds natively. Either way you get
`palmguard-magisk.zip`. Flash it in Magisk and reboot.

`cargo test` replays real captures from the phone (`tests/data/`) through the
filter logic and runs on any Linux/macOS host.

## Check it's working

    su -c 'cat /data/adb/palmguard/palmguard.log'
    su -c 'dumpsys input' | grep -A5 palmguard_ts

The device config line should point at `palmguard_ts.idc`.

## Tuning

Watch decisions live without touching the real input (stop the service first
with the Magisk Action button):

    su -c '/data/adb/modules/palmguard/bin/palmguard --dry-run'

Each contact is logged with its size and whether it was treated as pen,
passed, held, or rejected. Edit `/data/adb/palmguard/palmguard.conf`, then
toggle the Action button twice. `touch /data/adb/palmguard/verbose` logs
decisions from the service too.

## If touch ever misbehaves

1. Dock the pen. That releases the real touchscreen immediately.
2. Magisk Action button stops the daemon (and keeps it off).
3. `touch /data/adb/palmguard/disable` keeps it off across reboots.
4. The daemon holds the grab through its own file handle, so if it crashes
   the stock touchscreen comes back automatically.
