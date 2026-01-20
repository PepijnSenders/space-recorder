# 04 - TUI

The terminal user interface layer using ratatui. Renders the PTY output and ASCII camera as composited layers.

## Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         Terminal                                 │
├─────────────────────────────────────────────────────────────────┤
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                                                           │  │
│  │   PTY Output Layer (full screen)                          │  │
│  │                                                           │  │
│  │   $ cargo build                                           │  │
│  │      Compiling...                                         │  │
│  │                                                           │  │
│  │                              ┌─────────────────┐          │  │
│  │                              │  Camera Modal   │          │  │
│  │                              │  (floating)     │          │  │
│  │                              └─────────────────┘          │  │
│  │                                                           │  │
│  └───────────────────────────────────────────────────────────┘  │
│  [Status Bar: camera on | bottom-right | 20x10 | standard]      │
└─────────────────────────────────────────────────────────────────┘
```

## Crate Dependencies

```toml
[dependencies]
ratatui = "0.28"
crossterm = "0.28"
```

## Data Structures

```rust
pub struct Tui {
    /// Terminal handle
    terminal: Terminal<CrosstermBackend<Stdout>>,
    /// PTY display buffer
    pty_buffer: PtyBuffer,
    /// Camera modal state
    camera_modal: CameraModal,
    /// Status bar
    status_bar: StatusBar,
    /// Whether to show status bar
    show_status: bool,
}

pub struct PtyBuffer {
    /// Raw output bytes (or parsed cells if using VT100)
    content: String,
    /// Scroll offset
    scroll: u16,
}

pub struct CameraModal {
    /// Whether camera is visible
    pub visible: bool,
    /// Position on screen
    pub position: ModalPosition,
    /// Size preset
    pub size: ModalSize,
    /// Current ASCII frame
    pub frame: Option<AsciiFrame>,
    /// Border style
    pub border: bool,
}

#[derive(Clone, Copy)]
pub enum ModalPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

#[derive(Clone, Copy)]
pub enum ModalSize {
    Small,   // 20x10 chars
    Medium,  // 40x20 chars
    Large,   // 60x30 chars
}

