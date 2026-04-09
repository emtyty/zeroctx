#!/usr/bin/env bash
# ZeroCTX PreToolUse hook for Claude Code — Read tool interceptor
# __ZERO_PATH__ is replaced by `zero install` with the actual binary path

set -uo pipefail

ZERO="__ZERO_PATH__"
if [ ! -x "$ZERO" ] 2>/dev/null; then
  ZERO=$(command -v zero 2>/dev/null || command -v zero.exe 2>/dev/null || echo "")
  [ -z "$ZERO" ] && exit 0
fi

ZERO_DIR=$(dirname "$ZERO")
if [ -x "$ZERO_DIR/jq" ] || [ -x "$ZERO_DIR/jq.exe" ]; then
  JQ="$ZERO_DIR/jq"
  [ -x "$ZERO_DIR/jq.exe" ] && JQ="$ZERO_DIR/jq.exe"
else
  JQ=$(command -v jq 2>/dev/null || echo "")
  [ -z "$JQ" ] && exit 0
fi

INPUT=$(cat)
FILE_PATH=$("$JQ" -r '.tool_input.file_path // empty' <<< "$INPUT" 2>/dev/null) || exit 0
[ -z "$FILE_PATH" ] && exit 0

# Only compress code files
EXT="${FILE_PATH##*.}"
case "$EXT" in
  rs|py|js|ts|jsx|tsx|cs|go|java|rb|cpp|c|h|hpp) ;;
  *) exit 0 ;;
esac

# Skip small files (< ~3KB)
[ -f "$FILE_PATH" ] && SIZE=$(wc -c < "$FILE_PATH" 2>/dev/null || echo "0") || exit 0
[ "$SIZE" -lt 3000 ] && exit 0

TEMP_PATH=$("$ZERO" compress-read "$FILE_PATH" 2>/dev/null) || exit 0
[ -z "$TEMP_PATH" ] && exit 0

UPDATED_INPUT=$("$JQ" -c '.tool_input' <<< "$INPUT" | "$JQ" --arg fp "$TEMP_PATH" '.file_path = $fp' 2>/dev/null)

cat <<HOOK_EOF
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow",
    "permissionDecisionReason": "ZeroCTX AST compression",
    "updatedInput": $UPDATED_INPUT
  }
}
HOOK_EOF
