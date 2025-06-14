#!/usr/bin/env bash
set -e

REPO="dlahmad/sync-nudger"
BINARY="sync-nudger"

# Detect latest version
if [ -z "$VERSION" ]; then
    VERSION=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | awk -F'"' '/tag_name/ {print $4; exit}')
fi

# Detect OS and ARCH
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
    x86_64) ARCH="x86_64" ;;
    arm64|aarch64) ARCH="aarch64" ;;
    armv7l|armv7) ARCH="armv7" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

# Compose download URL
case "$OS" in
    darwin)
        case "$ARCH" in
            x86_64) FILE="$BINARY-${VERSION}-x86_64-apple-darwin.tar.gz" ;;
            aarch64) FILE="$BINARY-${VERSION}-aarch64-apple-darwin.tar.gz" ;;
            *) echo "Unsupported architecture: $ARCH for macOS"; exit 1 ;;
        esac
    ;;
    linux)
        case "$ARCH" in
            x86_64) FILE="$BINARY-${VERSION}-x86_64-unknown-linux-musl.tar.gz" ;;
            aarch64) FILE="$BINARY-${VERSION}-aarch64-unknown-linux-gnu.tar.gz" ;;
            armv7l|armv7) FILE="$BINARY-${VERSION}-armv7-unknown-linux-gnueabihf.tar.gz" ;;
            *) echo "Unsupported architecture: $ARCH for Linux"; exit 1 ;;
        esac
    ;;
    *)
        echo "Unsupported OS: $OS"
        exit 1
    ;;
esac

URL="https://github.com/$REPO/releases/download/$VERSION/$FILE"

# Create a temp directory for download and extraction
TMPDIR=$(mktemp -d)
cd "$TMPDIR"

trap 'cd /; rm -rf "$TMPDIR"' EXIT

echo "Downloading $URL ..."
curl -L "$URL" -o "$FILE"

echo "Extracting $FILE ..."
tar -xzf "$FILE"

# Find the binary in the extracted directory
FOUND=$(find . -type f -name "$BINARY" | head -n 1)
if [ -z "$FOUND" ]; then
    echo "Could not find $BINARY after extraction!"
    exit 1
fi

echo "Installing $BINARY to /usr/local/bin (may require sudo)"
sudo mv "$FOUND" /usr/local/bin/

cd /
rm -rf "$TMPDIR"

echo "Installed $BINARY version $VERSION!"
"$BINARY" --version