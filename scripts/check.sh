#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"

bash -n assets/shell/termi.bash packaging/*.sh scripts/*.sh
bash scripts/test-shell-integration.sh
if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate packaging/ai.clairos.termi.desktop
fi
if command -v appstreamcli >/dev/null 2>&1; then
    appstreamcli validate --no-net packaging/ai.clairos.termi.metainfo.xml
fi

cargo fmt --all -- --check
cargo check --all-targets --locked
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
