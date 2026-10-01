#!/usr/bin/env bash
set -euo pipefail

# Disposable configuration keeps validation independent of installed profiles.
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
skc_version=0.9.1
skc_release="https://github.com/humanfrontier/skillcatalog-releases/releases/download/v${skc_version}"

fail() { printf '%s\n' "$*" >&2; exit 1; }
[[ $# -eq 0 ]] || fail "Usage: bash .github/scripts/check-skillcatalog.sh"
[[ -n "${TMPDIR:-}" && -d "$TMPDIR" && -w "$TMPDIR" && -x "$TMPDIR" ]] ||
  fail "Set TMPDIR to an existing, writable task-owned directory."
[[ "$TMPDIR" = /* ]] || fail "TMPDIR must be an absolute path."
[[ -f "$repo_root/.skillcatalog/catalog/catalog.yaml" ]] || fail "Catalog is missing."

case "$(uname -s)/$(uname -m)" in
  Linux/x86_64)
    skc_asset="skc-v${skc_version}-x86_64-unknown-linux-gnu.tar.gz"
    skc_hash=a7ff9827f9d19c627052b63b389eb3d161f45f43675583e5a7378f1f918d6f64
    skc_member=skc
    ;;
  Darwin/arm64)
    skc_asset=SkillCatalog.app.tar.gz
    skc_hash=ffa1c8f6935d0be2ce3f1568451e2d566978e684c6bd1ffd309c57ba5865e97d
    skc_member=SkillCatalog.app/Contents/MacOS/skc
    ;;
  *) fail "SkillCatalog ${skc_version} publishes validators for Linux x86_64 and macOS arm64 only." ;;
esac

umask 077
skc_tmp="$(mktemp -d "$TMPDIR/type-history-skc.XXXXXXXX")"
trap 'rm -rf -- "$skc_tmp"' EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

curl --fail --silent --show-error --location \
  --output "$skc_tmp/$skc_asset" "$skc_release/$skc_asset"
(
  cd -- "$skc_tmp"
  if command -v sha256sum >/dev/null 2>&1; then
    printf '%s  %s\n' "$skc_hash" "$skc_asset" | sha256sum -c -
  else
    printf '%s  %s\n' "$skc_hash" "$skc_asset" | shasum -a 256 -c -
  fi
)
tar -xzf "$skc_tmp/$skc_asset" -C "$skc_tmp" "$skc_member"
skc_bin="$skc_tmp/$skc_member"
mkdir "$skc_tmp/config"
skc_actual="$("$skc_bin" --config-dir "$skc_tmp/config" --version)"
[[ "$skc_actual" = "skc $skc_version" ]] || fail "Unexpected validator: $skc_actual"
"$skc_bin" --config-dir "$skc_tmp/config" validate \
  --no-drift --path "$repo_root/.skillcatalog/catalog"
