use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color as RatColor;
use ratatui::widgets::Widget;

use crate::render::framebuffer::{Color, Framebuffer};

/// Halfblock framebuffer dimensions per terminal cell.
pub const PX_PER_COL: u32 = 1;
pub const PX_PER_ROW: u32 = 2;

/// Default fallback cell pixel size if the terminal won't tell us.
const DEFAULT_CELL_W: u16 = 8;
const DEFAULT_CELL_H: u16 = 16;

/// Detect the terminal's cell pixel dimensions via `TIOCGWINSZ` (`ws_xpixel`
/// / `ws_ypixel` divided by cell counts). Falls back to a sensible default
/// on platforms where the terminal doesn't populate the pixel fields
/// (e.g. macOS Terminal.app, some tmux configurations).
#[cfg(unix)]
pub fn detect_cell_pixel_size() -> (u16, u16) {
    use std::mem::MaybeUninit;
    let mut ws: MaybeUninit<libc::winsize> = MaybeUninit::uninit();
    let result =
        unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, ws.as_mut_ptr()) };
    if result == 0 {
        let ws = unsafe { ws.assume_init() };
        if ws.ws_xpixel > 0 && ws.ws_ypixel > 0 && ws.ws_col > 0 && ws.ws_row > 0 {
            let w = (ws.ws_xpixel / ws.ws_col).max(4);
            let h = (ws.ws_ypixel / ws.ws_row).max(8);
            return (w, h);
        }
    }
    (DEFAULT_CELL_W, DEFAULT_CELL_H)
}

#[cfg(not(unix))]
pub fn detect_cell_pixel_size() -> (u16, u16) {
    (DEFAULT_CELL_W, DEFAULT_CELL_H)
}

pub struct JoustView<'a> {
    pub framebuffer: &'a Framebuffer,
}

impl Widget for JoustView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let fb = self.framebuffer;
        let fb_w = fb.width;
        let fb_h = fb.height;
        let buf_len = fb.color.len();
        // Defensive: if width/height somehow got out of sync with `color`
        // (shouldn't happen, but a mid-frame resize race or a future bug
        // could produce it), bail out instead of panicking with an OOB index.
        if (fb_w as usize) * (fb_h as usize) != buf_len {
            return;
        }
        let cols = area.width.min(fb_w as u16);
        let rows = area.height.min((fb_h / 2) as u16);

        for cy in 0..rows {
            let py_top = cy as u32 * 2;
            let py_bot = py_top + 1;
            let top_row = (py_top * fb_w) as usize;
            let bot_row = if py_bot < fb_h {
                (py_bot * fb_w) as usize
            } else {
                top_row
            };

            for cx in 0..cols {
                let px = cx as usize;
                let top_idx = top_row + px;
                if top_idx >= buf_len {
                    continue;
                }
                let top = fb.color[top_idx];
                let bot = if py_bot < fb_h {
                    let bot_idx = bot_row + px;
                    if bot_idx >= buf_len {
                        Color::BLACK
                    } else {
                        fb.color[bot_idx]
                    }
                } else {
                    Color::BLACK
                };

                if let Some(cell) = buf.cell_mut((area.x + cx, area.y + cy)) {
                    cell.set_char('▀');
                    cell.set_fg(RatColor::Rgb(top.r, top.g, top.b));
                    cell.set_bg(RatColor::Rgb(bot.r, bot.g, bot.b));
                }
            }
        }
    }
}
