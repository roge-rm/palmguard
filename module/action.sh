#!/system/bin/sh
# Magisk "Action" button: toggle PalmGuard on/off without rebooting.
# Magisk runs this as `sh action.sh` from the module dir, so $0 has no path.
MODDIR=$(dirname "$(readlink -f "$0")")
if [ ! -f /data/adb/palmguard/disable ] && pidof palmguard > /dev/null; then
  sh "$MODDIR/ctl.sh" stop
  echo "PalmGuard stopped. Tap Action again to start it."
else
  sh "$MODDIR/ctl.sh" start
  sleep 1
  if pidof palmguard > /dev/null; then
    echo "PalmGuard running. Pull the pen out to begin filtering."
  else
    echo "PalmGuard failed to start. Log: /data/adb/palmguard/palmguard.log"
    tail -n 5 /data/adb/palmguard/palmguard.log 2> /dev/null
  fi
fi
