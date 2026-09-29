# PalmGuard

Palm and stray-touch rejection for passive-stylus phones, built for the
Moto G Stylus 2024 (`goodix_ts` touchscreen, `pen_detect` silo switch).

## Requirements

- Moto G Stylus 2024 (arm64, Android 10 or newer). It's the only phone
  PalmGuard has been tested on; see "Other phones" below.
- Root with [Magisk](https://github.com/topjohnwu/Magisk). The Action button
  on the module needs Magisk 28 or newer; everything else works without it.
- Nothing else: no Xposed/LSPosed, no extra apps.

## Install

1. Download `palmguard-magisk-vX.Y.Z.zip` from the
   [latest release](https://github.com/roge-rm/palmguard/releases/latest).
2. In Magisk, open Modules → Install from storage and pick the zip.
3. Reboot.
4. Open the PalmGuard app from the launcher and grant root when Magisk asks.
5. Pull the pen out. The app should say "Filtering" once your hand is off
   the screen.

Or over adb from a PC:

    adb push palmguard-magisk-vX.Y.Z.zip /sdcard/
    adb shell su -c 'magisk --install-module /sdcard/palmguard-magisk-vX.Y.Z.zip'
    adb reboot

To update, install the new zip the same way; your settings in
`/data/adb/palmguard/palmguard.conf` are kept. To uninstall, remove the
module in Magisk and reboot; that also deletes `/data/adb/palmguard`.

**Other phones.** PalmGuard finds the touchscreen and the pen silo switch by
input device name (`touch_device = goodix_ts`, `pen_device = pen_detect` in
the config). Another passive-stylus phone could work with those changed and
`pen_max_major` tuned using the app's contact-size card, but that's untested,
and the installer only copies the screen's calibration from a `goodix_ts`
device config. `adb shell getevent -lp` lists your device names.

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

## App

The module installs a PalmGuard control app and a Quick Settings tile. It
shows up in the launcher; if Android doesn't pick it up as a system app, the
module installs it as a regular app on the next boot. Open it once and grant
root in the Magisk prompt.

**Status.** An on/off switch (off = stock touchscreen, and it stays off
across reboots) with the current state:

| State     | Meaning                                                        |
|-----------|----------------------------------------------------------------|
| Filtering | Pen is out, palm rejection is active                           |
| Ready     | Pen is out, waiting for your hand to lift before taking over   |
| Standby   | Pen is docked, stock touchscreen                               |
| Off       | Switched off                                                   |
| Crashed   | The daemon exited 5 times in a row; check the log, switch on   |

"Restart daemon" is only needed after changing device names in the config.

**Contact sizes.** While the daemon runs, a live card shows the last and
largest pen contact, the last and smallest finger/hand contact, and how many
touches were passed or blocked. Touch the screen with the pen, a finger and
the side of your hand to see what each reports. If a non-pen contact was
small enough to count as the pen at the current limit, it warns you.

**Settings.** Sliders for the filter, applied live with "Save & apply" (the
config file is rewritten in place, comments kept, and the daemon reloads it):

| Setting                | Config key          | Default | What it does                                              |
|------------------------|---------------------|---------|-----------------------------------------------------------|
| Largest pen contact    | `pen_max_major`     | 80      | Contacts this size or smaller when they land are the pen  |
| Pen grows into hand    | `pen_regrade_major` | 150     | A pen contact that grows past this is cancelled as a hand |
| Grace after pen lifts  | `grace_ms`          | 500     | Ignore new touches this long after the pen lifts          |
| Touch hold-off         | `holdoff_ms`        | 40      | Delay finger touches so a hand landing first is caught    |
| Only while pen is out  | `require_pen_out`   | on      | Off filters all the time, even with the pen docked        |
| Verbose log            | `verbose`           | off     | Log every contact decision                                |

"Undo" drops unsaved changes and "Defaults" restores the values above.

**Log.** "View log" shows the end of `/data/adb/palmguard/palmguard.log`.

**Quick Settings tile.** Toggles PalmGuard on and off; its subtitle shows
Filtering, Standby, Off, Crashed, or why it can't work (No root, Not
installed).

The Magisk Action button toggles it too, and reports whether it started.

**Stylus-only drawing apps.** Since the pen is reported as a real stylus,
apps like Sketchbook work with "stylus only" turned on. With the pen docked
or PalmGuard off the pen is a plain finger again, so those apps ignore it.

## Build

    ./build.sh

On a PC this uses `cargo-ndk` (needs the Android NDK) and Gradle for the
app (needs the Android SDK; `app/local.properties` sets `sdk.dir`). In Termux
(`pkg install rust zip`) it builds natively. Without `zip`, `build.sh`
falls back to python3. Either way you get
`palmguard-magisk.zip`; install it as above. (Termux builds skip
the app.) To update just the app without reflashing:
`adb install -r app/app/build/outputs/apk/release/app-release.apk`.

`cargo test` replays real captures from the phone (`tests/data/`) through the
filter logic and runs on any Linux/macOS host.

## Check it's working

    su -c 'cat /data/adb/palmguard/palmguard.log'
    su -c 'dumpsys input' | grep -A5 palmguard_ts

The device config line should point at `palmguard_ts.idc`.

## Tuning

Watch decisions live without touching the real input (stop the service first
with the app's switch or the Magisk Action button):

    su -c '/data/adb/modules/palmguard/bin/palmguard --dry-run'

Each contact is logged with its size and whether it was treated as pen,
passed, held, or rejected. Edit `/data/adb/palmguard/palmguard.conf`, then
run `su -c sh /data/adb/modules/palmguard/ctl.sh reload`, or use the app's
settings and contact-size card instead.

## If touch ever misbehaves

1. Dock the pen. That releases the real touchscreen immediately.
2. The app's switch, the Quick Settings tile or the Magisk Action button
   stops the daemon (and keeps it off).
3. `touch /data/adb/palmguard/disable` keeps it off across reboots.
4. The daemon holds the grab through its own file handle, so if it crashes
   the stock touchscreen comes back automatically.
