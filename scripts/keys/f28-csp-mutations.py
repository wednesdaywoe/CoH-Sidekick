#!/usr/bin/env python3
"""Mutation-grade the desktop Content-Security-Policy.

The policy in `crates/app/src/desktop-csp.txt` is guarded by three graders that see different
things, and the reason there are three is that the first two both passed a policy that made the
app unusable:

  cargo     `desktop_csp_tests` in crates/app/src/main.rs — reads the file. Catches a directive
            deleted, a source added, a quote smuggled in. Cannot know what any of it DOES.
  browser   tests/desktop-csp.spec.ts — loads the policy into Chromium and measures behaviour.
            Catches a blocked eval, image or remote script. It does not run dioxus's interpreter,
            so it cannot see the app's own machinery being blocked.
  desktop   crates/app/examples/csp_probe.rs — the shipped engine, the real `dioxus://` origin,
            the real loopback edits socket and the real events XHR. The only grader that can
            fail the two mutations below it is the only one to catch.

`connect-src 'none'` shipped in commit 33443ed215 and was green in both of the first two. In the
real engine it renders a blank window that never draws a frame, because every DOM edit arrives
over `ws://127.0.0.1` (`edits.rs:95`). Opening only the socket renders correctly and drops every
click, because every user event is an XHR to `dioxus://index.html//__events`. Those are the last
two mutations here, and a harness without the `desktop` grader passes both.

Usage:
    f28-csp-mutations.py                 # cargo + browser graders
    f28-csp-mutations.py --with-desktop  # all three; needs a display, cannot run in CI
    f28-csp-mutations.py --self-test     # grade this script before trusting its verdict

A mutation is REPORTED if at least one grader fails on it. Any mutation that is green everywhere
is the finding restating itself, and this script exits non-zero.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
POLICY_FILE = REPO / "crates/app/src/desktop-csp.txt"

# Each mutation takes the shipped policy and returns a damaged one. Anything that returns the
# policy unchanged is a broken mutation, not a passing one — `--self-test` is what says so.
MUTATIONS: dict[str, callable] = {
    "connect-src opened to anything": lambda p: re.sub(r"connect-src [^;]*", "connect-src *", p),
    "connect-src removed altogether": lambda p: re.sub(r"connect-src [^;]*; ", "", p),
    "a remote script source admitted": lambda p: p.replace(
        "script-src 'self'", "script-src 'self' https://cdn.jsdelivr.net"
    ),
    "the avatar host dropped from img-src": lambda p: p.replace(
        " https://cdn.discordapp.com", ""
    ),
    "data: dropped, so the build preview stops rendering": lambda p: p.replace(
        "data: blob:", "blob:"
    ),
    "'unsafe-eval' dropped, so every document::eval site dies": lambda p: p.replace(
        " 'unsafe-eval'", ""
    ),
    "frame-src opened": lambda p: p.replace("frame-src 'none'", "frame-src *"),
    "a double quote smuggled in, truncating the attribute": lambda p: p.replace(
        "object-src 'none'", 'object-src "none"'
    ),
    # The two the first two graders cannot see. Both were green before the desktop grader existed.
    "connect-src back to 'none' (the app never draws a frame)": lambda p: re.sub(
        r"connect-src [^;]*", "connect-src 'none'", p
    ),
    "connect-src loses its origin (the app draws, then ignores every click)": lambda p: re.sub(
        r"connect-src [^;]*", "connect-src ws://127.0.0.1:*", p
    ),
}


def run(cmd: list[str], timeout: int) -> subprocess.CompletedProcess:
    return subprocess.run(
        cmd, cwd=REPO, capture_output=True, text=True, timeout=timeout, check=False
    )


def cargo_catches() -> bool:
    """True when the file-reading tests fail on the current policy."""
    proc = run(
        ["cargo", "test", "-q", "-p", "app", "--bin", "Sidekick", "--features", "desktop",
         "desktop_csp"],
        timeout=900,
    )
    return proc.returncode != 0


def browser_catches() -> bool:
    """True when the Chromium behavioural spec fails on the current policy.

    Chromium only: WebKit needs system libraries this box does not have and Firefox is not
    installed, so asking for all three would report a red that is about the machine.
    """
    proc = run(
        ["./node_modules/.bin/playwright", "test", "tests/desktop-csp.spec.ts",
         "--project=chromium", "--reporter=line"],
        timeout=600,
    )
    return proc.returncode != 0


def desktop_catches() -> bool:
    """True when the shipped engine cannot run the app under the current policy."""
    proc = run(
        ["cargo", "run", "-q", "-p", "app", "--features", "desktop", "--example", "csp_probe"],
        timeout=300,
    )
    if proc.returncode != 0:
        return True  # the watchdog fired: no frame was ever drawn

    match = re.search(r"=== F28 CSP PROBE RESULT ===\n(\{.*?\n\})", proc.stdout, re.S)
    if match is None:
        return True  # no verdict at all is a failure, not a pass
    result = json.loads(match.group(1))

    # `remoteFetch` is graded only on ALLOWED. A cross-origin fetch can also fail for reasons
    # that are nothing to do with this policy — example.com sends no `Access-Control-Allow-Origin`,
    # so an ordinary fetch is refused by CORS whatever `connect-src` says, which is why the probe
    # asks in `no-cors` mode and corroborates with the violation listener. Treating a bare failure
    # as proof would have this grader report success for a policy that permits everything.
    return (
        result.get("eventRoundTrip") != "ok"
        or result.get("remoteFetch") == "ALLOWED"
        or result.get("asyncFunction") != 42
        or result.get("appCssApplied") != "tabular-nums"
        or any("NOT LOADED" in sheet or "opaque" in sheet for sheet in result.get("sheets", []))
        or result.get("brandIcon", "").startswith(("BLANK", "missing"))
        or result.get("treeIcon", "").startswith(("BLANK", "missing"))
        or result.get("dataImage") != "loaded"
        or result.get("blobImage") != "loaded"
    )


def self_test() -> int:
    """Grade the harness itself: a mutation that changes nothing grades nothing."""
    shipped = POLICY_FILE.read_text()
    failures = []

    for name, mutate in MUTATIONS.items():
        mutated = mutate(shipped)
        if mutated == shipped:
            failures.append(f"{name}: left the policy unchanged, so it grades nothing")

    # The restore path is the one that can quietly corrupt the repo, so it is asserted rather
    # than assumed — and it is asserted by round-tripping, not by reading the code.
    probe = POLICY_FILE.read_text()
    POLICY_FILE.write_text(MUTATIONS["frame-src opened"](probe))
    POLICY_FILE.write_text(probe)
    if POLICY_FILE.read_text() != shipped:
        failures.append("restore did not put the shipped policy back byte for byte")

    # And the shipped policy must be the one that PASSES, or every mutation below is measured
    # against a broken baseline.
    if "connect-src 'none'" in shipped:
        failures.append(
            "the shipped policy still says connect-src 'none', which never draws a frame"
        )

    for failure in failures:
        print(f"SELF-TEST FAIL  {failure}")
    if failures:
        return 1
    print(f"SELF-TEST OK    {len(MUTATIONS)} mutations all change the policy; restore round-trips")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--with-desktop", action="store_true",
                        help="also run the shipped-engine probe (needs a display)")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    graders: list[tuple[str, callable]] = [("cargo", cargo_catches), ("browser", browser_catches)]
    if args.with_desktop:
        graders.append(("desktop", desktop_catches))

    shipped = POLICY_FILE.read_text()
    survivors: list[str] = []
    try:
        for name, mutate in MUTATIONS.items():
            POLICY_FILE.write_text(mutate(shipped))
            caught = [label for label, grade in graders if grade()]
            if caught:
                print(f"RED    {name}  [{','.join(caught)}]")
            else:
                print(f"GREEN  {name}  <- SURVIVED")
                survivors.append(name)
    finally:
        # Written rather than copied, so the restored file is NEWER than any artifact built from
        # a mutated one. A restore that leaves an older mtime lets cargo reuse the mutated build,
        # which is how a reverted mutation can look like it is still failing — or still passing.
        POLICY_FILE.write_text(shipped)

    if survivors:
        print(f"\n{len(survivors)} mutation(s) survived every grader:")
        for name in survivors:
            print(f"  - {name}")
        return 1
    print(f"\nall {len(MUTATIONS)} mutations reported by at least one grader")
    return 0


if __name__ == "__main__":
    sys.exit(main())
