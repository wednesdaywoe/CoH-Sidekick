#!/usr/bin/env python3
"""No crate's `target/` is tracked, and every one of them is ignored.

**Why this exists.** F87. `crates/app/target/dx/Sidekick.app` -- 3,492 files
and a 41.6 MB unsigned Mach-O executable -- is committed to this repository, in
`682c3e32554b746dc318c079886158d3ed866e73` (2026-09-17), by a local `dx bundle` run to check a
rename. Nobody put it there on purpose and nothing noticed for four days.

**Two facts had to line up, and each is defensible alone.** `.gitignore` says `/target/`,
anchored, and the comment above it says why: an unanchored `target/` also swallows
`exported_powers/*/pets/target/`, which is real export data and not build output. So the anchor
is correct and must stay. Meanwhile `dioxus.toml` sets `out_dir = "target/dx"`, and dx 0.7.9
resolves that key against the CRATE directory, not the workspace root
(`src/build/request.rs:2604`, `crate_out_dir()` -> `self.crate_dir().join(out_dir)`) -- then
copies every finished bundle there at the end of `dx bundle` (`src/cli/bundle.rs:95`). The
destination is `crates/app/target/dx`, which is inside the source tree and outside the anchor.
A correct ignore rule and a correct config, and between them a hole that `git add -A` walks into.

**So the rule is not "no path called target".** It is: a `target/` directory whose parent holds
a `Cargo.toml` is cargo's, and nothing in it belongs to git. That discriminator is the whole
point -- it catches all five non-root crates in this workspace, including the three that have no
output today and the vendored one, and it leaves the pet exports alone because no `Cargo.toml`
sits beside them. A path allow-list would have had to name the exception that caused the anchor
in the first place, which is how the hole gets re-opened by whoever adds the sixth crate.

**Both halves are checked, because either alone rots.** Untracking the files without fixing the
ignore means the next local bundle re-adds them; fixing the ignore without untracking leaves the
committed copy, since `.gitignore` has no effect on a path git already tracks. The ignore half
is asked of `git check-ignore` rather than parsed out of `.gitignore` -- the file has negations,
anchors and a `vendor/` rule interacting, and a reimplementation of git's matching is a second
opinion about the thing itself.

**What this does NOT claim.** The blob stays in history; this guard reads the worktree's index,
not the past. F87's entry says why that was left: the committed binary was measured for the four
`option_env!` secrets and carries none of them, so what remains in history is 36 MB of weight
rather than an exposure, and a history rewrite is a cost nobody is owed.

**How to grade it**: `--self-test` builds throwaway git repositories and runs the real rules
against them -- one violating each half, one clean, and one carrying `data/pets/target/index.json`
with no `Cargo.toml` beside it, which must pass. That last case is the mutation that matters: a
check that flagged it would be the anchoring bug again, pointed the other way.
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def git(repo: Path, *args: str) -> str:
    return subprocess.run(
        ("git", *args), cwd=repo, capture_output=True, text=True, check=True
    ).stdout


def crate_dirs(repo: Path) -> list[str]:
    """Every directory holding a tracked `Cargo.toml`, as a repo-relative posix path."""
    manifests = [line for line in git(repo, "ls-files", "*Cargo.toml").splitlines() if line]
    return sorted({str(Path(found).parent).removeprefix(".") for found in manifests})


def tracked_under_target(repo: Path, crates: list[str]) -> dict[str, list[str]]:
    """Tracked files living under each crate's `target/`. Empty is the only acceptable answer."""
    tracked = [line for line in git(repo, "ls-files").splitlines() if line]
    found: dict[str, list[str]] = {}
    for crate in crates:
        prefix = f"{crate}/target/" if crate else "target/"
        hits = [path for path in tracked if path.startswith(prefix)]
        if hits:
            found[prefix] = hits
    return found


def unignored(repo: Path, crates: list[str]) -> list[str]:
    """Crate target directories git would NOT ignore. Asked of git, not of `.gitignore`."""
    missing = []
    for crate in crates:
        # A probe path rather than the directory: `check-ignore` answers about a path, and the
        # directory need not exist -- which is exactly the case this is guarding, since four of
        # these crates have never been built into.
        probe = f"{crate}/target/probe" if crate else "target/probe"
        result = subprocess.run(
            ("git", "check-ignore", "-q", probe), cwd=repo, capture_output=True
        )
        if result.returncode != 0:
            missing.append(probe)
    return missing


def check(repo: Path) -> list[str]:
    """Both rules. Returns one line per violation; empty means green."""
    crates = crate_dirs(repo)
    problems = []
    for prefix, hits in tracked_under_target(repo, crates).items():
        shown = ", ".join(hits[:3]) + (f", +{len(hits) - 3} more" if len(hits) > 3 else "")
        problems.append(f"TRACKED  {len(hits)} file(s) under {prefix} -- {shown}")
    for probe in unignored(repo, crates):
        problems.append(f"UNIGNORED  git would add files under {probe.removesuffix('probe')}")
    return problems


def scratch_repo(root: Path, name: str, files: dict[str, str], ignore: str) -> Path:
    repo = root / name
    repo.mkdir()
    git(repo, "init", "-q")
    for path, text in files.items():
        target = repo / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)
    (repo / ".gitignore").write_text(ignore)
    git(repo, "add", "-A", "-f")
    return repo


def self_test() -> int:
    manifest = '[package]\nname = "x"\nversion = "0.1.0"\n'
    cases = [
        # (name, files, .gitignore, must this repo be refused, why)
        (
            "tracked",
            {"crates/app/Cargo.toml": manifest, "crates/app/target/dx/app": "bundle"},
            "/target/\n",
            True,
            "a crate's target/ is committed",
        ),
        (
            "unignored",
            {"crates/app/Cargo.toml": manifest},
            "/target/\n",
            True,
            "crates/app/target/ is not ignored, so the next build lands in the tree",
        ),
        (
            "clean",
            {"crates/app/Cargo.toml": manifest},
            "/target/\ncrates/app/target/\n",
            False,
            "both halves satisfied",
        ),
        (
            "data-not-build",
            {
                "Cargo.toml": manifest,
                "data/pets/target/index.json": "{}",
            },
            "/target/\n",
            False,
            "a target/ with no Cargo.toml beside it is data -- flagging it is the anchoring bug"
            " pointed the other way",
        ),
    ]
    failures = 0
    with tempfile.TemporaryDirectory() as tmp:
        for name, files, ignore, must_refuse, why in cases:
            problems = check(scratch_repo(Path(tmp), name, files, ignore))
            refused = bool(problems)
            verdict = "ok " if refused == must_refuse else "FAIL"
            if refused != must_refuse:
                failures += 1
            print(f"self-test {verdict} {name}: {why}")
            for line in problems:
                print(f"           {line}")
    if failures:
        print(f"\nself-test: {failures} case(s) did not behave", file=sys.stderr)
        return 1
    print("\nself-test: both rules refuse what they are for, and the data case passes")
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    problems = check(ROOT)
    for line in problems:
        print(line, file=sys.stderr)
    if problems:
        print(
            "\nF87. A crate's target/ is cargo's, not git's. Untrack with\n"
            "`git rm -r --cached <path>` and add the directory to .gitignore -- both, since the\n"
            "ignore does nothing to a path already tracked and untracking alone lets the next\n"
            "build re-add it.",
            file=sys.stderr,
        )
        return 1

    crates = crate_dirs(ROOT)
    print(f"ok: {len(crates)} crate target directories, none tracked, all ignored")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
