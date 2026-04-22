#!/bin/bash
# ZeroCTX PostToolUse hook — compresses Glob and Grep tool outputs.
# Invoked by Claude Code after Glob/Grep tool calls.
# stdin:  {"tool_name":"Glob","tool_input":{...},"tool_response":{"output":"..."}}
# stdout: PostToolUse response JSON, or empty (pass through)

ZERO_PATH="__ZERO_PATH__"

# Check zero binary exists
if [ ! -x "$ZERO_PATH" ]; then
    exit 0  # pass through
fi

# Read full input
INPUT=$(cat)

# Extract tool_name
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // ""' 2>/dev/null)

case "$TOOL_NAME" in
    Glob|Grep)
        # Extract tool output
        TOOL_OUTPUT=$(echo "$INPUT" | jq -r '.tool_response.output // ""' 2>/dev/null)
        if [ -z "$TOOL_OUTPUT" ]; then
            exit 0  # no output, pass through
        fi

        # Count lines — only compress if large enough to be worth it
        LINE_COUNT=$(echo "$TOOL_OUTPUT" | wc -l)
        if [ "$LINE_COUNT" -lt 20 ]; then
            exit 0  # small result, pass through
        fi

        # Run ZeroCTX compression
        COMPRESSED=$(echo "$TOOL_OUTPUT" | "$ZERO_PATH" compress-output --tool "$TOOL_NAME" 2>/dev/null)
        EXIT_CODE=$?

        if [ $EXIT_CODE -eq 0 ] && [ -n "$COMPRESSED" ]; then
            echo "$COMPRESSED"
            exit 0
        fi
        exit 0  # pass through on error
        ;;
    *)
        exit 0  # unknown tool, pass through
        ;;
esac
