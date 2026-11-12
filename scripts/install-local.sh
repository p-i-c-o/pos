#!/usr/bin/env bash
# Build and install POS locally without sudo, leaving SQLite data untouched.
set -euo pipefail
usage() {
    cat <<'HELP'
Usage: scripts/install-local.sh [--prefix DIR] [--upgrade]

Build the POS client in release mode using the pinned Rust toolchain and
install "pos" to DIR/bin (default: $HOME/.local/bin).

  --prefix DIR    Installation root (binary placed at DIR/bin/pos)
  --upgrade       Explicitly replace an existing executable
  --help          Display this help

Requires Rust/Cargo. Does not use sudo, alter PATH or access user data.
HELP
}
upgrade=false
prefix="${POS_INSTALL_PREFIX-${HOME:?HOME is required}/.local}"
while (($#)); do
    case "$1" in
        --prefix)
            if (($# < 2)) || [[ -z "$2" ]]; then
                echo "POS installer: --prefix requires a directory" >&2
                exit 2
            fi
            prefix="$2"
            shift 2
            ;;
        --upgrade)
            upgrade=true
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "POS installer: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done
if [[ -z "$prefix" || "$prefix" != /* ]]; then
    echo "POS installer: installation prefix must be a nonempty absolute path" >&2
    exit 2
fi
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
bin_dir="$prefix/bin"
destination="$bin_dir/pos"
if [[ -L "$destination" || -d "$destination" ]]; then
    echo "POS installer: refusing to replace a symlink or directory: $destination" >&2
    exit 1
fi
if [[ -e "$destination" && "$upgrade" != true ]]; then
    echo "POS installer: destination already exists; pass --upgrade: $destination" >&2
    exit 1
fi
command -v cargo >/dev/null 2>&1 || {
    echo "POS installer: Cargo is required (install Rust via rustup)" >&2
    exit 1
}
# Build first; an unsuccessful build must never replace an installed binary.
(
    cd "$repo_root"
    cargo build --release --locked -p pos-cli --bin pos
)
target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"
if [[ "$target_dir" != /* ]]; then
    target_dir="$repo_root/$target_dir"
fi
source_binary="$target_dir/release/pos"
if [[ ! -x "$source_binary" ]]; then
    echo "POS installer: compiled binary not found: $source_binary" >&2
    exit 1
fi
mkdir -p -- "$bin_dir"
staging="$(mktemp "$bin_dir/.pos-install.XXXXXXXX")"
trap 'rm -f -- "$staging"' EXIT
install -m 0755 -- "$source_binary" "$staging"
"$staging" --version >/dev/null
# Link creation refuses an existing destination, including a race.
# --upgrade uses same-directory atomic rename only when explicitly authorized.
if [[ "$upgrade" == true ]]; then
    if [[ -L "$destination" || -d "$destination" ]]; then
        echo "POS installer: refusing to replace a symlink or directory: $destination" >&2
        exit 1
    fi
    mv -fT -- "$staging" "$destination"
else
    ln -- "$staging" "$destination"
    rm -f -- "$staging"
fi
trap - EXIT
printf 'Installed %s to %s\n' "$("$destination" --version)" "$destination"
printf 'Add %s to PATH if needed. No shell configuration was changed.\n' "$bin_dir"
