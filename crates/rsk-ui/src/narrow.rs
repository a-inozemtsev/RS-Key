// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 RS-Key contributors

//! The reduced screens for a 172x320 panel without a touch controller (Waveshare
//! RP2350-LCD-1.47-A). The full UI is laid out in absolute coordinates for 240x320
//! and needs a finger; this panel has a single button, so it shows only what the
//! button can answer: the trusted confirm prompt (the real relying party and
//! account, then "press BOOT") and an idle card.
//!
//! Same contract as [`crate::render()`]: pure, generic over
//! `DrawTarget<Color = Rgb565>`, untrusted text arrives only as a sanitized
//! [`Label`], and nothing draws outside the panel. A relying-party id too long for
//! its box wraps; one that still does not fit is marked with an ellipsis rather
//! than clipped silently.

use embedded_graphics::{
    Drawable,
    draw_target::DrawTarget,
    geometry::{Point as EgPoint, Size},
    pixelcolor::Rgb565,
    primitives::{Primitive, PrimitiveStyle, Rectangle},
};

use crate::font::{self, Role};
use crate::{ConfirmPrompt, LABEL_MAX, Label, theme};

/// Panel width in pixels.
pub const NARROW_W: u16 = 172;
/// Panel height in pixels.
pub const NARROW_H: u16 = 320;

/// Left and right inset of every text block.
const MARGIN: i32 = 12;
/// Width available to a text block.
const TEXT_W: u32 = NARROW_W as u32 - 2 * MARGIN as u32;
/// Distance between baselines' vertical centres in a wrapped block.
const LINE_H: i32 = 17;

const TITLE_LINES: usize = 2;
/// A domain keeps its tail, so the cut that [`Label::clamp_domain`] reports is
/// shown as a leading `...` — three more characters than [`LABEL_MAX`]. The widest
/// glyph still fits five lines (`tests::worst_case_label_never_cuts`).
const PRIMARY_LINES: usize = 5;
const SECONDARY_LINES: usize = 3;
const MAX_LINES: usize = 5;
const ELLIPSIS: &str = "...";
const MAX_TEXT: usize = LABEL_MAX + ELLIPSIS.len();

const TITLE_CY: i32 = 24;
const DIVIDER_Y: i32 = 58;
const PRIMARY_CAPTION_CY: i32 = 74;
const PRIMARY_CY: i32 = 94;
const SECONDARY_CAPTION_CY: i32 = 182;
const SECONDARY_CY: i32 = 200;
const HINT_DIVIDER_Y: i32 = 252;
const HINT_CY: i32 = 272;

const TITLE_ROLE: Role = Role::BodyStrong;
const PRIMARY_ROLE: Role = Role::BodyStrong;
const SECONDARY_ROLE: Role = Role::Body;

/// The text of one wrapped block: byte ranges into the source, and whether the
/// source did not fit the line budget (the last line then has room for `...`).
struct Wrapped {
    lines: [(usize, usize); MAX_LINES],
    count: usize,
    cut: bool,
}

fn fits(text: &str, role: Role) -> bool {
    font::width(text, role).is_some_and(|w| w <= TEXT_W)
}

/// Greedy character wrap of printable ASCII into at most `max_lines` lines of
/// [`TEXT_W`] pixels. Always makes progress (a glyph wider than the box takes a
/// line to itself). On overflow the last line is shortened until `...` fits after it.
fn wrap(text: &str, role: Role, max_lines: usize) -> Wrapped {
    let mut out = Wrapped {
        lines: [(0, 0); MAX_LINES],
        count: 0,
        cut: false,
    };
    let len = text.len();
    let mut start = 0;
    while start < len {
        let mut end = start;
        while end < len && text.get(start..=end).is_some_and(|s| fits(s, role)) {
            end += 1;
        }
        if end == start {
            end = start + 1;
        }
        if out.count == max_lines {
            out.cut = true;
            let (s, mut e) = out.lines[max_lines - 1];
            let dots = font::width(ELLIPSIS, role).unwrap_or(0);
            while e > s + 1
                && text
                    .get(s..e)
                    .is_none_or(|t| font::width(t, role).unwrap_or(u32::MAX) + dots > TEXT_W)
            {
                e -= 1;
            }
            out.lines[max_lines - 1] = (s, e);
            break;
        }
        out.lines[out.count] = (start, end);
        out.count += 1;
        start = end;
    }
    out
}

/// Draw `text` wrapped, starting at vertical centre `cy`. Returns nothing drawn
/// beyond `max_lines` lines.
fn block<D: DrawTarget<Color = Rgb565>>(
    t: &mut D,
    text: &str,
    cy: i32,
    role: Role,
    color: Rgb565,
    max_lines: usize,
) -> Result<(), D::Error> {
    let w = wrap(text, role, max_lines);
    for i in 0..w.count {
        let (s, e) = w.lines[i];
        let Some(line) = text.get(s..e) else {
            continue;
        };
        let y = cy + LINE_H * i as i32;
        font::left(t, line, EgPoint::new(MARGIN, y), role, color, theme::BG)?;
        if w.cut && i + 1 == w.count {
            let x = MARGIN + font::width(line, role).unwrap_or(0) as i32;
            font::left(t, ELLIPSIS, EgPoint::new(x, y), role, color, theme::BG)?;
        }
    }
    Ok(())
}