pub struct StatusBar {
    pub camera_status: String,
    pub position: String,
    pub size: String,
    pub charset: String,
}
```

## Terminal Setup

```rust
impl Tui {
    pub fn new() -> Result<Self> {
        // Enter raw mode
        crossterm::terminal::enable_raw_mode()?;

        // Enter alternate screen (preserves original terminal content)
        let mut stdout = std::io::stdout();
        crossterm::execute!(
            stdout,
            crossterm::terminal::EnterAlternateScreen,
            crossterm::event::EnableMouseCapture, // optional
        )?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            terminal,
            pty_buffer: PtyBuffer::default(),
            camera_modal: CameraModal::default(),
            status_bar: StatusBar::default(),
            show_status: true,
        })
    }

    pub fn restore(&mut self) -> Result<()> {
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            self.terminal.backend_mut(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture,
        )?;
        self.terminal.show_cursor()?;
        Ok(())
    }
}
```

## Rendering

### Main Render Loop

```rust
impl Tui {
    pub fn render(&mut self) -> Result<()> {
        self.terminal.draw(|frame| {
            let area = frame.area();

            // Calculate layout
            let main_area = if self.show_status {
                Rect {
                    height: area.height.saturating_sub(1),
                    ..area
                }
            } else {
                area
            };

            // Layer 1: PTY output (full area)
            self.render_pty(frame, main_area);

            // Layer 2: Camera modal (floating overlay)
            if self.camera_modal.visible {
                self.render_camera_modal(frame, main_area);
            }

            // Layer 3: Status bar (bottom)
            if self.show_status {
                let status_area = Rect {
                    x: 0,
                    y: area.height - 1,
                    width: area.width,
                    height: 1,
                };
                self.render_status_bar(frame, status_area);
            }
        })?;

        Ok(())
    }
}
```

### PTY Rendering

For MVP, just render raw PTY output:

```rust
fn render_pty(&self, frame: &mut Frame, area: Rect) {
    let paragraph = Paragraph::new(self.pty_buffer.content.as_str())
        .style(Style::default());
    frame.render_widget(paragraph, area);
}
```

For proper terminal emulation (future), would need VT100 parser and cell-by-cell rendering.

### Camera Modal Rendering

```rust
fn render_camera_modal(&self, frame: &mut Frame, container: Rect) {
    let modal = &self.camera_modal;
    let (width, height) = modal.size.dimensions();

    // Calculate position
    let rect = modal.position.calculate_rect(container, width, height);

    // Clear the modal area (important for overlay)
    frame.render_widget(Clear, rect);

    // Render ASCII frame
    if let Some(ascii_frame) = &modal.frame {
        let text = ascii_frame.to_string();

        let block = if modal.border {
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
        } else {
            Block::default()
        };

        let inner = block.inner(rect);
        frame.render_widget(block, rect);

        let paragraph = Paragraph::new(text)
            .style(Style::default().fg(Color::White));
        frame.render_widget(paragraph, inner);
    }
}
```

### Position Calculation

```rust
impl ModalPosition {
    fn calculate_rect(&self, container: Rect, width: u16, height: u16) -> Rect {
        let margin = 1;

        let (x, y) = match self {
            ModalPosition::TopLeft => (margin, margin),
            ModalPosition::TopRight => (
                container.width.saturating_sub(width + margin),
                margin,
            ),
            ModalPosition::BottomLeft => (
                margin,
                container.height.saturating_sub(height + margin),
            ),
            ModalPosition::BottomRight => (
                container.width.saturating_sub(width + margin),
                container.height.saturating_sub(height + margin),
            ),
            ModalPosition::Center => (
                (container.width.saturating_sub(width)) / 2,
                (container.height.saturating_sub(height)) / 2,
            ),
        };

        Rect { x, y, width, height }
    }
}
```

### Size Dimensions

```rust
impl ModalSize {
    fn dimensions(&self) -> (u16, u16) {
        match self {
            ModalSize::Small => (22, 12),   // 20x10 + border
            ModalSize::Medium => (42, 22),  // 40x20 + border
            ModalSize::Large => (62, 32),   // 60x30 + border
        }
    }

