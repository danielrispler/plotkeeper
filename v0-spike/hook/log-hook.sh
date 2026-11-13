#!/bin/sh
# Spike for danielrispler/Falinks#315: append each PreToolUse payload, timestamped, to pretooluse.log next to this script.
printf '%s %s\n' "$(date +%T)" "$(cat)" >> "$(dirname "$0")/pretooluse.log"
