# knightty

A jousting game that runs in your terminal.

![screenshot](screenshot.png)

Two mounted knights in a round arena, one lance each. Hit your opponent with your lance before they hit you with theirs. Rendered entirely with halfblock pixels and a bitmap font. No graphics window, just your terminal.

## How to play

Your lance points wherever you are moving, and a hit only counts when you have some speed. Unhorse ten opponents in a row to win the tourney. Get unhorsed once and the run is over.

Each round adds more hazards. Riding into a pit ends your run, and an opponent who falls into one counts as a win for you. Bumpers bounce you away harder than you came in. A jump carries you over pits, bumpers and lances, but your own lance can't hit anything while you are in the air.

## Run

```sh
cargo run --release
```

You need a recent stable Rust toolchain. Sound is on by default; the assets ship in `assets/sfx/`.

`cargo test` runs the tests, including ones that render frames without a terminal.

## Controls

| Key                     | Action                     |
|-------------------------|----------------------------|
| `WASD` / arrows         | Move the knight            |
| `Space`                 | Jump                       |
| `R` / `Enter` / `Space` | Restart after losing       |
| `Esc`                   | Back to menu               |
| `Ctrl-C`                | Quit                       |

In the menu, `W`/`S` or the up and down arrows move the selection and `Enter` or `Space` picks it. Picking the difficulty line cycles through EASY, NORMAL and HARD.

## Built with

- [ratatui](https://ratatui.rs) + [crossterm](https://github.com/crossterm-rs/crossterm) for terminal rendering
- [rodio](https://github.com/RustAudio/rodio) for sound
- A halfblock framebuffer with a custom bitmap font, so every glyph and sprite shares one coordinate system and scales together across terminal font sizes

## Tips

- Lower your terminal's font size for a bigger arena and sharper knights. The arena grows with the window, and 80x24 is enough to play.
- The arena size is picked when a run starts. Resizing during a run only rescales the view, and if the window gets too small the game pauses until you make it bigger again.
- Controls are smoothest in terminals that support the kitty keyboard protocol, such as kitty, WezTerm or Ghostty.
- A truecolor terminal (most modern ones) is required for the palette.
