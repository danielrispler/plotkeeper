#!/bin/sh
# v0 for #315: append each hook payload as one line to events.jsonl, which the panel tails.
jq -c . >> "$(dirname "$0")/events.jsonl"
