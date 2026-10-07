#!/usr/bin/env bash
# Publish the web build to next.coh-sidekick.com from this machine: deploy-web.yml's two steps,
# run locally while the repo is private and hosted Actions minutes are out.
#
# Reads the three build settings from deploy-web.env.local at the repo root (git-ignored). Needs
# `npx wrangler login` once on the machine.
set -euo pipefail
cd "$(dirname "$0")/.."

# What goes live should be a commit someone can check out, not whatever is in the working tree.
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  echo "deploy-web: uncommitted changes; commit or stash them first" >&2
  exit 1
fi
git fetch -q origin main
if [ "$(git rev-parse HEAD)" != "$(git rev-parse origin/main)" ]; then
  echo "deploy-web: HEAD is not origin/main; publishing $(git rev-parse --short HEAD) anyway" >&2
fi

env_file=deploy-web.env.local
[ -f "$env_file" ] || { echo "deploy-web: $env_file missing" >&2; exit 1; }
set -a; . "./$env_file"; set +a
# An empty setting builds as configured-but-blank, which fails every call — see deploy-web.yml.
for v in SIDEKICK_SUPABASE_URL SIDEKICK_SUPABASE_ANON_KEY SIDEKICK_SENTRY_DSN; do
  [ -n "${!v:-}" ] || { echo "deploy-web: $v is empty in $env_file" >&2; exit 1; }
done
# A mangled key builds fine and fails every call with "Invalid API key"; ask the server first.
status=$(curl -s -o /dev/null -w '%{http_code}' -H "apikey: $SIDEKICK_SUPABASE_ANON_KEY" \
  "$SIDEKICK_SUPABASE_URL/auth/v1/settings")
[ "$status" = 200 ] || {
  echo "deploy-web: Supabase refused SIDEKICK_SUPABASE_ANON_KEY (HTTP $status); check $env_file" >&2
  exit 1
}

npm run build:web
npx -y wrangler@4.144.0 deploy
