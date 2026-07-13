#!/usr/bin/env bash
# End-to-end mini-world demo: a live in-game day, a week of AFK fast-forward with
# a returning-player digest, and one latent conversation backfilled through the
# TEXT tier. Set MW_TEXT_LIVE=1 to render dialogue with the real backend instead
# of the offline mock. Exits 0 on success.
set -euo pipefail

cd "$(dirname "$0")/.."

AGENTS="${AGENTS:-50}"
SEED="${SEED:-1}"
DAY_TICKS=86400 # one in-game day (1 tick = 1 in-game second)

echo "== building (release) =="
cargo build --release --quiet -p mw-sim

BIN=target/release/mw-sim

echo "== 1) live headless: one in-game day ($DAY_TICKS ticks), $AGENTS agents =="
"$BIN" soak --ticks "$DAY_TICKS" --agents "$AGENTS" --seed "$SEED"

echo "== 2) AFK fast-forward: one week, then the returning-player digest =="
"$BIN" ff --days 7 --agents "$AGENTS" --seed "$SEED"

echo "== 3) latent dialogue: render the observed conversation, backfill a latent one =="
# Honors MW_TEXT_LIVE=1 (real TEXT backend); defaults to the offline mock.
"$BIN" dialogue --seed "$SEED"

echo "== demo complete =="
