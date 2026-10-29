// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 RS-Key contributors

use std::vec::Vec;

use embedded_graphics::{
    Pixel,
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Size},
    pixelcolor::Rgb565,
    prelude::PointsIter,
    primitives::Rectangle,
};

use super::*;

/// A 172x320 frame that refuses any write outside the panel — the real panel
/// would wrap such a write onto the wrong column.
struct Frame {
    px: Vec<Rgb565>,
}

impl Frame {
    fn new() -> Self {
        Self {
            px: std::vec![theme::BG; usize::from(NARROW_W) * usize::from(NARROW_H)],
        }
    }

    fn put(&mut self, x: i32, y: i32, color: Rgb565) {
        assert!(
            (0..i32::from(NARROW_W)).contains(&x) && (0..i32::from(NARROW_H)).contains(&y),
            "write at ({x}, {y}) is outside the {NARROW_W}x{NARROW_H} panel"
        );
        self.px[y as usize * usize::from(NARROW_W) + x as usize] = color;
    }

    fn lit(&self) -> usize {
        self.px.iter().filter(|&&p| p != theme::BG).count()
    }
}

impl OriginDimensions for Frame {
    fn size(&self) -> Size {
        Size::new(NARROW_W.into(), NARROW_H.into())
    }
}

impl DrawTarget for Frame {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Self::Error> {
        for Pixel(p, c) in pixels {
            self.put(p.x, p.y, c);
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Rgb565) -> Result<(), Self::Error> {
        for p in area.points() {
            self.put(p.x, p.y, color);
        }
        Ok(())
    }

    fn fill_contiguous<I: IntoIterator<Item = Rgb565>>(
        &mut self,
        area: &Rectangle,
        colors: I,
    ) -> Result<(), Self::Error> {
        for (p, c) in area.points().zip(colors) {
            self.put(p.x, p.y, c);
        }
        Ok(())
    }
}

fn confirm_frame(title: &'static str, primary: &[u8], secondary: &[u8]) -> Frame {
    let mut f = Frame::new();
    let _ = render_confirm_narrow(&mut f, &ConfirmPrompt::new(title, primary, secondary));
    f
}

#[test]
fn worst_case_label_never_cuts() {
    // 48 kept bytes plus the 3-character head mark, in every printable glyph: the
    // registrable suffix of a domain must never be the part that gets dropped.
    for c in 0x20u8..0x7F {
        let text: std::string::String = core::iter::repeat_n(c as char, MAX_TEXT).collect();
        let w = wrap(&text, PRIMARY_ROLE, PRIMARY_LINES);
        assert!(
            !w.cut,
            "{:?} x{MAX_TEXT} overflows {PRIMARY_LINES} lines",
            c as char
        );
    }
}

#[test]
fn wrap_covers_the_text_without_gaps() {
    let text = "accounts.google.com.attacker.example.org";
    let w = wrap(text, PRIMARY_ROLE, PRIMARY_LINES);
    assert!(w.count >= 2);
    let mut at = 0;
    for &(s, e) in &w.lines[..w.count] {
        assert_eq!(s, at);
        assert!(fits(&text[s..e], PRIMARY_ROLE));
        at = e;
    }
    assert_eq!(at, text.len());
}

#[test]
fn overflow_is_marked_and_still_fits() {
    let text: std::string::String = core::iter::repeat_n('W', 48).collect();
    let w = wrap(&text, SECONDARY_ROLE, 1);
    assert!(w.cut);
    assert_eq!(w.count, 1);
    let (s, e) = w.lines[0];
    let dots = font::width(ELLIPSIS, SECONDARY_ROLE).unwrap();
    assert!(font::width(&text[s..e], SECONDARY_ROLE).unwrap() + dots <= TEXT_W);
}

#[test]
fn a_glyph_wider_than_the_box_still_advances() {
    // Role::Ready at 30 px is far wider per glyph than a 148 px box allows for a
    // long run; the wrap must not loop on a line that cannot fit even one glyph.
    let w = wrap("WWWWWWWW", Role::Ready, MAX_LINES);
    assert!(w.count >= 1);
    assert!(w.lines[..w.count].iter().all(|&(s, e)| e > s));
}

#[test]
fn confirm_and_idle_stay_inside_the_panel() {
    let long = [b'W'; 80];
    let f = confirm_frame("Sign in to a service with a very long title?", &long, &long);
    assert!(f.lit() > 0);
    let mut idle = Frame::new();
    let _ = render_idle_narrow(&mut idle);
    assert!(idle.lit() > 0);
}

#[test]
fn control_bytes_never_reach_the_frame() {
    // Sanitizing lives in `Label`; this pins that the renderer gets only its output.
    let hostile = confirm_frame("Sign in?", b"ex\x1b[2Jample.com\xff", b"a\x00b");
    let plain = confirm_frame("Sign in?", b"ex?[2Jample.com?", b"a?b");
    assert!(hostile.px == plain.px);
}

#[test]
fn different_relying_parties_look_different() {
    let a = confirm_frame("Sign in?", b"example.com", b"alice");
    let b = confirm_frame("Sign in?", b"example.org", b"alice");
    let c = confirm_frame("Sign in?", b"example.com", b"bob");
    assert!(a.px != b.px);
    assert!(a.px != c.px);
}

#[test]
fn a_cut_domain_is_marked() {
    let tail = [b'a'; 48];
    let mut longer = [b'z'; 60];
    longer[12..].copy_from_slice(&tail);
    let whole = confirm_frame("Sign in?", &tail, b"");
    let cut = confirm_frame("Sign in?", &longer, b"");
    assert!(
        whole.px != cut.px,
        "a head-cut id looked like a complete one"
    );
}

#[test]
fn a_cut_account_is_marked() {
    let whole = confirm_frame("Sign in?", b"example.com", &[b'u'; 48]);
    let cut = confirm_frame("Sign in?", b"example.com", &[b'u'; 60]);
    assert!(whole.px != cut.px, "a tail-cut account looked complete");
}

#[test]
fn prompt_without_account_has_no_account_caption() {
    let with = confirm_frame("Sign in?", b"example.com", b"alice");
    let without = confirm_frame("Sign in?", b"example.com", b"");
    assert!(with.lit() > without.lit());
}
