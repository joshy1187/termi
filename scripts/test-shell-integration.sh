#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
TERM_PROGRAM=not-termi

# shellcheck source=../assets/shell/termi.bash
source "$ROOT/assets/shell/termi.bash"

encoded_path=$(__termi_percent_encode_path $'/tmp/a b#?\e]\a')
[[ $encoded_path == '/tmp/a%20b%23%3F%1B%5D%07' ]]

safe_title=$(__termi_safe_title $'safe\e]2;bad\a\n')
[[ $safe_title == 'safe%1B]2;bad%07%0A' ]]

printf 'Shell integration checks passed.\n'
