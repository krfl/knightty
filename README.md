# knightty

A jousting game that runs in your terminal.

![screenshot](screenshot.png)

Two knights, a round arena, one lance each. Hit the other knight with your lance before they hit you. Beat ten in a row to win.

Everything is drawn with halfblock characters and a bitmap font.

## Run

```sh
cargo run --release
```

Needs a recent stable Rust.

## Controls

| Key                     | Action               |
|-------------------------|----------------------|
| `WASD` / arrows         | Move                 |
| `Space`                 | Jump                 |
| `R` / `Enter` / `Space` | Restart after a loss |
| `Esc`                   | Back to menu         |
| `Ctrl-C`                | Quit                 |

In the menu, move with `W`/`S` or the arrows and pick with `Enter`.

## Good to know

- Your lance points the way you are moving, and a hit only counts at speed.
- Pits end your run, and your opponent's.
- A jump clears pits, bumpers and lances.
- A smaller font gives a bigger arena and more detail. 80x24 is the minimum.
- The arena size is set when a run starts. Resizing mid-run only zooms.
- You need a truecolor terminal.
- Controls work best with the kitty keyboard protocol (kitty, WezTerm, Ghostty).

## Built with

[ratatui](https://ratatui.rs), [crossterm](https://github.com/crossterm-rs/crossterm) and [rodio](https://github.com/RustAudio/rodio).
