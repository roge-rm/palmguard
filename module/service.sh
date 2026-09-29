#!/system/bin/sh
# Magisk late_start service: start the supervisor once boot has finished.
MODDIR=$(dirname "$(readlink -f "$0")")
APK=$MODDIR/system/app/PalmGuard/PalmGuard.apk
(
  until [ "$(getprop sys.boot_completed)" = "1" ]; do sleep 2; done
  sleep 5
  sh "$MODDIR/ctl.sh" boot
  # If the package manager didn't pick up the system app, install it as a
  # regular app so it still shows up in the launcher.
  if [ -f "$APK" ] && ! pm path ws.hunke.palmguard > /dev/null 2>&1; then
    echo "control app not installed as system app, installing it with pm" >> /data/adb/palmguard/palmguard.log
    cp "$APK" /data/local/tmp/palmguard.apk
    pm install -r /data/local/tmp/palmguard.apk >> /data/adb/palmguard/palmguard.log 2>&1
    rm -f /data/local/tmp/palmguard.apk
  fi
) > /dev/null 2>&1 &
