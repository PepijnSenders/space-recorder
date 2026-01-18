# Single-Task Backpressure Protocol

**One task per context window.** Complete the next unchecked `- [ ]` from plan.md.

## Context Gathering

Before starting, run these Explore subagents **in parallel** to gather context:

1. `specs/` directory → return summary of all 7 spec files
2. `plan.md` → return summary of phases and current task status
3. `progress.txt` → return summary of completed work

## Workflow

1. **Find** first unchecked `- [ ]` in plan.md
2. **Study** relevant specs before implementing
3. **Implement** to satisfy ALL `- AC:` criteria
4. **Validate**: `cargo check && cargo clippy && cargo test && cargo build`
5. **Log** results to progress.txt
6. **Fix** failures, re-validate until all pass
7. **Mark complete** - change `- [ ]` to `- [x]`

## progress.txt Format

```
Phase: [phase name]
Task: [task title]

[✓] Implemented X
[✗] cargo check failed: Y
    Fix: Z
[✓] cargo check passed

✅ VALIDATED - Task complete
```

## Rules

- ONE task per session
- ALL acceptance criteria must pass
- Log EVERY validation result
- Stop at "Final MVP Verification" (skip Post-MVP Backlog)
