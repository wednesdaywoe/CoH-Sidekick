#!/usr/bin/env bash
# Stand up a throwaway Postgres 15 carrying this repo's real schema, so a query can be EXECUTED
# before it is pointed at production. Executing rather than reading is what found two defects the
# eye missed: pg_policy.polcmd is type "char", so `text || polcmd` is ambiguous and needs ::text
# (q02 and q07 both errored on it), and F82 reproduced outright -- display_name_length fires and
# takes the auth.users INSERT with it, which is why F82's production count cannot see a victim.
#
# The schema is DERIVED from ../../schema.sql rather than copied. A committed copy would be a
# second unsynced schema that drifts from the tracked one -- F84's failure mode, pointed at the
# file this whole directory exists to reason about. The only edit is commenting the two pg_cron
# lines, which a stock postgres image has no extension for.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCHEMA="$HERE/../../schema.sql"
NAME="${NAME:-sk-sqlcheck}"
DB="${DB:-skcheck}"

[ -f "$SCHEMA" ] || { echo "no schema at $SCHEMA" >&2; exit 1; }

docker rm -f "$NAME" >/dev/null 2>&1 || true
docker run -d --name "$NAME" -e POSTGRES_PASSWORD=postgres postgres:15-alpine >/dev/null

# Wait INSIDE the container. A foreground host sleep is blocked in this harness, and a bare retry
# loop on the host spins instantly and reports "no response" before the server has opened a socket.
docker exec "$NAME" sh -c 'until pg_isready -U postgres >/dev/null 2>&1; do sleep 1; done'

# A fresh database each run. Loading twice into the same one dies on `relation "shared_builds"
# already exists`, and the second run's errors then look like query defects.
docker exec "$NAME" psql -U postgres -c "DROP DATABASE IF EXISTS $DB" >/dev/null
docker exec "$NAME" psql -U postgres -c "CREATE DATABASE $DB" >/dev/null

load() { docker exec -i "$NAME" psql -v ON_ERROR_STOP=1 -U postgres -d "$DB" -q; }

load < "$HERE/_bootstrap.sql"
sed -E 's@^(CREATE EXTENSION IF NOT EXISTS pg_cron;)@-- [fixture] \1@; /^SELECT cron\.schedule\(/,/^\);/ s@^@-- [fixture] @' "$SCHEMA" | load
load < "$HERE/_seed.sql"

echo "ready: docker exec -i $NAME psql -U postgres -d $DB -f - < ../q01-f34-column-grants.sql"
echo "teardown: docker rm -f $NAME"
