# 01 - PTY Host

The PTY host spawns the user's shell and relays I/O between the shell and the TUI, allowing the terminal content to be rendered within ratatui while maintaining full shell functionality.

## Overview

```
┌─────────────┐      ┌─────────────┐      ┌─────────────┐
│   stdin     │ ───▶ │   PTY Host  │ ───▶ │    Shell    │
│   stdout    │ ◀─── │             │ ◀─── │  (zsh/bash) │
│   (raw)     │      │             │      │             │
└─────────────┘      └─────────────┘      └─────────────┘
                            │
                            ▼
                     ┌─────────────┐
                     │  TUI Layer  │
                     │  (ratatui)  │
                     └─────────────┘
```

## Responsibilities

1. **Shell Spawning** - Fork and exec the user's shell in a PTY
2. **I/O Relay** - Forward stdin to shell, shell output to TUI
3. **Signal Handling** - Forward SIGWINCH on resize, handle SIGINT/SIGTERM
4. **Terminal State** - Manage raw mode, restore on exit

## Crate Dependencies

```toml
[dependencies]
portable-pty = "0.8"      # Cross-platform PTY handling
tokio = { version = "1", features = ["full"] }
```

Alternative: `rustix` + manual PTY handling for lower-level control.

## Data Structures

```rust
pub struct PtyHost {
    /// The PTY master handle
    master: Box<dyn MasterPty + Send>,
    /// Child process handle
    child: Box<dyn Child + Send + Sync>,
    /// Reader for shell output
    reader: Box<dyn Read + Send>,
    /// Writer for shell input
    writer: Box<dyn Write + Send>,
    /// Current terminal size
    size: PtySize,
}

pub struct PtySize {
    pub rows: u16,
    pub cols: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}
```

## API

```rust
impl PtyHost {
    /// Spawn a new shell in a PTY
    pub fn spawn(shell: &str, size: PtySize) -> Result<Self>;

    /// Resize the PTY (call on SIGWINCH)
    pub fn resize(&self, size: PtySize) -> Result<()>;

    /// Write bytes to the shell's stdin
    pub fn write(&mut self, data: &[u8]) -> Result<usize>;

    /// Read available bytes from the shell's stdout
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    /// Check if the shell process has exited
    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>>;

    /// Kill the shell process
    pub fn kill(&mut self) -> Result<()>;
}
```

## Shell Selection

Priority order for determining which shell to spawn:

1. `--shell` CLI argument
2. `$SHELL` environment variable
3. `/bin/zsh` (macOS default)
4. `/bin/bash` (fallback)

```rust
fn default_shell() -> String {
    std::env::var("SHELL")
        .unwrap_or_else(|_| "/bin/zsh".to_string())
}
```

## Terminal Raw Mode

The application must put the terminal in raw mode to:
- Capture all keystrokes (including Ctrl sequences)
- Disable line buffering
- Disable echo (shell handles its own echo)

```rust
// On startup
let original_termios = termios::tcgetattr(stdin)?;
let mut raw = original_termios.clone();
termios::cfmakeraw(&mut raw);
termios::tcsetattr(stdin, termios::TCSANOW, &raw)?;

// On exit (must restore!)
termios::tcsetattr(stdin, termios::TCSANOW, &original_termios)?;
```

Use `scopeguard` or similar to ensure restoration on panic.

## Signal Handling

### SIGWINCH (Window Resize)

```rust
// Register handler
signal_hook::flag::register(SIGWINCH, Arc::clone(&resize_flag))?;

// In main loop
if resize_flag.swap(false, Ordering::Relaxed) {
    let size = terminal_size()?;
    pty_host.resize(size)?;
}
```

### SIGINT / SIGTERM

Forward to shell, don't exit immediately:

```rust
// SIGINT (Ctrl+C) - forward to shell
pty_host.write(&[0x03])?; // ETX

// SIGTERM - graceful shutdown
pty_host.kill()?;
restore_terminal();
```

