// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 RS-Key contributors

//! The `lcd-confirm` build: a screen without a touch controller (Waveshare
//! RP2350-LCD-1.47-A, ST7789V3 172x320 over PIO).
//!
//! The button stays the presence source, so the whole confirm path — the arbiter,
//! the transports' keepalives, cancel and timeout — is the stock [`ButtonPresence`]
//! wait. What this adds is the one thing a button cannot do alone: say *what* it is
//! approving. Before the wait the panel paints the trusted prompt (the real relying
//! party and account, [`rsk_ui::narrow`]); after it, the idle card.
//!
//! There is no Deny: the button has no decline gesture, so declining is letting the
//! wait time out, and the prompt says so. That is also why [`LcdPresence`] does not
//! claim `shows_confirm` — the panel is a display of the request, not the trust
//! anchor the touch build's Approve/Deny is, and the `authenticatorReset` window
//! exemption that claim buys is not this build's to take.

use embassy_rp::gpio::Output;
use embassy_rp::pwm::{Config as PwmConfig, Pwm};
use mipidsi::options::{ColorInversion, ColorOrder};

use rsk_sdk::{Confirm, Presence, UserPresence};
use rsk_ui::ConfirmPrompt;

use crate::presence::ButtonPresence;

#[path = "display_panel.rs"]
mod display_panel;
use display_panel::Panel;
pub(crate) use display_panel::PioDisplayTx;

/// Backlight PWM counter top; `duty` is the on-fraction of it.
const BL_TOP: u16 = 255;
/// Dim while idle, full while a request waits for the button.
const BL_IDLE: u16 = BL_TOP / 4;
const BL_ACTIVE: u16 = BL_TOP;

/// PWM config for the backlight. Both compare registers carry `duty`: the pad is on
/// channel A or B of its slice depending on the board, and the other channel drives
/// nothing, so setting only one would leave a B-channel backlight dark for good.
pub fn backlight_cfg(duty: u16) -> PwmConfig {
    let mut cfg = PwmConfig::default();
    cfg.top = BL_TOP;
    cfg.compare_a = duty.min(BL_TOP);
    cfg.compare_b = duty.min(BL_TOP);
    cfg
}

/// The panel's bus, control pins and backlight, bundled for `main`.
pub struct PanelHw {
    pub spi: PioDisplayTx,
    pub cs: Output<'static>,
    pub dc: Output<'static>,
    pub rst: Output<'static>,
    pub bl: Pwm<'static>,
}

/// [`ButtonPresence`] with a confirm screen in front of the wait.
pub struct LcdPresence {
    inner: ButtonPresence,
    panel: Panel,
    // Held for the device's lifetime: dropping an embassy PWM disconnects the pad.
    bl: Pwm<'static>,
}

impl LcdPresence {
    /// Bring the panel up (its reset takes ~370 ms) and paint the idle card.
    pub fn new(inner: ButtonPresence, hw: PanelHw) -> Self {
        let invert = if crate::BUILD_DISPLAY_INVERT_COLORS {
            ColorInversion::Inverted
        } else {
            ColorInversion::Normal
        };
        let color_order = match crate::BUILD_DISPLAY_COLOR_ORDER {
            1 => ColorOrder::Bgr,
            _ => ColorOrder::Rgb,
        };
        // The damage key only seeds the retained-scene path; this build draws directly.
        let panel = Panel::new(hw.spi, hw.cs, hw.dc, hw.rst, [0; 2], invert, color_order);
        let mut this = Self {
            inner,
            panel,
            bl: hw.bl,
        };
        this.show_idle();
        this
    }

    fn backlight(&mut self, duty: u16) {
        self.bl.set_config(&backlight_cfg(duty));
    }

    /// Paint, then light: the controller's RAM is undefined after reset, so the
    /// backlight comes up only once there is something on it.
    fn show_idle(&mut self) {
        let _ = rsk_ui::narrow::render_idle_narrow(&mut self.panel);
        self.backlight(BL_IDLE);
    }

    fn show_prompt(&mut self, confirm: &Confirm<'_>) {
        let prompt = ConfirmPrompt::new(confirm.title, confirm.primary, confirm.secondary);
        let _ = rsk_ui::narrow::render_confirm_narrow(&mut self.panel, &prompt);
        self.backlight(BL_ACTIVE);
    }

    /// One non-blocking sample of the button, for the typed-ticket watcher.
    pub fn poll_pressed(&mut self) -> bool {
        self.inner.poll_pressed()
    }
}

impl UserPresence for LcdPresence {
    fn request(&mut self, confirm: Confirm<'_>) -> Presence {
        self.show_prompt(&confirm);
        let outcome = self.inner.request(confirm);
        self.show_idle();
        outcome
    }

    fn request_ceremony(&mut self, confirm: Confirm<'_>) -> Presence {
        self.show_prompt(&confirm);
        let outcome = self.inner.request_ceremony(confirm);
        self.show_idle();
        outcome
    }
}