fn fill<D: DrawTarget<Color = Rgb565>>(
    t: &mut D,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    color: Rgb565,
) -> Result<(), D::Error> {
    Rectangle::new(EgPoint::new(x, y), Size::new(w, h))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(t)
}

fn background<D: DrawTarget<Color = Rgb565>>(t: &mut D) -> Result<(), D::Error> {
    fill(t, 0, 0, NARROW_W.into(), NARROW_H.into(), theme::BG)
}

fn rule<D: DrawTarget<Color = Rgb565>>(t: &mut D, y: i32) -> Result<(), D::Error> {
    fill(t, MARGIN, y, TEXT_W, 1, theme::DIVIDER)
}

/// A relying-party id for display: the tail the label kept, behind a leading `...`
/// when [`Label::clamp_domain`] cut the head.
fn with_head_mark(label: &Label, out: &mut [u8; MAX_TEXT]) -> usize {
    let text = label.as_str().as_bytes();
    let mut n = 0;
    if label.truncated {
        out[..ELLIPSIS.len()].copy_from_slice(ELLIPSIS.as_bytes());
        n = ELLIPSIS.len();
    }
    out[n..n + text.len()].copy_from_slice(text);
    n + text.len()
}

/// The trusted confirm prompt. `prompt.primary` is a relying-party id (clamped by
/// [`ConfirmPrompt::new`] with [`Label::clamp_domain`], so a truncated one keeps
/// its registrable suffix); `prompt.secondary` is the account. The footer names
/// the one gesture this panel can read.
pub fn render_confirm_narrow<D: DrawTarget<Color = Rgb565>>(
    t: &mut D,
    prompt: &ConfirmPrompt,
) -> Result<(), D::Error> {
    background(t)?;
    block(
        t,
        prompt.title,
        TITLE_CY,
        TITLE_ROLE,
        theme::TEXT,
        TITLE_LINES,
    )?;
    rule(t, DIVIDER_Y)?;

    if !prompt.primary.is_empty() {
        font::left(
            t,
            "SITE",
            EgPoint::new(MARGIN, PRIMARY_CAPTION_CY),
            Role::MonoSmall,
            theme::CAPTION,
            theme::BG,
        )?;
        let mut buf = [0u8; MAX_TEXT];
        let n = with_head_mark(&prompt.primary, &mut buf);
        let text = core::str::from_utf8(&buf[..n]).unwrap_or("");
        block(
            t,
            text,
            PRIMARY_CY,
            PRIMARY_ROLE,
            theme::TEXT,
            PRIMARY_LINES,
        )?;
    }
    if !prompt.secondary.is_empty() {
        font::left(
            t,
            "ACCOUNT",
            EgPoint::new(MARGIN, SECONDARY_CAPTION_CY),
            Role::MonoSmall,
            theme::CAPTION,
            theme::BG,
        )?;
        let mut text = prompt.secondary.as_str();
        // The head is what identifies an account; a cut one ends in `...`.
        let marked;
        if prompt.secondary.truncated {
            let mut buf = [0u8; MAX_TEXT];
            let n = text.len();
            buf[..n].copy_from_slice(text.as_bytes());
            buf[n..n + ELLIPSIS.len()].copy_from_slice(ELLIPSIS.as_bytes());
            marked = buf;
            text = core::str::from_utf8(&marked[..n + ELLIPSIS.len()]).unwrap_or("");
        }
        block(
            t,
            text,
            SECONDARY_CY,
            SECONDARY_ROLE,
            theme::TEXT_2,
            SECONDARY_LINES,
        )?;
    }

    rule(t, HINT_DIVIDER_Y)?;
    font::left(
        t,
        "Press BOOT to approve",
        EgPoint::new(MARGIN, HINT_CY),
        Role::BodyStrong,
        theme::ACCENT_TEXT,
        theme::BG,
    )?;
    font::left(
        t,
        "Wait to cancel",
        EgPoint::new(MARGIN, HINT_CY + LINE_H + 3),
        Role::Body,
        theme::MUTED,
        theme::BG,
    )
}

/// The card shown when no ceremony is pending.
pub fn render_idle_narrow<D: DrawTarget<Color = Rgb565>>(t: &mut D) -> Result<(), D::Error> {
    background(t)?;
    font::centered(
        t,
        "RS-Key",
        EgPoint::new(i32::from(NARROW_W) / 2, 140),
        Role::Ready,
        theme::TEXT,
        theme::BG,
    )?;
    font::centered(
        t,
        "Ready",
        EgPoint::new(i32::from(NARROW_W) / 2, 180),
        Role::Body,
        theme::MUTED,
        theme::BG,
    )
}

#[cfg(test)]
#[path = "narrow_tests.rs"]
mod tests;
