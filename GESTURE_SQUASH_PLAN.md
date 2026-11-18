# Gesture module squash candidate plan

This document is a **plan only**. Nothing here has been executed: no branch
was reset, no commit was rewritten, and no shared history was changed.

As of `0f3b9d4`, `origin/render/p0-parity..HEAD` contains **56 commits** and
**74 files** (`+11988/-153`). All 56 commits are already published on their
own dedicated `gesture/*` branches, so nothing is lost.

## Shared-worktree constraint

The shared tree has uncommitted files owned by other agents:

- `src/anim.rs`, `src/ir.rs`, `src/layout.rs`, `src/lib.rs` (modified);
- `tests/m3_motion_tokens.rs`, `tests/material_t3_tokens.rs` (untracked).

A squash must never use `git reset --hard`, `git checkout -A`, `git add -A`,
or `git commit -a`. Use a dedicated worktree, or stash those paths with
`git stash push -- <paths>` and pop them immediately afterwards.

## Recommended grouping (five logical commits)

1. **Gesture engine implementation**
   - Files: `src/gestures.rs`, `src/scroll_physics.rs`, `src/lib.rs`.
   - Source commits include: hit-path/double-tap release, iOS bounce
     physics, nested scroll coordinator, parallel pointer arenas, rotation
     callback, `DragStartBehavior`.
   - Suggested message: `feat: implement gesture engine parity`.
2. **Gesture test pins**
   - Files: all 37 files under `tests/` in the range.
   - Suggested message: `test: pin Flutter gesture parity`.
3. **Gate infrastructure**
   - Files: all files under `gesture-gate/` (33 files, including
     `fixtures/`).
   - Suggested message: `feat: add gesture parity gate stack`.
4. **Documentation**
   - Files: `GESTURE_BOUNCE_PHYSICS.md`, `GESTURE_SCALE_GATE_TRACE.md`.
   - Suggested message: `docs: record gesture source decisions`.
5. **Trace bookkeeping**
   - Files: `gesture-gate/gesture-trace-gate.mjs`,
     `gesture-gate/TRACE_GATE.md`, `gesture-gate/README.md` plus new test
     additions — keep these in group 2 and 3 only if the groups require a
     strictly monotonic file set; otherwise groups 2–4 already cover them.

If the dispatcher requires exactly one logical commit, combine groups 1–4
into a single commit with message `feat: complete gesture parity program`.
That loses intermediate file-level intent but preserves the final tree.

## Dry-run commands (do not execute without dispatcher approval)

```sh
# 1. Verify, in a dedicated worktree.
git worktree add /tmp/gesture-squash HEAD
cd /tmp/gesture-squash

# 2. Confirm the exact file set before any staging.
git diff --name-only origin/render/p0-parity..HEAD

# 3. Build a single squashed tree (index only; no commit yet).
git read-tree origin/render/p0-parity
git diff --cached --name-only   # must match the 74 files above

# 4. Commit only after re-running the full gate stack.
git commit -m "feat: complete gesture parity program"
```

The actual squash must stay off the shared branch until the dispatcher
decides which of the five groups to keep separate.

## Preconditions before any squash

- `trace`: pass (318 anchors);
- `e2e`: pass (21 checks);
- six gates: pass/na/fail = 0/0/1 each;
- baseline loop: pass=0, all fault matrices exit 1;
- archive/freshness/consistency: pass and all fault states exit 1;
- Rust: full `cargo test --all-targets` green except the two known
  unrelated fixture failures owned by other lanes;
- golden: 23/23.
