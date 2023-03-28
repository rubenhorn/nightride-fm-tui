#! /bin/sh

set -e

# Make sure we're in the right directory
cd "$(dirname "$0")"

# Install the binary
echo "Building and installing nightride binary..."
cargo install -q --path .

# Grab the icon
echo "Downloading icon..."
APP_DATA_DIR=~/.local/share/nightride/
mkdir -p "$APP_DATA_DIR"
curl -s https://nightride.fm/android-chrome-192x192.png > "$APP_DATA_DIR/icon.png"

# Validate the desktop entry
echo "Creating desktop entry..."
DESKTOP_FILE=nightride.desktop
TMP_DESKTOP_FILE="tmp.$DESKTOP_FILE"
# Expand home directory
sed "s|~|$HOME|g" "$DESKTOP_FILE" > "$TMP_DESKTOP_FILE"
echo "Validating desktop entry..."
desktop-file-validate "$TMP_DESKTOP_FILE"

# Install the desktop entry
echo "Installing desktop entry..."
APPS_DIR="$HOME/.local/share/applications"
mv "tmp.$DESKTOP_FILE" "$APPS_DIR/$DESKTOP_FILE"
# Refresh the desktop entry database
update-desktop-database "$APPS_DIR"

echo "Done!"
