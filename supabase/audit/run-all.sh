#!/usr/bin/env bash
# The read-only queries behind SECURITY_AUDIT.md's F34, F08, F69, F83 and F82 rows.
#
# Every file is a single SELECT. Verified three ways: pglast parses all of them with the real
# Postgres grammar and every statement node is a SelectStmt; all of them were executed against
# this repo's own schema.sql on a throwaway Postgres 15 (see fixture/); and the suite was run
# against production on 2026-09-18, which is the transcript in results/. Nothing here writes.
#
# Must run with the repo root as cwd: the Supabase CLI resolves supabase/config.toml from there,
# and invoking it from the canonical checkout fails with "entrypoint path does not exist" while
# silently leaving an untracked supabase/.temp/ behind -- F84's exact failure mode.
#
# HERE resolves BEFORE the cd. BASH_SOURCE is "./run-all.sh" when the script is invoked from its
# own directory, so dirname is "." -- computing it after the cd pointed it at the repo root, the
# q*.sql glob never expanded, and the results file landed as litter in the working tree.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE/../.." || exit 1
OUT="$HERE/results/results-$(date +%Y%m%d-%H%M%S).txt"
mkdir -p "$HERE/results"

{
  echo "# Sidekick read-only SQL session"
  echo "# $(date -Is)   project: $(cat supabase/.temp/project-ref 2>/dev/null || echo '(unknown)')"
  echo
  for f in "$HERE"/q*.sql; do
    printf '\n\n==================== %s ====================\n' "$(basename "$f")"
    sed -n '1,/^SELECT\|^WITH/p' "$f" | sed -n '/^--/p'
    echo
    supabase db query --linked -f "$f" 2>&1 | grep -v 'A new version of Supabase CLI\|We recommend updating'
    echo "(exit ${PIPESTATUS[0]})"
  done
} | tee "$OUT"

echo
echo "Saved to $OUT"
