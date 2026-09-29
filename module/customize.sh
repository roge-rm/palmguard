# Magisk install script
[ "$ARCH" = "arm64" ] || abort "! PalmGuard is built for arm64 only"
[ -f "$MODPATH/bin/palmguard" ] || abort "! bin/palmguard missing - build it first (see README)"

DATA=/data/adb/palmguard
mkdir -p "$DATA"
if [ ! -f "$DATA/palmguard.conf" ]; then
  cp "$MODPATH/palmguard.conf" "$DATA/palmguard.conf"
  ui_print "- Wrote default config to $DATA/palmguard.conf"
else
  ui_print "- Keeping existing $DATA/palmguard.conf"
fi

# Give the virtual touchscreen the same device config and key layout as the
# real one (calibration, wake gestures), plus flags so Android treats it as
# the built-in screen.
find_stock() {
  for d in /odm/usr /vendor/usr /product/usr /system/usr; do
    [ -f "$d/$1" ] && { echo "$d/$1"; return; }
  done
}
mkdir -p "$MODPATH/system/usr/idc" "$MODPATH/system/usr/keylayout"
IDC_OUT="$MODPATH/system/usr/idc/palmguard_ts.idc"
IDC=$(find_stock idc/goodix_ts.idc)
if [ -n "$IDC" ]; then
  ui_print "- Basing device config on $IDC"
  grep -vE '^[[:space:]]*(device\.internal|touch\.deviceType)' "$IDC" > "$IDC_OUT"
else
  ui_print "- No stock goodix_ts.idc found, using defaults"
  : > "$IDC_OUT"
fi
{
  echo ""
  echo "# added by palmguard"
  echo "touch.deviceType = touchScreen"
  echo "device.internal = 1"
  grep -q 'touch.orientationAware' "$IDC_OUT" || echo "touch.orientationAware = 1"
} >> "$IDC_OUT"

KL=$(find_stock keylayout/goodix_ts.kl)
if [ -n "$KL" ]; then
  ui_print "- Copying key layout from $KL"
  cp "$KL" "$MODPATH/system/usr/keylayout/palmguard_ts.kl"
fi

set_perm_recursive "$MODPATH/system" 0 0 0755 0644
set_perm "$MODPATH/bin/palmguard" 0 0 0755
set_perm "$MODPATH/service.sh" 0 0 0755
set_perm "$MODPATH/action.sh" 0 0 0755
set_perm "$MODPATH/ctl.sh" 0 0 0755
set_perm "$MODPATH/supervise.sh" 0 0 0755
if [ -f "$MODPATH/system/app/PalmGuard/PalmGuard.apk" ]; then
  ui_print "- Installing the PalmGuard app (open it after reboot to grant root)"
fi
ui_print "- Reboot, then pull the pen out to start filtering"
