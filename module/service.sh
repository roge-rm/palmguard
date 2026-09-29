#!/system/bin/sh
# Magisk late_start service: start the supervisor once boot has finished.
MODDIR=${0%/*}
(
  until [ "$(getprop sys.boot_completed)" = "1" ]; do sleep 2; done
  sleep 5
  sh "$MODDIR/ctl.sh" boot
) > /dev/null 2>&1 &