    fn inner_dimensions(&self) -> (u16, u16) {
        match self {
            ModalSize::Small => (20, 10),
            ModalSize::Medium => (40, 20),
            ModalSize::Large => (60, 30),
        }
    }
}
```

### Status Bar

```rust
fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
    let status = format!(
        " {} | {} | {} | {} ",
        if self.camera_modal.visible { "cam:on" } else { "cam:off" },
        self.camera_modal.position.name(),
        self.camera_modal.size.name(),
        "standard", // charset name
    );

    let paragraph = Paragraph::new(status)
        .style(Style::default().fg(Color::Black).bg(Color::White));
    frame.render_widget(paragraph, area);
}
```

## Input Handling

### Event Loop

```rust
pub async fn run(&mut self, pty: &mut PtyHost, camera: &mut Option<CameraCapture>) {
    let mut reader = crossterm::event::EventStream::new();

    loop {
        tokio::select! {
            // Terminal events
            event = reader.next() => {
                if let Some(Ok(event)) = event {
                    if self.handle_event(event, pty).await? == Action::Quit {
                        break;
                    }
                }
            }
            // PTY output
            data = pty.read_async() => {
                self.pty_buffer.append(&data);
                self.render()?;
            }
            // Camera frame
            _ = tokio::time::sleep(Duration::from_millis(66)) => {
                if let Some(camera) = camera {
                    if let Some(frame) = camera.get_frame() {
                        let ascii = renderer.render(&frame);
                        self.camera_modal.frame = Some(ascii);
                        self.render()?;
                    }
                }
            }
        }
    }
}
```

### Hotkey Handling

```rust
fn handle_event(&mut self, event: Event, pty: &mut PtyHost) -> Result<Action> {
    match event {
        Event::Key(key) => {
            match (key.modifiers, key.code) {
                // Alt+C: Toggle camera
                (KeyModifiers::ALT, KeyCode::Char('c')) => {
                    self.camera_modal.visible = !self.camera_modal.visible;
                }
                // Alt+P: Cycle position
                (KeyModifiers::ALT, KeyCode::Char('p')) => {
                    self.camera_modal.position = self.camera_modal.position.next();
                }
                // Alt+S: Cycle size
                (KeyModifiers::ALT, KeyCode::Char('s')) => {
                    self.camera_modal.size = self.camera_modal.size.next();
                }
                // Alt+A: Cycle ASCII style
                (KeyModifiers::ALT, KeyCode::Char('a')) => {
                    // cycle charset
                }
                // All other keys: forward to PTY
                _ => {
                    let bytes = key_to_bytes(key);
                    pty.write(&bytes)?;
                }
            }
        }
        Event::Resize(w, h) => {
            pty.resize(PtySize { cols: w, rows: h, .. })?;
        }
        _ => {}
    }
    Ok(Action::Continue)
}
```

### Key to Bytes Conversion

```rust
fn key_to_bytes(key: KeyEvent) -> Vec<u8> {
    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+A = 0x01, Ctrl+B = 0x02, etc.
                vec![(c as u8) & 0x1f]
            } else {
                c.to_string().into_bytes()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        // ... more keys
        _ => vec![],
    }
}
```

## ASCII Frame to String

```rust
impl AsciiFrame {
    pub fn to_string(&self) -> String {
        self.chars
            .chunks(self.width as usize)
            .map(|row| row.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
```

## Overlay Compositing

For true overlays (where camera floats over terminal text), need to:

1. Parse PTY output into a 2D cell buffer
2. Render cell buffer
3. Overdraw camera modal on top

The `Clear` widget + re-render approach works but means terminal text is hidden under the modal. For transparent overlay effect, would need:

```rust
// Future: semi-transparent overlay
for (y, row) in ascii_frame.rows().enumerate() {
    for (x, ch) in row.chars().enumerate() {
        let screen_x = modal_x + x;
        let screen_y = modal_y + y;

        // Blend with underlying terminal cell
        let bg_cell = pty_buffer.get_cell(screen_x, screen_y);
        let blended = blend_chars(bg_cell.char, ch, opacity);
        frame.set_cell(screen_x, screen_y, blended);
    }
}
```

## Performance

- Target 30 FPS for smooth feel
- Only re-render when state changes
- Use `terminal.draw()` which handles diffing

```rust
// Track dirty state
let mut needs_render = false;

if pty_has_new_output {
    needs_render = true;
}
if camera_has_new_frame {
    needs_render = true;
}
if hotkey_pressed {
    needs_render = true;
}

if needs_render {
    self.render()?;
}
```

## Testing

```rust
#[test]
fn test_position_calculation() {
    let container = Rect { x: 0, y: 0, width: 80, height: 24 };
    let rect = ModalPosition::BottomRight.calculate_rect(container, 20, 10);
    assert_eq!(rect.x, 59); // 80 - 20 - 1
    assert_eq!(rect.y, 13); // 24 - 10 - 1
}

#[test]
fn test_size_cycle() {
    let size = ModalSize::Small;
    assert_eq!(size.next(), ModalSize::Medium);
    assert_eq!(size.next().next(), ModalSize::Large);
    assert_eq!(size.next().next().next(), ModalSize::Small);
}
```

## Implementation Checklist

- [ ] Terminal setup with crossterm
- [ ] Alternate screen mode
- [ ] PTY output rendering
- [ ] Camera modal rendering with Clear
- [ ] Position calculation for all corners
- [ ] Size presets
- [ ] Status bar
- [ ] Hotkey handling
- [ ] Key-to-bytes conversion for PTY
- [ ] Window resize handling
- [ ] Event loop with tokio::select
- [ ] Graceful cleanup on exit