## I/O Architecture

### Non-blocking I/O with Tokio

```rust
async fn io_loop(pty: PtyHost, tui: Tui) {
    let mut stdin = tokio::io::stdin();
    let mut pty_reader = /* async wrapper for pty.reader */;

    loop {
        tokio::select! {
            // User input -> shell
            result = stdin.read(&mut buf) => {
                let n = result?;
                if is_hotkey(&buf[..n]) {
                    handle_hotkey(&buf[..n], &mut tui);
                } else {
                    pty.write(&buf[..n])?;
                }
            }
            // Shell output -> TUI
            result = pty_reader.read(&mut buf) => {
                let n = result?;
                tui.process_output(&buf[..n]);
            }
        }
    }
}
```

### Hotkey Interception

Some key sequences are intercepted before reaching the shell:

| Sequence | Bytes | Action |
|----------|-------|--------|
| `Ctrl+\` | `0x1C` | Toggle camera |
| `Ctrl+]` | `0x1D` | Cycle position |
| `Ctrl+[` | `0x1B` | Ambiguous (ESC), use different binding |

Revised hotkey plan - use Alt sequences to avoid conflicts:

| Sequence | Action |
|----------|--------|
| `Alt+c` | Toggle camera |
| `Alt+p` | Cycle position |
| `Alt+s` | Cycle size |
| `Alt+a` | Cycle ASCII style |

## Terminal Emulation

The PTY outputs raw terminal escape sequences. Options:

### Option A: Pass-through (Recommended for MVP)

Don't parse escape sequences. The TUI renders the PTY output byte-for-byte into a buffer that gets displayed. Works because ratatui can render raw text.

Limitation: Can't do true overlays (camera would overwrite text).

### Option B: VT100 Parser

Use `vte` crate to parse escape sequences and maintain a virtual terminal buffer:

```rust
use vte::{Parser, Perform};

struct TerminalBuffer {
    cells: Vec<Vec<Cell>>,
    cursor: (usize, usize),
}

impl Perform for TerminalBuffer {
    fn print(&mut self, c: char) { /* add char at cursor */ }
    fn execute(&mut self, byte: u8) { /* handle control chars */ }
    fn csi_dispatch(&mut self, params: &[u16], ...) { /* cursor moves, colors */ }
}
```

This allows true compositing - render terminal buffer, then overlay camera on top.

**Recommendation**: Start with pass-through, add VT100 parsing later if overlay quality matters.

## Error Handling

```rust
pub enum PtyError {
    /// Failed to spawn shell
    SpawnFailed(std::io::Error),
    /// PTY I/O error
    IoError(std::io::Error),
    /// Shell exited unexpectedly
    ShellExited(ExitStatus),
    /// Failed to resize PTY
    ResizeFailed(std::io::Error),
}
```

## Testing

### Unit Tests

```rust
#[test]
fn test_spawn_shell() {
    let pty = PtyHost::spawn("/bin/echo", PtySize::default())?;
    // Should spawn and exit quickly
}

#[test]
fn test_echo() {
    let mut pty = PtyHost::spawn("/bin/cat", size)?;
    pty.write(b"hello")?;
    let mut buf = [0u8; 5];
    pty.read(&mut buf)?;
    assert_eq!(&buf, b"hello");
}
```

### Integration Tests

- Spawn shell, send commands, verify output
- Test resize handling
- Test signal forwarding
- Test cleanup on exit

## Implementation Checklist

- [ ] Basic PTY spawning with portable-pty
- [ ] Shell selection logic
- [ ] Raw mode enter/exit with cleanup
- [ ] Stdin -> PTY forwarding
- [ ] PTY -> stdout forwarding
- [ ] SIGWINCH resize handling
- [ ] Hotkey interception
- [ ] Graceful shutdown
- [ ] VT100 parsing (future)
