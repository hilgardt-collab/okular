#!/bin/bash
set -e

PREFIX="${PREFIX:-/usr/local}"
BINDIR="${PREFIX}/bin"
ICONDIR="${PREFIX}/share/icons/hicolor/256x256/apps"
DESKTOPDIR="${PREFIX}/share/applications"

echo "Uninstalling Procular..."

# Check if running as root for system-wide uninstall
if [ "$EUID" -ne 0 ] && [ "$PREFIX" = "/usr/local" ]; then
    echo "Note: Uninstalling from $PREFIX requires root privileges."
    echo "Run with sudo, or set PREFIX=~/.local for user uninstall."
    exit 1
fi

# Remove files
if [ -f "$BINDIR/procular" ]; then
    rm -f "$BINDIR/procular"
    echo "Removed $BINDIR/procular"
fi

if [ -f "$ICONDIR/procular.png" ]; then
    rm -f "$ICONDIR/procular.png"
    echo "Removed $ICONDIR/procular.png"
fi

if [ -f "$DESKTOPDIR/procular.desktop" ]; then
    rm -f "$DESKTOPDIR/procular.desktop"
    echo "Removed $DESKTOPDIR/procular.desktop"
fi

# Update icon cache if available
if command -v gtk-update-icon-cache &> /dev/null; then
    gtk-update-icon-cache -f -t "${PREFIX}/share/icons/hicolor" 2>/dev/null || true
fi

echo ""
echo "Procular uninstalled successfully!"
