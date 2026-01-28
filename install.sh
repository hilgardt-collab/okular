#!/bin/bash
set -e

PREFIX="${PREFIX:-/usr/local}"
BINDIR="${PREFIX}/bin"
ICONDIR="${PREFIX}/share/icons/hicolor/256x256/apps"
DESKTOPDIR="${PREFIX}/share/applications"

echo "Installing Procular..."

# Check if running as root for system-wide install
if [ "$EUID" -ne 0 ] && [ "$PREFIX" = "/usr/local" ]; then
    echo "Note: Installing to $PREFIX requires root privileges."
    echo "Run with sudo, or set PREFIX=~/.local for user install."
    exit 1
fi

# Create directories
mkdir -p "$BINDIR"
mkdir -p "$ICONDIR"
mkdir -p "$DESKTOPDIR"

# Install binary
install -m 755 procular "$BINDIR/procular"
echo "Installed binary to $BINDIR/procular"

# Install icon
install -m 644 procular.png "$ICONDIR/procular.png"
echo "Installed icon to $ICONDIR/procular.png"

# Create desktop entry
cat > "$DESKTOPDIR/procular.desktop" << 'DESKTOP'
[Desktop Entry]
Name=Procular
Comment=Linux Process Monitor
Exec=procular
Icon=procular
Terminal=false
Type=Application
Categories=System;Monitor;GTK;
Keywords=process;monitor;system;cpu;memory;
DESKTOP
chmod 644 "$DESKTOPDIR/procular.desktop"
echo "Installed desktop entry to $DESKTOPDIR/procular.desktop"

# Update icon cache if available
if command -v gtk-update-icon-cache &> /dev/null; then
    gtk-update-icon-cache -f -t "${PREFIX}/share/icons/hicolor" 2>/dev/null || true
fi

echo ""
echo "Procular installed successfully!"
echo "Run 'procular' or find it in your application menu."
