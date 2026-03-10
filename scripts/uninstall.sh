#!/bin/bash
#
# Uninstall script for phomemo-macos CUPS driver.
#
# Usage: sudo ./scripts/uninstall.sh
#

set -e

echo "Removing Phomemo CUPS printers..."
for printer in $(lpstat -p 2>/dev/null | awk '/^printer Phomemo/ {print $2}'); do
  echo "  Removing $printer"
  lpadmin -x "$printer" 2>/dev/null || true
done

echo "Removing CUPS backends..."
rm -f /usr/libexec/cups/backend/phomemo-serial
rm -f /usr/libexec/cups/backend/phomemo-ble

echo "Removing CUPS filters..."
rm -f /usr/libexec/cups/filter/rastertopm02
rm -f /usr/libexec/cups/filter/rastertopm110
rm -f /usr/libexec/cups/filter/rastertopd30
rm -f /usr/libexec/cups/filter/phomemo-send
rm -f /usr/libexec/cups/filter/phomemo-ble

echo "Removing PPD files..."
rm -f /Library/Printers/PPDs/Contents/Resources/Phomemo-M02.ppd
rm -f /Library/Printers/PPDs/Contents/Resources/Phomemo-M02Pro.ppd
rm -f /Library/Printers/PPDs/Contents/Resources/Phomemo-M02S.ppd
rm -f /Library/Printers/PPDs/Contents/Resources/Phomemo-T02.ppd
rm -f /Library/Printers/PPDs/Contents/Resources/Phomemo-M110.ppd
rm -f /Library/Printers/PPDs/Contents/Resources/Phomemo-M120.ppd
rm -f /Library/Printers/PPDs/Contents/Resources/Phomemo-M220.ppd
rm -f /Library/Printers/PPDs/Contents/Resources/Phomemo-M421.ppd
rm -f /Library/Printers/PPDs/Contents/Resources/Phomemo-D30.ppd

echo "Forgetting package receipt..."
pkgutil --forget com.phomemo.macos.driver 2>/dev/null || true

echo "Restarting CUPS..."
launchctl kickstart -k system/org.cups.cupsd 2>/dev/null || true

echo "Done. Phomemo driver has been removed."
