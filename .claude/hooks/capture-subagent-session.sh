#!/usr/bin/env bash
# SubagentStop hook — writes agent_id into the matching in-progress phase manifest entry,
# enabling /compound's Phase Synthesizer to locate subagent transcripts.
#
# Requires Claude Code >= v2.0.42 (agent_id + agent_transcript_path in SubagentStop input).
# Silent no-op on older versions or when no phase manifest is active.

set -euo pipefail

INPUT=$(cat)

# Parse each field separately to handle spaces in paths correctly.
# Stderr flows naturally to the hook error log; || true prevents set -e exit on bad JSON.
AGENT_ID=$(printf '%s' "$INPUT" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    v = d.get('agent_id')
    print(v if isinstance(v, str) and v else '', end='')
except Exception:
    pass
" || true)

AGENT_TRANSCRIPT=$(printf '%s' "$INPUT" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    print(d.get('agent_transcript_path', ''), end='')
except Exception:
    pass
" || true)

CWD=$(printf '%s' "$INPUT" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    print(d.get('cwd', ''), end='')
except Exception:
    pass
" || true)

# agent_id was added in v2.0.42 — exit silently on older versions
[[ -z "$AGENT_ID" || -z "$CWD" ]] && exit 0

CLAUDE_DIR="$CWD/.claude"
[[ -d "$CLAUDE_DIR" ]] || exit 0

ATOMIC_WRITE="${HOME}/.claude/templates/bin/atomic-json-write.sh"
[[ -x "$ATOMIC_WRITE" ]] || exit 0

# Collect in-scope phase manifests (sorted for deterministic ordering)
MANIFESTS=()
while IFS= read -r -d '' f; do
  MANIFESTS+=("$f")
done < <(find "$CLAUDE_DIR" -maxdepth 1 -name "phase-*-manifest.json" -print0 2>/dev/null \
  | sort -z)

[[ ${#MANIFESTS[@]} -eq 0 ]] && exit 0

# Extract phase ID from the subagent transcript. Phase subagent prompts always reference
# their manifest as ".claude/phase-<id>-manifest.json". Anchoring on the .claude/ prefix
# avoids false matches from doc text that mentions other phases. We take the most-frequent
# match (uniq -c) because the prompt references it many times while incidental doc mentions
# appear once. Uses -m1 on find to stop after first line, then reads full for frequency.
PHASE_HINT=""
if [[ -n "$AGENT_TRANSCRIPT" && -f "$AGENT_TRANSCRIPT" ]]; then
  PHASE_HINT=$(grep -oE '\.claude/phase-[0-9A-Za-z_.-]+-manifest\.json' \
      "$AGENT_TRANSCRIPT" 2>/dev/null \
    | sed 's|\.claude/phase-||;s|-manifest\.json$||' \
    | sort | uniq -c | sort -rn \
    | awk 'NR==1{print $2}' || true)
fi

# When multiple manifests are present and no hint could be extracted, refuse to guess.
if [[ -z "$PHASE_HINT" && ${#MANIFESTS[@]} -gt 1 ]]; then
  echo "capture-subagent-session: multiple manifests and no phase hint — skipping" >&2
  exit 0
fi

# Temp file used to signal a successful update out of the flock subshell.
SIGNAL=$(mktemp)
trap 'rm -f "$SIGNAL"' EXIT

for MANIFEST in "${MANIFESTS[@]}"; do
  LOCKFILE="${MANIFEST}.lock"

  # Serialise concurrent hook instances on this manifest to prevent lost-update races
  # when two subagents finish near-simultaneously.
  (
    flock -x 9

    UPDATED=$(python3 - "$MANIFEST" "$AGENT_ID" "$PHASE_HINT" << 'PYEOF'
import sys, json

manifest_path, agent_id, phase_hint = sys.argv[1], sys.argv[2], sys.argv[3]

try:
    with open(manifest_path) as f:
        m = json.load(f)
except Exception as e:
    print(f"capture-subagent-session: failed to parse {manifest_path}: {e}", file=sys.stderr)
    sys.exit(0)

# Skip manifests not matching the phase hint (if we have one)
if phase_hint and str(m.get('phase_id', '')) != phase_hint:
    sys.exit(0)

# Only act on active phases; ignore crashed or completed manifests to prevent
# stale in_progress entries from grabbing session IDs of unrelated subagents.
if m.get('status') != 'in_progress':
    sys.exit(0)

try:
    subagents = m.get('subagents', [])
    if not isinstance(subagents, list):
        raise TypeError(f"subagents field is {type(subagents).__name__}, expected list")
    candidates = [
        s for s in subagents
        if isinstance(s, dict)
        and s.get('session_id') is None
        and s.get('status') == 'in_progress'
    ]
except Exception as e:
    print(f"capture-subagent-session: error scanning candidates: {e}", file=sys.stderr)
    sys.exit(0)

if not candidates:
    sys.exit(0)

# Only assign when unambiguous. Work units run sequentially in /phase so there is
# normally exactly one candidate. Multiple candidates means parallel spawning or a
# crash left stale entries — skip rather than risk assigning to the wrong unit.
if len(candidates) != 1:
    print(
        f"capture-subagent-session: {len(candidates)} in-progress candidates in"
        f" {manifest_path} — skipping (ambiguous)",
        file=sys.stderr,
    )
    sys.exit(0)

candidates[0]['session_id'] = agent_id
print(json.dumps(m))
PYEOF
    )

    if [[ -n "$UPDATED" ]]; then
      "$ATOMIC_WRITE" "$MANIFEST" "$UPDATED"
      printf 'updated' > "$SIGNAL"
    fi

  ) 9>"$LOCKFILE"

  # Stop after the first successful manifest update
  [[ "$(cat "$SIGNAL" 2>/dev/null)" == "updated" ]] && break
done
