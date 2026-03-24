use super::{OverlayMsg, OverlayState};
use objc2::rc::Retained;
use objc2::{msg_send, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSColor, NSFont, NSFontWeightRegular,
    NSLineBreakMode, NSPanel, NSScreen, NSTextAlignment, NSTextField,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
    NSVisualEffectView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};
use std::sync::mpsc;
use std::time::{Duration, Instant};

// -- Layout constants --------------------------------------------------------
const OVERLAY_WIDTH: f64 = 480.0;
const OVERLAY_MIN_HEIGHT: f64 = 40.0;
const OVERLAY_MAX_HEIGHT: f64 = 200.0;
const CORNER_RADIUS: f64 = 12.0;
const BOTTOM_MARGIN: f64 = 120.0;
const STRIPE_WIDTH: f64 = 3.0;
const TEXT_LEFT_INSET: f64 = 20.0;
const TEXT_RIGHT_PAD: f64 = 16.0;
const VERTICAL_PAD: f64 = 8.0;

// -- Stripe pulse timing -----------------------------------------------------
const PULSE_HALF_CYCLE: Duration = Duration::from_millis(600);
const STRIPE_OPACITY_DIM: f32 = 0.4;
const STRIPE_OPACITY_BRIGHT: f32 = 1.0;

// -- Fade timing --------------------------------------------------------------
const FADE_IN_STEPS: usize = 9;
const FADE_IN_STEP_MS: u64 = 16;
const FADE_OUT_STEPS: usize = 7;
const FADE_OUT_STEP_MS: u64 = 16;

