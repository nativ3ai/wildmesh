#!/usr/bin/env bash
set -euo pipefail

REPO="nativ3ai/wildmesh"
TAG="v0.3.9"
METHOD="auto"
RUN_SETUP=1
WITH_HERMES=1
LAUNCH_AGENT=1
LOCAL_ONLY=0
COOPERATE=0
EXECUTOR_MODE=""
EXECUTOR_URL=""
EXECUTOR_MODEL=""
AGENT_LABEL=""
AGENT_DESCRIPTION=""
INTERESTS=()

usage() {
  cat <<'EOF'
WildMesh bootstrap installer

Usage:
  install.sh [options]

Options:
  --method <auto|brew|cargo>
  --no-setup
  --with-hermes <true|false>
  --launch-agent <true|false>
  --local-only
  --cooperate
  --executor-mode <disabled|builtin|openai_compat>
  --executor-url <url>
  --executor-model <model>
  --agent-label <label>
  --agent-description <description>
  --interest <interest>    May be repeated
  -h, --help

Examples:
  ./scripts/install.sh
  ./scripts/install.sh --agent-label NATIVEs-Mini --interest general --interest local-first
  ./scripts/install.sh --local-only --with-hermes false --launch-agent false
EOF
}

bool_flag() {
  case "${1:-}" in
    true|TRUE|1) echo 1 ;;
    false|FALSE|0) echo 0 ;;
    *)
      echo "invalid boolean value: $1" >&2
      exit 1
      ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --method)
      METHOD="${2:-}"
      shift 2
      ;;
    --no-setup)
      RUN_SETUP=0
      shift
      ;;
    --with-hermes)
      WITH_HERMES="$(bool_flag "${2:-}")"
      shift 2
      ;;
    --launch-agent)
      LAUNCH_AGENT="$(bool_flag "${2:-}")"
      shift 2
      ;;
    --local-only)
      LOCAL_ONLY=1
      shift
      ;;
    --cooperate)
      COOPERATE=1
      shift
      ;;
    --executor-mode)
      EXECUTOR_MODE="${2:-}"
      shift 2
      ;;
    --executor-url)
      EXECUTOR_URL="${2:-}"
      shift 2
      ;;
    --executor-model)
      EXECUTOR_MODEL="${2:-}"
      shift 2
      ;;
    --agent-label)
      AGENT_LABEL="${2:-}"
      shift 2
      ;;
    --agent-description)
      AGENT_DESCRIPTION="${2:-}"
      shift 2
      ;;
    --interest)
      INTERESTS+=("${2:-}")
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ -z "$AGENT_LABEL" ]]; then
  AGENT_LABEL="$(hostname -s 2>/dev/null || hostname)"
fi

install_with_brew() {
  if ! command -v brew >/dev/null 2>&1; then
    return 1
  fi
  brew tap nativ3ai/wildmesh >/dev/null
  if brew list wildmesh >/dev/null 2>&1; then
    brew upgrade wildmesh || true
  else
    brew install wildmesh
  fi
}

install_with_cargo() {
  if ! command -v cargo >/dev/null 2>&1; then
    return 1
  fi
  cargo install --git "https://github.com/${REPO}" --tag "${TAG}" wildmesh
}

case "$METHOD" in
  auto)
    install_with_brew || install_with_cargo || {
      echo "failed to install WildMesh with brew or cargo" >&2
      exit 1
    }
    ;;
  brew)
    install_with_brew || {
      echo "brew install path unavailable" >&2
      exit 1
    }
    ;;
  cargo)
    install_with_cargo || {
      echo "cargo install path unavailable" >&2
      exit 1
    }
    ;;
  *)
    echo "invalid method: $METHOD" >&2
    exit 1
    ;;
esac

if ! command -v wildmesh >/dev/null 2>&1; then
  echo "wildmesh binary not found after install" >&2
  exit 1
fi

if [[ "$RUN_SETUP" -eq 1 ]]; then
  setup_cmd=(wildmesh setup --agent-label "$AGENT_LABEL")
  if [[ -n "$AGENT_DESCRIPTION" ]]; then
    setup_cmd+=(--agent-description "$AGENT_DESCRIPTION")
  fi
  for interest in "${INTERESTS[@]}"; do
    setup_cmd+=(--interest "$interest")
  done
  if [[ "$LOCAL_ONLY" -eq 1 ]]; then
    setup_cmd+=(--local-only)
  fi
  if [[ "$WITH_HERMES" -eq 0 ]]; then
    setup_cmd+=(--with-hermes false)
  fi
  if [[ "$LAUNCH_AGENT" -eq 0 ]]; then
    setup_cmd+=(--launch-agent false)
  fi
  if [[ "$COOPERATE" -eq 1 ]]; then
    setup_cmd+=(--cooperate)
  fi
  if [[ -n "$EXECUTOR_MODE" ]]; then
    setup_cmd+=(--executor-mode "$EXECUTOR_MODE")
  fi
  if [[ -n "$EXECUTOR_URL" ]]; then
    setup_cmd+=(--executor-url "$EXECUTOR_URL")
  fi
  if [[ -n "$EXECUTOR_MODEL" ]]; then
    setup_cmd+=(--executor-model "$EXECUTOR_MODEL")
  fi
  "${setup_cmd[@]}"
else
  echo "WildMesh installed. Run 'wildmesh setup --agent-label \"$AGENT_LABEL\"' next."
fi
