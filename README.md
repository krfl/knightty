# knightty

A jousting game that runs in your terminal.

![screenshot](screenshot.png)

Two mounted knights, one narrow lane, one lance each. Aim your lance, time your jump, unhorse your opponent. Rendered entirely with halfblock pixels and a bitmap font. No graphics window, just your terminal.

## Run

```sh
cargo run --release
```

Built and tested with Rust 2021. Sound is on by default; the assets ship in `assets/sfx/`.

## Controls

| Key            | Action          |
|----------------|-----------------|
| `WASD` / arrows | Move the knight |
| `Space`        | Jump            |
| `R`            | Restart (after a round) |
| `Esc`          | Back to menu    |
| `Ctrl-C`       | Quit            |

Difficulty (EASY / NORMAL / HARD) is selectable from the main menu.

## Built with

- [ratatui](https://ratatui.rs) + [crossterm](https://github.com/crossterm-rs/crossterm) for terminal rendering
- [rodio](https://github.com/RustAudio/rodio) for sound
- A halfblock framebuffer with a custom bitmap font, so every glyph and sprite shares one coordinate system and scales together across terminal font sizes

## Tips

- Lower your terminal's font size for a bigger arena and sharper knights. The arena grows with the window, and 80x24 is enough to play.
- The arena size is picked when a run starts. Resizing during a run only rescales the view, and if the window gets too small the game pauses until you make it bigger again.
- Controls are smoothest in terminals that support the kitty keyboard protocol, such as kitty, WezTerm or Ghostty.
- A truecolor terminal (most modern ones) is required for the palette.
