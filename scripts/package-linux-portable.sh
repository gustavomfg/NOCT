#!/usr/bin/env bash
# Writes NOCT_<version>_linux-x86_64.tar.gz: the release binary as `noct`, the icon, an example desktop entry and a short
# LEIA-ME with the system packages it needs. It is the portable Linux build for distributions where the AppImage is slow
# (its WebKit runs without hardware acceleration). Nothing is installed and nothing is signed. build-installers.yml runs it.
#
# Usage: scripts/package-linux-portable.sh <release binary> <icon.png> <output directory>
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <release binary> <icon.png> <output directory>" >&2
  exit 2
fi
binary="$1"
icon="$2"
out="$3"

root="$(cd "$(dirname "$0")/.." && pwd)"
version="$(jq -r '.version' "$root/crates/noct-app/tauri.conf.json")"
if [ -z "$version" ] || [ "$version" = "null" ]; then
  echo "the version is not in crates/noct-app/tauri.conf.json" >&2
  exit 1
fi
[ -f "$binary" ] && [ -x "$binary" ] || { echo "not an executable file: $binary" >&2; exit 1; }
[ -f "$icon" ] || { echo "not a file: $icon" >&2; exit 1; }

name="NOCT_${version}_linux-x86_64"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

mkdir "$work/$name"
cp "$binary" "$work/$name/noct"
chmod 755 "$work/$name/noct"
cp "$icon" "$work/$name/icon.png"
cp "$root/scripts/linux-portable/noct.desktop" "$root/scripts/linux-portable/LEIA-ME.txt" "$work/$name/"

mkdir -p "$out"
# Sorted entries, root ownership and numeric ids: the archive does not depend on the user who builds it.
tar --sort=name --owner=0 --group=0 --numeric-owner -C "$work" -czf "$out/$name.tar.gz" "$name"
ls -l "$out/$name.tar.gz"