/// Run the overlay on the main thread. This function does not return until
/// the app is quit (via OverlayMsg::Quit or channel disconnect).
pub fn run_overlay(state: OverlayState) {
    let mtm =
        MainThreadMarker::new().expect("run_overlay must be called from the main thread");

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    // --- Create the floating panel ---
    let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
    let screen = NSScreen::mainScreen(mtm).expect("no main screen");
    let screen_frame = screen.frame();

    let x = (screen_frame.size.width - OVERLAY_WIDTH) / 2.0;
    let y = BOTTOM_MARGIN;
    let content_rect = NSRect::new(
        NSPoint::new(x, y),
        NSSize::new(OVERLAY_WIDTH, OVERLAY_MIN_HEIGHT),
    );

    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        content_rect,
        style,
        objc2_app_kit::NSBackingStoreType(2),
        false,
    );

    panel.setLevel(25); // NSStatusWindowLevel
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(true);
    panel.setMovableByWindowBackground(true);
    panel.setIgnoresMouseEvents(true);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );

    // --- NSVisualEffectView as background (vibrancy / blur) ---
    let effect_view = {
        let local_frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(OVERLAY_WIDTH, OVERLAY_MIN_HEIGHT),
        );
        let view = NSVisualEffectView::initWithFrame(
            NSVisualEffectView::alloc(mtm),
            local_frame,
        );
        view.setMaterial(NSVisualEffectMaterial::HUDWindow);
        view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        view.setState(NSVisualEffectState::Active);
        view.setWantsLayer(true);

        // Force dark appearance
        unsafe {
            let appearance_name = NSString::from_str("NSAppearanceNameDarkAqua");
            let appearance: Option<Retained<objc2::runtime::NSObject>> = msg_send![
                objc2::class!(NSAppearance),
                appearanceNamed: &*appearance_name
            ];
            if let Some(app_obj) = appearance {
                let _: () = msg_send![&view, setAppearance: &*app_obj];
            }
        }

        // Corner radius + shadow
        unsafe {
            let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&view, layer];
            let _: () = msg_send![&layer, setCornerRadius: CORNER_RADIUS];
            let _: () = msg_send![&layer, setMasksToBounds: false];
            let _: () = msg_send![&layer, setShadowOpacity: 0.5f32];
            let _: () = msg_send![&layer, setShadowRadius: 20.0f64];
            let shadow_offset = NSSize::new(0.0, -4.0);
            let _: () = msg_send![&layer, setShadowOffset: shadow_offset];
            let black = NSColor::colorWithSRGBRed_green_blue_alpha(0.0, 0.0, 0.0, 1.0);
            let cg_color: *const std::ffi::c_void = msg_send![&black, CGColor];
            let _: () = msg_send![&layer, setShadowColor: cg_color];
        }

        view
    };

    // --- Dark tint layer ---
    let tint_view = {
        let local_frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(OVERLAY_WIDTH, OVERLAY_MIN_HEIGHT),
        );
        let view = objc2_app_kit::NSView::initWithFrame(
            objc2_app_kit::NSView::alloc(mtm),
            local_frame,
        );
        view.setWantsLayer(true);
        unsafe {
            let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&view, layer];
            let tint_color = NSColor::colorWithSRGBRed_green_blue_alpha(
                10.0 / 255.0,
                14.0 / 255.0,
                20.0 / 255.0,
                0.70,
            );
            let cg_color: *const std::ffi::c_void = msg_send![&tint_color, CGColor];
            let _: () = msg_send![&layer, setBackgroundColor: cg_color];
            let _: () = msg_send![&layer, setCornerRadius: CORNER_RADIUS];
            let _: () = msg_send![&layer, setMasksToBounds: true];
        }
        view
    };

    // --- Left accent stripe (electric blue) ---
    let stripe_view = {
        let stripe_frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(STRIPE_WIDTH, OVERLAY_MIN_HEIGHT),
        );
        let view = objc2_app_kit::NSView::initWithFrame(
            objc2_app_kit::NSView::alloc(mtm),
            stripe_frame,
        );
        view.setWantsLayer(true);
        unsafe {
            let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&view, layer];
            let stripe_color = NSColor::colorWithSRGBRed_green_blue_alpha(
                59.0 / 255.0,
                130.0 / 255.0,
                246.0 / 255.0,
                1.0,
            );
            let cg_color: *const std::ffi::c_void = msg_send![&stripe_color, CGColor];
            let _: () = msg_send![&layer, setBackgroundColor: cg_color];
            let _: () = msg_send![&layer, setCornerRadius: CORNER_RADIUS];
            let corner_mask: usize = 1 | 4; // top-left + bottom-left
            let _: () = msg_send![&layer, setMaskedCorners: corner_mask];
        }
        view
    };

    // --- "Listening..." label ---
    let text_width = OVERLAY_WIDTH - TEXT_LEFT_INSET - TEXT_RIGHT_PAD;
    let listening_label = {
        let label = NSTextField::initWithFrame(
            NSTextField::alloc(mtm),
            NSRect::new(
                NSPoint::new(TEXT_LEFT_INSET, VERTICAL_PAD),
                NSSize::new(text_width, OVERLAY_MIN_HEIGHT - 2.0 * VERTICAL_PAD),
            ),
        );
        label.setEditable(false);
        label.setBezeled(false);
        label.setDrawsBackground(false);
        label.setSelectable(false);
        label.setStringValue(&NSString::from_str("Listening..."));
        {
            let weight = unsafe { NSFontWeightRegular };
            let font = NSFont::monospacedSystemFontOfSize_weight(14.0, weight);
            label.setFont(Some(&font));
        }
        let slate_grey = NSColor::colorWithSRGBRed_green_blue_alpha(
            100.0 / 255.0,
            116.0 / 255.0,
            139.0 / 255.0,
            1.0,
        );
        label.setTextColor(Some(&slate_grey));
        label.setAlignment(NSTextAlignment::Left);
        label
    };

    // --- Transcript text label ---
    let text_label = {
        let label = NSTextField::initWithFrame(
            NSTextField::alloc(mtm),
            NSRect::new(
                NSPoint::new(TEXT_LEFT_INSET, VERTICAL_PAD),
                NSSize::new(text_width, OVERLAY_MIN_HEIGHT - 2.0 * VERTICAL_PAD),
            ),
        );
        label.setEditable(false);
        label.setBezeled(false);
        label.setDrawsBackground(false);
        label.setSelectable(false);
        label.setStringValue(&NSString::from_str(""));
        {
            let weight = unsafe { NSFontWeightRegular };
            let font = NSFont::monospacedSystemFontOfSize_weight(14.0, weight);
            label.setFont(Some(&font));
        }
        let text_color = NSColor::colorWithSRGBRed_green_blue_alpha(
            226.0 / 255.0,
            232.0 / 255.0,
            240.0 / 255.0,
            1.0,
        );
        label.setTextColor(Some(&text_color));
        label.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        unsafe {
            let cell: Option<Retained<objc2_app_kit::NSCell>> = msg_send![&label, cell];
            if let Some(cell) = cell {
                let _: () = msg_send![&cell, setWraps: true];
            }
        }
        label.setMaximumNumberOfLines(0);
        label
    };

    // Assemble view hierarchy
    effect_view.addSubview(&tint_view);
    effect_view.addSubview(&stripe_view);
    effect_view.addSubview(&listening_label);
    effect_view.addSubview(&text_label);
    panel.setContentView(Some(&effect_view));

    // Start hidden
    panel.orderOut(None);

    // --- Run loop ---
    app.finishLaunching();

    let receiver = state.receiver;

    let mut pulse_bright = true;
    let mut pulse_last_toggle = Instant::now();
    let mut is_pulsing = false;
    let mut has_transcript = false;

    let mut fade_in_remaining: usize = 0;
    let mut fade_out_remaining: usize = 0;
    let mut fade_last_step = Instant::now();

    loop {
        drain_events(&app);

        // Stripe pulse
        if is_pulsing && pulse_last_toggle.elapsed() >= PULSE_HALF_CYCLE {
            pulse_bright = !pulse_bright;
            pulse_last_toggle = Instant::now();
            let opacity = if pulse_bright {
                STRIPE_OPACITY_BRIGHT
            } else {
                STRIPE_OPACITY_DIM
            };
            set_view_opacity(&stripe_view, opacity);
        }

        // Fade-in
        if fade_in_remaining > 0
            && fade_last_step.elapsed() >= Duration::from_millis(FADE_IN_STEP_MS)
        {
            fade_in_remaining -= 1;
            let progress = 1.0 - (fade_in_remaining as f64 / FADE_IN_STEPS as f64);
            unsafe {
                let _: () = msg_send![&panel, setAlphaValue: progress];
            }
            fade_last_step = Instant::now();
        }

        // Fade-out
        if fade_out_remaining > 0
            && fade_last_step.elapsed() >= Duration::from_millis(FADE_OUT_STEP_MS)
        {
            fade_out_remaining -= 1;
            let progress = fade_out_remaining as f64 / FADE_OUT_STEPS as f64;
            unsafe {
                let _: () = msg_send![&panel, setAlphaValue: progress];
            }
            fade_last_step = Instant::now();
            if fade_out_remaining == 0 {
                panel.orderOut(None);
            }
        }

        match receiver.recv_timeout(Duration::from_millis(16)) {
            Ok(OverlayMsg::Show) => {
                has_transcript = false;
                is_pulsing = true;
                pulse_bright = true;
                pulse_last_toggle = Instant::now();
                set_view_opacity(&stripe_view, STRIPE_OPACITY_BRIGHT);

                listening_label.setHidden(false);
                text_label.setStringValue(&NSString::from_str(""));
                text_label.setHidden(true);

                resize_overlay(
                    &panel,
                    &effect_view,
                    &tint_view,
                    &stripe_view,
                    &text_label,
                    &listening_label,
                    screen_frame,
                    OVERLAY_MIN_HEIGHT,
                );

                fade_out_remaining = 0;
                unsafe {
                    let _: () = msg_send![&panel, setAlphaValue: 0.0f64];
                }
                panel.orderFront(None);
                fade_in_remaining = FADE_IN_STEPS;
                fade_last_step = Instant::now();
            }
            Ok(OverlayMsg::UpdateText(text)) => {
                if text.is_empty() {
                    if has_transcript {
                        has_transcript = false;
                        is_pulsing = true;
                        pulse_bright = true;
                        pulse_last_toggle = Instant::now();
                        set_view_opacity(&stripe_view, STRIPE_OPACITY_BRIGHT);
                        listening_label.setHidden(false);
                        text_label.setHidden(true);
                    }
                    text_label.setStringValue(&NSString::from_str(""));
                    resize_overlay(
                        &panel,
                        &effect_view,
                        &tint_view,
                        &stripe_view,
                        &text_label,
                        &listening_label,
                        screen_frame,
                        OVERLAY_MIN_HEIGHT,
                    );
                } else {
                    if !has_transcript {
                        has_transcript = true;
                        is_pulsing = false;
                        set_view_opacity(&stripe_view, STRIPE_OPACITY_BRIGHT);
                        listening_label.setHidden(true);
                        text_label.setHidden(false);
                    }
                    text_label.setStringValue(&NSString::from_str(&text));
                    let needed =
                        compute_text_height(&text_label, text_width) + 2.0 * VERTICAL_PAD;
                    let new_height = if needed.is_finite() && needed > OVERLAY_MIN_HEIGHT {
                        if needed < OVERLAY_MAX_HEIGHT {
                            needed
                        } else {
                            OVERLAY_MAX_HEIGHT
                        }
                    } else {
                        OVERLAY_MIN_HEIGHT
                    };
                    resize_overlay(
                        &panel,
                        &effect_view,
                        &tint_view,
                        &stripe_view,
                        &text_label,
                        &listening_label,
                        screen_frame,
                        new_height,
                    );
                }
            }
            Ok(OverlayMsg::Hide) => {
                is_pulsing = false;
                fade_in_remaining = 0;
                fade_out_remaining = FADE_OUT_STEPS;
                fade_last_step = Instant::now();
            }
            Ok(OverlayMsg::Quit) => {
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }
}

fn set_view_opacity(view: &objc2_app_kit::NSView, opacity: f32) {
    unsafe {
        let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&*view, layer];
        let _: () = msg_send![&layer, setOpacity: opacity];
    }
}

