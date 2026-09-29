#!/system/bin/sh
# PalmGuard control, used by the app, its Quick Settings tile and action.sh.
#   ctl.sh start     clear the disable flag and start the daemon
#   ctl.sh stop      set the disable flag and stop the daemon
#   ctl.sh restart   restart the daemon (needed after changing device names)
#   ctl.sh reload    re-read palmguard.conf without restarting
#   ctl.sh toggle    stop if running, else start
#   ctl.sh state     print running=0|1 enabled=0|1 gave_up=0|1
#   ctl.sh boot      start unless disabled (called by service.sh)
MODDIR=${0%/*}
DATA=/data/adb/palmguard
LOG=$DATA/palmguard.log

supervisor_alive() {
  [ -f "$DATA/supervisor.pid" ] && kill -0 "$(cat "$DATA/supervisor.pid")" 2> /dev/null
}

launch() {
  rm -f "$DATA/gave_up"
  supervisor_alive && return 0
  nohup sh "$MODDIR/supervise.sh" > /dev/null 2>&1 &
}

stop_daemon() {
  touch "$DATA/disable"
  pkill -x palmguard
  # Wait for the supervisor to notice so a quick start can't race it.
  i=0
  while supervisor_alive && [ $i -lt 30 ]; do sleep 0.1; i=$((i + 1)); done
}

mkdir -p "$DATA"
case "$1" in
  boot)
    [ -f "$LOG" ] && mv -f "$LOG" "$LOG.old"
    [ -f "$DATA/disable" ] || launch
    ;;
  start)
    rm -f "$DATA/disable"
    launch
    ;;
  stop)
    stop_daemon
    ;;
  restart)
    stop_daemon
    rm -f "$DATA/disable"
    launch
    ;;
  reload)
    pkill -HUP -x palmguard
    ;;
  toggle)
    if [ -f "$DATA/disable" ]; then "$0" start; else "$0" stop; fi
    ;;
  state)
    running=0; enabled=1; gave_up=0
    pidof palmguard > /dev/null && running=1
    [ -f "$DATA/disable" ] && enabled=0
    [ -f "$DATA/gave_up" ] && gave_up=1
    echo "running=$running"
    echo "enabled=$enabled"
    echo "gave_up=$gave_up"
    ;;
  *)
    echo "usage: ctl.sh start|stop|restart|reload|toggle|state" >&2
    exit 2
    ;;
esac
