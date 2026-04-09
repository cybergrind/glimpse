#!/usr/bin/env bash
# Lists all D-Bus clients that have called StartDiscovery on BlueZ adapters.
# BlueZ tracks discovery sessions per unique D-Bus sender name (:1.xx).
# Requires: busctl (systemd), or dbus-send + bluetoothctl

set -euo pipefail

echo "=== BlueZ adapter discovery state ==="
busctl call org.bluez / org.freedesktop.DBus.ObjectManager GetManagedObjects 2>/dev/null \
  | grep -oP '/org/bluez/hci\d+' | sort -u | while read -r adapter; do
    discovering=$(busctl get-property org.bluez "$adapter" org.bluez.Adapter1 Discovering 2>/dev/null | awk '{print $2}')
    echo "  $adapter: Discovering=$discovering"
done

echo ""
echo "=== D-Bus clients talking to org.bluez ==="
# Get all unique sender names from active connections to org.bluez
busctl tree org.bluez 2>/dev/null | head -5

echo ""
echo "=== Active discovery sessions (from bluetoothctl) ==="
if command -v bluetoothctl &>/dev/null; then
    echo "show" | bluetoothctl 2>/dev/null | grep -iE "Discovering|Name|Powered" || true
fi

echo ""
echo "=== Processes with open D-Bus connections matching bluez ==="
# Find processes that have org.bluez in their /proc/*/fd (unix sockets)
# More reliable: use dbus-monitor to see who is talking, but that's live.
# Instead, check who has the system bus socket open and recently called bluez:
for pid in /proc/[0-9]*/; do
    pid_num=$(basename "$pid")
    cmdline=$(tr '\0' ' ' < "${pid}cmdline" 2>/dev/null) || continue
    [ -z "$cmdline" ] && continue
    if echo "$cmdline" | grep -qi "bluetooth\|bluez\|blueman\|bluedevil\|gnome-bluetooth\|overskride"; then
        echo "  PID $pid_num: $cmdline"
    fi
done

echo ""
echo "=== D-Bus match rules on system bus (bluez watchers) ==="
# busctl monitor would show live traffic; instead list names that might be BT managers
for name in org.blueman.Mechanism org.blueman.Applet org.gnome.Bluetooth org.kde.bluedevil; do
    owner=$(busctl --system list 2>/dev/null | grep "$name" || true)
    if [ -n "$owner" ]; then
        echo "  $name: $owner"
    fi
done

echo ""
echo "Done."
