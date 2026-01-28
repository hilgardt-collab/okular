#!/bin/bash
set -e

PREFIX="${PREFIX:-/usr/local}"
BINDIR="${PREFIX}/bin"
ICONDIR="${PREFIX}/share/icons/hicolor/256x256/apps"
DESKTOPDIR="${PREFIX}/share/applications"
PIXMAPDIR="${PREFIX}/share/pixmaps"

# Find the script's directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Find binary (check current dir, script dir, and target/release)
if [ -f "$SCRIPT_DIR/procular" ]; then
    BINARY="$SCRIPT_DIR/procular"
elif [ -f "$SCRIPT_DIR/target/release/procular" ]; then
    BINARY="$SCRIPT_DIR/target/release/procular"
elif [ -f "./procular" ]; then
    BINARY="./procular"
elif [ -f "./target/release/procular" ]; then
    BINARY="./target/release/procular"
else
    echo "Error: Could not find procular binary."
    echo "Run 'cargo build --release' first, or run from extracted archive."
    exit 1
fi

# Find icon
if [ -f "$SCRIPT_DIR/procular.png" ]; then
    ICON="$SCRIPT_DIR/procular.png"
elif [ -f "./procular.png" ]; then
    ICON="./procular.png"
else
    echo "Error: Could not find procular.png icon."
    exit 1
fi

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
mkdir -p "$PIXMAPDIR"

# Install binary
install -m 755 "$BINARY" "$BINDIR/procular"
echo "Installed binary to $BINDIR/procular"

# Install icon to multiple locations for compatibility
install -m 644 "$ICON" "$ICONDIR/procular.png"
echo "Installed icon to $ICONDIR/procular.png"
install -m 644 "$ICON" "$PIXMAPDIR/procular.png"
echo "Installed icon to $PIXMAPDIR/procular.png"
install -m 644 "$ICON" "$BINDIR/procular.png"
echo "Installed icon to $BINDIR/procular.png"

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