/// Estimate text height using a character-width heuristic.
fn compute_text_height(_label: &NSTextField, width: f64) -> f64 {
    if width <= 0.0 || !width.is_finite() {
        return OVERLAY_MIN_HEIGHT - 2.0 * VERTICAL_PAD;
    }

    let nsstr: Retained<NSString> = _label.stringValue();
    let text = nsstr.to_string();

    if text.is_empty() {
        return OVERLAY_MIN_HEIGHT - 2.0 * VERTICAL_PAD;
    }

    // SF Mono 14pt: average char width ~8.4px
    let avg_char_width: f64 = 8.4;
    let chars_per_line = (width / avg_char_width).max(1.0) as usize;

    let mut line_count: usize = 0;
    for line in text.split('\n') {
        let len = line.len();
        if len == 0 {
            line_count += 1;
        } else {
            line_count += (len + chars_per_line - 1) / chars_per_line;
        }
    }
    line_count = line_count.max(1);

    let line_height: f64 = 18.0;
    let h = line_count as f64 * line_height;

    if h.is_finite() && h > 0.0 {
        h
    } else {
        OVERLAY_MIN_HEIGHT - 2.0 * VERTICAL_PAD
    }
}

fn resize_overlay(
    panel: &NSPanel,
    effect_view: &NSVisualEffectView,
    tint_view: &objc2_app_kit::NSView,
    stripe_view: &objc2_app_kit::NSView,
    text_label: &NSTextField,
    listening_label: &NSTextField,
    screen_frame: NSRect,
    new_height: f64,
) {
    let new_height = if new_height.is_finite() && new_height >= OVERLAY_MIN_HEIGHT {
        if new_height <= OVERLAY_MAX_HEIGHT {
            new_height
        } else {
            OVERLAY_MAX_HEIGHT
        }
    } else {
        OVERLAY_MIN_HEIGHT
    };

    let x = (screen_frame.size.width - OVERLAY_WIDTH) / 2.0;
    let y = BOTTOM_MARGIN;
    let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(OVERLAY_WIDTH, new_height));
    panel.setFrame_display(frame, true);

    let content_frame = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(OVERLAY_WIDTH, new_height),
    );
    effect_view.setFrame(content_frame);
    tint_view.setFrame(content_frame);

    unsafe {
        let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&*effect_view, layer];
        let _: () = msg_send![&layer, setCornerRadius: CORNER_RADIUS];
    }
    unsafe {
        let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&*tint_view, layer];
        let _: () = msg_send![&layer, setCornerRadius: CORNER_RADIUS];
        let _: () = msg_send![&layer, setMasksToBounds: true];
    }

    let stripe_frame = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(STRIPE_WIDTH, new_height),
    );
    stripe_view.setFrame(stripe_frame);
    unsafe {
        let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&*stripe_view, layer];
        let _: () = msg_send![&layer, setCornerRadius: CORNER_RADIUS];
        let corner_mask: usize = 1 | 4;
        let _: () = msg_send![&layer, setMaskedCorners: corner_mask];
    }

    let text_width = OVERLAY_WIDTH - TEXT_LEFT_INSET - TEXT_RIGHT_PAD;
    let text_height = (new_height - 2.0 * VERTICAL_PAD).max(1.0);
    let text_frame = NSRect::new(
        NSPoint::new(TEXT_LEFT_INSET, VERTICAL_PAD),
        NSSize::new(text_width, text_height),
    );
    text_label.setFrame(text_frame);
    listening_label.setFrame(text_frame);
}

fn drain_events(app: &NSApplication) {
    loop {
        let event = unsafe {
            app.nextEventMatchingMask_untilDate_inMode_dequeue(
                objc2_app_kit::NSEventMask::Any,
                None,
                objc2_foundation::NSDefaultRunLoopMode,
                true,
            )
        };
        match event {
            Some(event) => {
                app.sendEvent(&event);
            }
            None => break,
        }
    }
}
