#!/usr/bin/env bash
# Benchmark the TEXT candidates through the installed llama.cpp binaries.
# Set REPS, PROMPT_TOKENS, GEN_TOKENS, and MW_TEXT_MODEL_DIR to override defaults.
set -euo pipefail

cd "$(dirname "$0")/.."

: "${REPS:=3}"
: "${PROMPT_TOKENS:=512}"
: "${GEN_TOKENS:=128}"
: "${MW_TEXT_MODEL_DIR:=$HOME/.cache/mini-world/models}"

command -v llama-bench >/dev/null || { echo "llama-bench not found" >&2; exit 1; }
command -v llama-cli >/dev/null || { echo "llama-cli not found" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq not found" >&2; exit 1; }

mkdir -p "$MW_TEXT_MODEL_DIR"
qwen35="$MW_TEXT_MODEL_DIR/Qwen3.5-0.8B-Q4_0.gguf"
gemma="$MW_TEXT_MODEL_DIR/gemma-3-270M-it-Q4_K_M.gguf"
qwen="${MW_TEXT_MODEL_DIR}/Qwen3-0.6B-Q4_0.gguf"

fetch() {
  local path="$1" url="$2"
  if [[ ! -s "$path" ]]; then
    echo "Downloading $(basename "$path")..." >&2
    curl --fail --location --retry 3 "$url" -o "$path"
  fi
}

fetch "$qwen35" "https://huggingface.co/unsloth/Qwen3.5-0.8B-GGUF/resolve/main/Qwen3.5-0.8B-Q4_0.gguf"
fetch "$gemma" "https://huggingface.co/lm-kit/gemma-3-270m-instruct-gguf/resolve/main/gemma-3-270M-it-Q4_K_M.gguf"
[[ -s "$qwen" ]] || { echo "Missing incumbent: $qwen" >&2; exit 1; }

work="$(mktemp -d "${TMPDIR:-/tmp}/mini-world-text.XXXXXX")"
trap 'rm -rf "$work"' EXIT

llama_version="$(llama-cli --version 2>&1)"
llama_version="${llama_version%%$'\n'*}"
printf '# TEXT bake-off (%s, llama.cpp %s, reps=%s, pp=%s, tg=%s)\n' \
  "$(uname -m)" "$llama_version" "$REPS" "$PROMPT_TOKENS" "$GEN_TOKENS"
printf '| model | backend | prompt tok/s | decode tok/s | file MiB | max RSS MiB | status |\n'
printf '|---|---:|---:|---:|---:|---:|---|\n'

bench() {
  local label="$1" path="$2" backend="$3" stem="$4"
  local json="$work/$stem.json" timing="$work/$stem.time" log="$work/$stem.log"
  local -a args=(llama-bench -m "$path" -p "$PROMPT_TOKENS" -n "$GEN_TOKENS" -r "$REPS" -o json)
  [[ "$backend" == "CPU" ]] && args+=(-ngl 0)

  if ! /usr/bin/time -l -o "$timing" "${args[@]}" >"$json" 2>"$log"; then
    printf '| %s | %s | — | — | — | — | llama.cpp cannot load |\n' "$label" "$backend"
    sed -n '1,8p' "$log" >&2
    return 0
  fi

  local prompt decode size rss
  prompt="$(jq -r '[.[] | select(.n_prompt > 0) | .avg_ts] | add / length' "$json")"
  decode="$(jq -r '[.[] | select(.n_gen > 0) | .avg_ts] | add / length' "$json")"
  size="$(awk -v bytes="$(stat -f '%z' "$path")" 'BEGIN { printf "%.1f", bytes / 1048576 }')"
  rss="$(awk '/maximum resident set size/ { printf "%.1f", $1 / 1048576 }' "$timing")"
  printf '| %s | %s | %.1f | %.1f | %s | %s | loaded |\n' "$label" "$backend" "$prompt" "$decode" "$size" "$rss"
}

bench 'Qwen3-0.6B Q4_0' "$qwen" Metal qwen-metal
bench 'Qwen3-0.6B Q4_0' "$qwen" CPU qwen-cpu
bench 'Qwen3.5-0.8B Q4_0 (hybrid)' "$qwen35" Metal qwen35-metal
bench 'Qwen3.5-0.8B Q4_0 (hybrid)' "$qwen35" CPU qwen35-cpu
bench 'Gemma3-270M it Q4_K_M' "$gemma" Metal gemma-metal
bench 'Gemma3-270M it Q4_K_M' "$gemma" CPU gemma-cpu

persona='You are Nara, a patient village baker. Speak in one terse in-character line.'
scene='The flour delivery is late. Reassure me. /no_think'
printf '\n## Dialogue smoke (identical OpenAI messages; Metal, seed=1, no thinking)\n'
port="${MW_TEXT_SMOKE_PORT:-8099}"
for model in qwen qwen35 gemma; do
  case "$model" in
    qwen) label='Qwen3-0.6B Q4_0'; path="$qwen" ;;
    qwen35) label='Qwen3.5-0.8B Q4_0 (hybrid)'; path="$qwen35" ;;
    gemma) label='Gemma3-270M it Q4_K_M'; path="$gemma" ;;
  esac
  log="$work/$model.server.log"
  response="$work/$model.response.json"
  llama-server -m "$path" -ngl 99 --port "$port" --host 127.0.0.1 -c 2048 \
    >"$log" 2>&1 &
  server_pid=$!
  ready=0
  for ((attempt = 0; attempt < 300; attempt++)); do
    if curl --fail --silent "http://127.0.0.1:$port/health" >/dev/null 2>&1; then
      ready=1
      break
    fi
    sleep 0.1
  done
  if ((ready == 0)); then
    printf '%s: llama.cpp cannot run dialogue smoke\n' "$label"
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
    continue
  fi
  request="$(jq -n --arg persona "$persona" --arg scene "$scene" '{
    messages: [
      {role: "system", content: $persona},
      {role: "user", content: $scene}
    ],
    temperature: 0.7,
    seed: 1,
    max_tokens: 48,
    chat_template_kwargs: {enable_thinking: false},
    stream: false
  }')"
  if curl --fail --silent "http://127.0.0.1:$port/v1/chat/completions" \
      -H 'Content-Type: application/json' -d "$request" >"$response"; then
    content="$(jq -r '.choices[0].message.content // empty' "$response" | tr '\n' ' ' | sed 's/[[:space:]]\+/ /g; s/^ //; s/ $//')"
    reasoning="$(jq -r '.choices[0].message.reasoning_content // empty' "$response" | tr '\n' ' ' | sed 's/[[:space:]]\+/ /g; s/^ //; s/ $//')"
    if [[ -n "$content" ]]; then
      printf '%s: %s\n' "$label" "$content"
    elif [[ -n "$reasoning" ]]; then
      printf '%s: reasoning-only response (no dialogue): %s\n' "$label" "$reasoning"
    else
      printf '%s: empty dialogue response\n' "$label"
    fi
  else
    printf '%s: llama.cpp cannot run dialogue smoke\n' "$label"
  fi
  kill "$server_pid" 2>/dev/null || true
  wait "$server_pid" 2>/dev/null || true
done
