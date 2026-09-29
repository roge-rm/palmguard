#!/system/bin/sh
# Keeps the palmguard daemon running until $DATA/disable exists.
# Started by ctl.sh; do not run two of these (ctl.sh checks the pid file).
MODDIR=${0%/*}
DATA=/data/adb/palmguard
LOG=$DATA/palmguard.log
mkdir -p "$DATA"
echo $$ > "$DATA/supervisor.pid"

fails=0
while [ ! -f "$DATA/disable" ]; do
  VERBOSE=""
  [ -f "$DATA/verbose" ] && VERBOSE="-v"
  start=$(date +%s)
  "$MODDIR/bin/palmguard" -c "$DATA/palmguard.conf" -s "$DATA/status" $VERBOSE >> "$LOG" 2>&1
  echo "palmguard exited with status $?" >> "$LOG"
  if [ $(( $(date +%s) - start )) -lt 10 ]; then fails=$((fails + 1)); else fails=0; fi
  if [ "$fails" -ge 5 ]; then
    echo "palmguard exited 5 times in a row, giving up (start it again from the app)" >> "$LOG"
    echo "crashloop" > "$DATA/gave_up"
    break
  fi
  sleep 2
done
rm -f "$DATA/supervisor.pid" "$DATA/status"
