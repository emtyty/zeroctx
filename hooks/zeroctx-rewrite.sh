#!/usr/bin/env bash
# ZeroCTX PreToolUse hook for Claude Code — Bash tool interceptor
# __ZERO_PATH__ is replaced by `zero install` with the actual binary path

set -uo pipefail

# Find zero binary: embedded path → PATH → fail silently
ZERO="__ZERO_PATH__"
if [ ! -x "$ZERO" ] 2>/dev/null; then
  ZERO=$(command -v zero 2>/dev/null || command -v zero.exe 2>/dev/null || echo "")
  [ -z "$ZERO" ] && exit 0
fi

# Find jq: beside zero → PATH → fail silently
ZERO_DIR=$(dirname "$ZERO")
if [ -x "$ZERO_DIR/jq" ] || [ -x "$ZERO_DIR/jq.exe" ]; then
  JQ="$ZERO_DIR/jq"
  [ -x "$ZERO_DIR/jq.exe" ] && JQ="$ZERO_DIR/jq.exe"
else
  JQ=$(command -v jq 2>/dev/null || echo "")
  [ -z "$JQ" ] && exit 0
fi

INPUT=$(cat)
CMD=$("$JQ" -r '.tool_input.command // empty' <<< "$INPUT" 2>/dev/null) || exit 0
[ -z "$CMD" ] && exit 0

REWRITTEN=$("$ZERO" rewrite "$CMD" 2>/dev/null) || true
EXIT_CODE=$?

case $EXIT_CODE in
  0)
    [ -z "$REWRITTEN" ] && exit 0
    [ "$REWRITTEN" = "$CMD" ] && exit 0
    UPDATED_INPUT=$("$JQ" -c '.tool_input' <<< "$INPUT" | "$JQ" --arg cmd "$REWRITTEN" '.command = $cmd' 2>/dev/null)
    cat <<HOOK_EOF
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow",
    "permissionDecisionReason": "ZeroCTX auto-rewrite",
    "updatedInput": $UPDATED_INPUT
  }
}
HOOK_EOF
    ;;
  *) exit 0 ;;
esac
