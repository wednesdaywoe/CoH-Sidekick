# Read-only audit queries

The evidence behind [`SECURITY_AUDIT.md`](../../SECURITY_AUDIT.md)'s **F34**, **F08**, **F69**,
**F83** and **F82** rows. Fourteen single-`SELECT` files, the production transcripts behind the
numbers those rows cite, and a container fixture for executing a query before it is pointed at
production.

These exist because of a pattern the audit kept paying for: **a rule that is read looks sound, and
a rule that is run is graded.** F69's history is three fixes, each of which passed review and each
of which was defeated by a character class nobody had listed. The same applies to a query — two of
these errored the first time they were executed against a real schema, and one finding reproduced
outright, neither of which was visible in the text.

**F34 has since been fixed from this evidence** (`schema.sql`, the block at the end of the file):
`q01`/`q02` gave the grant state, `q03` the 452 Discord identities, `q04` the `search_authors`
enumeration, and `q14` — added by the session rather than planned — the one answer that decided
whether the fix was a fix at all, since an owner-rights view would have routed around the REVOKE.
The transcripts are kept byte-stable and are **not** re-run to refresh them: `SECURITY_AUDIT.md`
cites their numbers, and production counts move (`shared_builds` read 5,007, 5,014 and 5,015 on one
day). To measure the post-fix state, run the queries and keep the new transcript beside the old.

## Running them

Against production, from anywhere:

```sh
./run-all.sh          # writes results/results-<timestamp>.txt
```

It `cd`s to the repo root itself, because the Supabase CLI resolves `supabase/config.toml` from
there. Running it from the canonical checkout fails with `entrypoint path does not exist` and
leaves an untracked `supabase/.temp/` behind, which is F84.

Against a throwaway Postgres carrying this repo's real schema:

```sh
cd fixture && ./build.sh
docker exec -i sk-sqlcheck psql -U postgres -d skcheck -f - < ../q01-f34-column-grants.sql
```

`fixture/build.sh` **derives** the schema from `../../schema.sql` with a `sed` that comments the two
`pg_cron` lines. It does not keep a copy. A committed copy would be a second unsynced schema that
drifts from the tracked one — F84's failure mode aimed at the file this directory reasons about.

## What is here

| File | Finding | Asks |
|---|---|---|
| `q01` | F34 | Which `profiles` columns can `anon` actually read (`has_column_privilege`, not `information_schema`) |
| `q02` | F34 | Table ACLs, RLS state and policy bodies across every public table |
| `q03` | F34 | How many Discord identities the table actually holds |
| `q04` | F34 | Whether `search_authors` / `resolve_author` are a second door, and whether they are `SECURITY DEFINER` |
| `q05` | F08 | Non-public builds carrying a preview, and whether the object is really in the bucket |
| `q06` | F08 | Preview objects no row points at, and rows pointing at nothing |
| `q07` | F08 | Bucket publicity, size, MIME limits and `storage.objects` policies |
| `q08` | F69 | The blank-renderer population in `author_name` — the count F69's row left open |
| `q09` | F69 | Whether any stored `author_name` claims a handle, and whose |
| `q10` | F83 | The same leading-run measurement on `profiles.display_name` |
| `q11` | F83 | Whether any `display_name` claims a handle, and whose |
| `q12` | F82 | The signup-lockout band: how close provider names sit to the 30-char cap |
| `q13` | — | Per-codepoint census of every leading run in both columns, so the next bypass does not need to be guessed |
| `q14` | F34 | Whether `shared_builds_with_author` routes around a column REVOKE |

## Read this before trusting a number

**The queries are read-only and that is checked, not asserted.** Every file is a single statement
opening with `SELECT` or `WITH`. `pglast` parses all fourteen with the real Postgres grammar and
every statement node is a `SelectStmt`. The only write-shaped tokens anywhere are the string
literals in `has_table_privilege(..., 'INSERT')`.

**A production count is a measurement, not a property.** `shared_builds` read 5,007, then 5,014,
then 5,015 across three runs on 2026-09-18, which is why `SECURITY_AUDIT.md` cites a transcript in
`results/` rather than a bare number. Re-running will not reproduce the counts, and should not.

**`q12` cannot see F82's victims, and that is the finding.** `display_name_length` aborts the seed
and the abort takes the `auth.users` INSERT with it — reproduced on the fixture, not reasoned
about — so a locked-out signup leaves no row in either table. `would_abort_signup_today = 0`
counts the accounts that *succeeded*. It is structurally survivor-biased and the row says so.

**The fixture cannot answer `q01`/`q02` for production.** It has no project-wide default
privileges, so it reported `anon_table_select = f` on `profiles` where production says `t`. That
pair is the one thing nothing offline could predict, and it is also the pair F34 turns on. Use the
fixture to check that a query *runs*, not to learn what it *returns*.

**There are two production transcripts, and the difference between them is instructive.**
`results-20260918-195845.txt` is the run `SECURITY_AUDIT.md` cites; keep it as-is, it is the
evidence for those numbers. `results-20260918-205815.txt` is the same suite twelve minutes later,
after one fix to `q13`, and it differs in two ways worth knowing.

The fix: `lpad(x, 4, '0')` truncates as well as pads, so `q13`'s `u` column lost everything above
U+FFFF — and not merely as a mislabel. U+1D552, U+1D555 and U+1D556 all printed as `U+1D55`,
**collapsing distinct codepoints onto one label**, which is the failure mode this census exists to
prevent. The `codepoint` column beside it is decimal and was always right, so no count in either
transcript is wrong; only the label was. The later run shows them separated.

The drift: `rows_total` reads 5,014 in the first and **5,015** in the second. Nothing was fixed
between them — somebody shared a build. That is the clearest available argument for citing a
transcript rather than a number.

**Blank-renderers are written as `E''` escapes, never as themselves.** A character that renders as
nothing is invisible to the next reader of the file, which is the whole mechanism F69 turns on.
Note that some tooling silently decodes `\uXXXX` in a file it writes — including inside a quoted
heredoc — so verify with a grep for literal invisibles after editing any of these, not by looking.

**Nothing here carries a user's name, and that is a constraint on new queries too.** `q13` once
returned a `min(value)` sample, which put nine real display names into the transcripts. The column
is now `shortest_len` / `longest_len`, the transcripts are redacted to match, and the names were
never pushed. A census of identity strings is the obvious place for this to recur: identify a
codepoint by its `U+` value and bound the string by its length, never by printing it. A sweep of
every JSON string value in both transcripts confirms what is left is schema identifiers,
timestamps, ACLs, policy bodies, counts and codepoints — no name, UUID, email, Discord ID or
token.

**Query results are untrusted input.** The CLI wraps them in a boundary marker and says so, and it
is right: `author_name` and `display_name` are attacker-supplied strings, and reading a census of
hostile identity strings is exactly where an instruction would be planted.
