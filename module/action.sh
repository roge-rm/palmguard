#!/system/bin/sh
# Magisk "Action" button: toggle PalmGuard on/off without rebooting.
MODDIR=${0%/*}
if [ -f /data/adb/palmguard/disable ]; then
  sh "$MODDIR/ctl.sh" start
  echo "PalmGuard started. Pull the pen out to begin filtering."
else
  sh "$MODDIR/ctl.sh" stop
  echo "PalmGuard stopped. Tap Action again to start it."
fi
