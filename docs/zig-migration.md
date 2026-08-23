# Migrating space-recorder to Zig

This document outlines how to port the codebase from Rust to Zig.

## Why Zig

- No hidden control flow or allocator magic — every allocation is explicit and passed in.
- `comptime` replaces macros and generics with ordinary code that runs at compile time.
- C interop with zero FFI boilerplate, which matters for the PTY and camera syscalls we already make.
- Smaller binary, no runtime, easier to cross-compile.

## Codebase map

| Rust module | What it does | Zig approach |
|---|---|---|
| `src/main.rs` | Arg parsing, wires up PTY + camera + event loop | `src/main.zig`, use `std.process.args` |
| `src/pty/` | Spawns shell, reads/writes via `fork`+`exec`+`openpty` | Thin wrapper around the same POSIX calls — no crate needed |
| `src/camera/` | Grabs frames via platform camera API | Call the C API directly (`@cImport`) |
| `src/ascii/` | Converts pixel data to ASCII/block characters | Pure computation — port as-is |
| `src/renderer.rs` | Composites ASCII art + terminal output | Write to a `std.ArrayList(u8)`, flush once per frame |
| `src/terminal/` | TUI (crossterm) — status bar, modal | Use `std.posix` raw terminal mode + ANSI escape sequences |
| `src/event_loop.rs` | Drives the main loop with `tokio` | Zig has no async runtime yet; use threads + `std.Thread.Mutex` |

## Suggested order

1. **PTY layer** — the tightest syscall boundary; easiest to validate against the Rust output.
2. **Camera capture** — `@cImport` of the platform headers, return raw byte slices.
3. **ASCII renderer** — pure functions, no I/O, easy to unit-test with `std.testing`.
4. **Terminal / TUI** — raw mode setup, ANSI rendering loop.
5. **Event loop + main** — wire everything together with threads instead of `tokio`.

## Key Rust → Zig translation notes

- Replace `tokio::sync::mpsc` with a `std.Thread.Mutex`-guarded ring buffer or `std.fifo.LinearFifo`.
- `Arc<Mutex<T>>` becomes a struct passed by pointer; Zig ownership is tracked manually.
- Rust's `?` operator → Zig's `try` (same idea, explicit error union in the return type).
- `Vec<u8>` → `std.ArrayList(u8)` with an explicit `Allocator` argument.
- `clap` argument parsing → `std.process.ArgIterator` or the small `args` pattern in the Zig stdlib.

## Building

```zig
// build.zig skeleton
pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const exe = b.addExecutable(.{
        .name = "space-recorder",
        .root_source_file = b.path("src/main.zig"),
        .target = target,
        .optimize = optimize,
    });

    // Link system libs needed for camera / PTY
    exe.linkLibC();
    // exe.linkSystemLibrary("avfoundation"); // macOS camera
    b.installArtifact(exe);
}
```
