use super::{OverlayMsg, OverlayState, OverlayStyle};
use objc2::rc::Retained;
use objc2::{msg_send, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSColor, NSFont, NSFontWeightMedium,
    NSFontWeightRegular, NSFontWeightSemibold, NSLineBreakMode, NSPanel, NSScreen,
    NSTextAlignment, NSTextField, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};
use std::sync::mpsc;
use std::time::{Duration, Instant};

// =============================================================================
// Shared constants
// =============================================================================
const BOTTOM_MARGIN: f64 = 120.0;
const OVERLAY_MAX_HEIGHT: f64 = 200.0;

// =============================================================================
// Per-style constants
// =============================================================================

// -- Bifrost Slab -------------------------------------------------------------
mod bifrost {
    use std::time::Duration;
    pub const WIDTH: f64 = 480.0;
    pub const MIN_HEIGHT: f64 = 40.0;
    pub const CORNER_RADIUS: f64 = 12.0;
    pub const STRIPE_WIDTH: f64 = 3.0;
    pub const TEXT_LEFT_INSET: f64 = 20.0;
    pub const TEXT_RIGHT_PAD: f64 = 16.0;
    pub const VERTICAL_PAD: f64 = 8.0;
    pub const PULSE_HALF_CYCLE: Duration = Duration::from_millis(600);
    pub const STRIPE_OPACITY_DIM: f32 = 0.4;
    pub const STRIPE_OPACITY_BRIGHT: f32 = 1.0;
    pub const FADE_IN_STEPS: usize = 9;
    pub const FADE_IN_STEP_MS: u64 = 16;
    pub const FADE_OUT_STEPS: usize = 7;
    pub const FADE_OUT_STEP_MS: u64 = 16;
    pub const FONT_SIZE: f64 = 14.0;
    pub const AVG_CHAR_WIDTH: f64 = 8.4;
    pub const LINE_HEIGHT: f64 = 18.0;
}

// -- Stormforge HUD -----------------------------------------------------------
mod stormforge {
    use std::time::Duration;
    pub const WIDTH: f64 = 480.0;
    pub const MIN_HEIGHT: f64 = 44.0;
    pub const CORNER_RADIUS: f64 = 10.0;
    pub const DOT_DIAMETER: f64 = 8.0;
    pub const DOT_X: f64 = 16.0;
    pub const TEXT_LEFT_INSET: f64 = 38.0;
    pub const TEXT_RIGHT_PAD: f64 = 16.0;
    pub const VERTICAL_PAD: f64 = 8.0;
    pub const BORDER_WIDTH: f64 = 1.0;
    pub const PULSE_HALF_CYCLE: Duration = Duration::from_millis(400);
    pub const DOT_OPACITY_DIM: f32 = 0.6;
    pub const DOT_OPACITY_BRIGHT: f32 = 1.0;
    pub const FADE_IN_STEPS: usize = 12;
    pub const FADE_IN_STEP_MS: u64 = 16;
    pub const FADE_OUT_STEPS: usize = 9;
    pub const FADE_OUT_STEP_MS: u64 = 16;
    pub const FONT_SIZE: f64 = 15.0;
    pub const LISTENING_FONT_SIZE: f64 = 15.0;
    pub const AVG_CHAR_WIDTH: f64 = 8.8;
    pub const LINE_HEIGHT: f64 = 20.0;
}

// -- Uru Terminal -------------------------------------------------------------
mod uru {
    pub const WIDTH: f64 = 440.0;
    pub const MIN_HEIGHT: f64 = 36.0;
    pub const CORNER_RADIUS: f64 = 6.0;
    pub const CURSOR_X: f64 = 12.0;
    pub const TEXT_LEFT_INSET: f64 = 30.0;
    pub const TEXT_RIGHT_PAD: f64 = 12.0;
    pub const VERTICAL_PAD: f64 = 6.0;
    pub const BLINK_HALF_CYCLE_MS: u64 = 300;
    pub const FONT_SIZE: f64 = 14.0;
    pub const CURSOR_FONT_SIZE: f64 = 16.0;
    pub const AVG_CHAR_WIDTH: f64 = 8.4;
    pub const LINE_HEIGHT: f64 = 18.0;
}

// =============================================================================
// Style-parametric helpers
// =============================================================================

struct StyleParams {
    width: f64,
    min_height: f64,
    corner_radius: f64,
    text_left_inset: f64,
    text_right_pad: f64,
    vertical_pad: f64,
    avg_char_width: f64,
    line_height: f64,
}

fn style_params(style: OverlayStyle) -> StyleParams {
    match style {
        OverlayStyle::Bifrost => StyleParams {
            width: bifrost::WIDTH,
            min_height: bifrost::MIN_HEIGHT,
            corner_radius: bifrost::CORNER_RADIUS,
            text_left_inset: bifrost::TEXT_LEFT_INSET,
            text_right_pad: bifrost::TEXT_RIGHT_PAD,
            vertical_pad: bifrost::VERTICAL_PAD,
            avg_char_width: bifrost::AVG_CHAR_WIDTH,
            line_height: bifrost::LINE_HEIGHT,
        },
        OverlayStyle::Stormforge => StyleParams {
            width: stormforge::WIDTH,
            min_height: stormforge::MIN_HEIGHT,
            corner_radius: stormforge::CORNER_RADIUS,
            text_left_inset: stormforge::TEXT_LEFT_INSET,
            text_right_pad: stormforge::TEXT_RIGHT_PAD,
            vertical_pad: stormforge::VERTICAL_PAD,
            avg_char_width: stormforge::AVG_CHAR_WIDTH,
            line_height: stormforge::LINE_HEIGHT,
        },
        OverlayStyle::Uru => StyleParams {
            width: uru::WIDTH,
            min_height: uru::MIN_HEIGHT,
            corner_radius: uru::CORNER_RADIUS,
            text_left_inset: uru::TEXT_LEFT_INSET,
            text_right_pad: uru::TEXT_RIGHT_PAD,
            vertical_pad: uru::VERTICAL_PAD,
            avg_char_width: uru::AVG_CHAR_WIDTH,
            line_height: uru::LINE_HEIGHT,
        },
    }
}

// =============================================================================
// Public entry point
// =============================================================================

/// Run the overlay on the main thread. This function does not return until
/// the app is quit (via OverlayMsg::Quit or channel disconnect).
pub fn run_overlay(state: OverlayState) {
    let mtm =
        MainThreadMarker::new().expect("run_overlay must be called from the main thread");

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let style = state.style;
    let sp = style_params(style);

    // --- Create the floating panel ---
    let win_style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
    let screen = NSScreen::mainScreen(mtm).expect("no main screen");
    let screen_frame = screen.frame();

    let x = (screen_frame.size.width - sp.width) / 2.0;
    let y = BOTTOM_MARGIN;
    let content_rect = NSRect::new(
        NSPoint::new(x, y),
        NSSize::new(sp.width, sp.min_height),
    );

    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        content_rect,
        win_style,
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

    // Dispatch to style-specific setup and run loop
    match style {
        OverlayStyle::Bifrost => run_bifrost(mtm, app, panel, screen_frame, sp, state.receiver),
        OverlayStyle::Stormforge => {
            run_stormforge(mtm, app, panel, screen_frame, sp, state.receiver)
        }
        OverlayStyle::Uru => run_uru(mtm, app, panel, screen_frame, sp, state.receiver),
    }
}

// =============================================================================
// Bifrost Slab
// =============================================================================

fn run_bifrost(
    mtm: MainThreadMarker,
    app: Retained<NSApplication>,
    panel: Retained<NSPanel>,
    screen_frame: NSRect,
    sp: StyleParams,
    receiver: mpsc::Receiver<OverlayMsg>,
) {
    // --- NSVisualEffectView as background ---
    let effect_view = make_vibrancy_view(mtm, sp.width, sp.min_height, sp.corner_radius);

    // Shadow
    unsafe {
        let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&effect_view, layer];
        let _: () = msg_send![&layer, setShadowOpacity: 0.5f32];
        let _: () = msg_send![&layer, setShadowRadius: 20.0f64];
        let shadow_offset = NSSize::new(0.0, -4.0);
        let _: () = msg_send![&layer, setShadowOffset: shadow_offset];
        let black = NSColor::colorWithSRGBRed_green_blue_alpha(0.0, 0.0, 0.0, 1.0);
        let cg_color: *const std::ffi::c_void = msg_send![&black, CGColor];
        let _: () = msg_send![&layer, setShadowColor: cg_color];
    }

    // --- Dark tint layer (#0A0E14 at 70%) ---
    let tint_view =
        make_tint_view(mtm, sp.width, sp.min_height, sp.corner_radius, 10.0, 14.0, 20.0, 0.70);

    // --- Left accent stripe (electric blue #3B82F6) ---
    let stripe_view = {
        let stripe_frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(bifrost::STRIPE_WIDTH, sp.min_height),
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
            let _: () = msg_send![&layer, setCornerRadius: sp.corner_radius];
            let corner_mask: usize = 1 | 4; // top-left + bottom-left
            let _: () = msg_send![&layer, setMaskedCorners: corner_mask];
        }
        view
    };

    // --- Labels ---
    let text_width = sp.width - sp.text_left_inset - sp.text_right_pad;

    let listening_label = {
        let label = make_label(
            mtm,
            sp.text_left_inset,
            sp.vertical_pad,
            text_width,
            sp.min_height - 2.0 * sp.vertical_pad,
        );
        label.setStringValue(&NSString::from_str("Listening..."));
        let weight = unsafe { NSFontWeightRegular };
        let font = NSFont::monospacedSystemFontOfSize_weight(bifrost::FONT_SIZE, weight);
        label.setFont(Some(&font));
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

    let text_label = {
        let label = make_label(
            mtm,
            sp.text_left_inset,
            sp.vertical_pad,
            text_width,
            sp.min_height - 2.0 * sp.vertical_pad,
        );
        label.setStringValue(&NSString::from_str(""));
        let weight = unsafe { NSFontWeightRegular };
        let font = NSFont::monospacedSystemFontOfSize_weight(bifrost::FONT_SIZE, weight);
        label.setFont(Some(&font));
        let text_color = NSColor::colorWithSRGBRed_green_blue_alpha(
            226.0 / 255.0,
            232.0 / 255.0,
            240.0 / 255.0,
            1.0,
        );
        label.setTextColor(Some(&text_color));
        enable_wrapping(&label);
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
        if is_pulsing && pulse_last_toggle.elapsed() >= bifrost::PULSE_HALF_CYCLE {
            pulse_bright = !pulse_bright;
            pulse_last_toggle = Instant::now();
            let opacity = if pulse_bright {
                bifrost::STRIPE_OPACITY_BRIGHT
            } else {
                bifrost::STRIPE_OPACITY_DIM
            };
            set_view_opacity(&stripe_view, opacity);
        }

        // Fade-in
        if fade_in_remaining > 0
            && fade_last_step.elapsed() >= Duration::from_millis(bifrost::FADE_IN_STEP_MS)
        {
            fade_in_remaining -= 1;
            let progress =
                1.0 - (fade_in_remaining as f64 / bifrost::FADE_IN_STEPS as f64);
            unsafe {
                let _: () = msg_send![&panel, setAlphaValue: progress];
            }
            fade_last_step = Instant::now();
        }

        // Fade-out
        if fade_out_remaining > 0
            && fade_last_step.elapsed() >= Duration::from_millis(bifrost::FADE_OUT_STEP_MS)
        {
            fade_out_remaining -= 1;
            let progress = fade_out_remaining as f64 / bifrost::FADE_OUT_STEPS as f64;
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
                set_view_opacity(&stripe_view, bifrost::STRIPE_OPACITY_BRIGHT);

                listening_label.setHidden(false);
                text_label.setStringValue(&NSString::from_str(""));
                text_label.setHidden(true);

                resize_overlay_generic(
                    &panel,
                    &effect_view,
                    &[&tint_view, &stripe_view],
                    &[&text_label, &listening_label],
                    &sp,
                    screen_frame,
                    sp.min_height,
                    Some((&stripe_view, bifrost::STRIPE_WIDTH)),
                );

                fade_out_remaining = 0;
                unsafe {
                    let _: () = msg_send![&panel, setAlphaValue: 0.0f64];
                }
                panel.orderFront(None);
                fade_in_remaining = bifrost::FADE_IN_STEPS;
                fade_last_step = Instant::now();
            }
            Ok(OverlayMsg::UpdateText(text)) => {
                if text.is_empty() {
                    if has_transcript {
                        has_transcript = false;
                        is_pulsing = true;
                        pulse_bright = true;
                        pulse_last_toggle = Instant::now();
                        set_view_opacity(&stripe_view, bifrost::STRIPE_OPACITY_BRIGHT);
                        listening_label.setHidden(false);
                        text_label.setHidden(true);
                    }
                    text_label.setStringValue(&NSString::from_str(""));
                    resize_overlay_generic(
                        &panel,
                        &effect_view,
                        &[&tint_view, &stripe_view],
                        &[&text_label, &listening_label],
                        &sp,
                        screen_frame,
                        sp.min_height,
                        Some((&stripe_view, bifrost::STRIPE_WIDTH)),
                    );
                } else {
                    if !has_transcript {
                        has_transcript = true;
                        is_pulsing = false;
                        set_view_opacity(&stripe_view, bifrost::STRIPE_OPACITY_BRIGHT);
                        listening_label.setHidden(true);
                        text_label.setHidden(false);
                    }
                    text_label.setStringValue(&NSString::from_str(&text));
                    let new_height = compute_needed_height(&text_label, &sp);
                    resize_overlay_generic(
                        &panel,
                        &effect_view,
                        &[&tint_view, &stripe_view],
                        &[&text_label, &listening_label],
                        &sp,
                        screen_frame,
                        new_height,
                        Some((&stripe_view, bifrost::STRIPE_WIDTH)),
                    );
                }
            }
            Ok(OverlayMsg::Hide) => {
                is_pulsing = false;
                fade_in_remaining = 0;
                fade_out_remaining = bifrost::FADE_OUT_STEPS;
                fade_last_step = Instant::now();
            }
            Ok(OverlayMsg::Quit) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

// =============================================================================
// Stormforge HUD
// =============================================================================

fn run_stormforge(
    mtm: MainThreadMarker,
    app: Retained<NSApplication>,
    panel: Retained<NSPanel>,
    screen_frame: NSRect,
    sp: StyleParams,
    receiver: mpsc::Receiver<OverlayMsg>,
) {
    // --- NSVisualEffectView background ---
    let effect_view = make_vibrancy_view(mtm, sp.width, sp.min_height, sp.corner_radius);

    // Shadow: black opacity 0.6, radius 24pt, offset (0, -6)
    unsafe {
        let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&effect_view, layer];
        let _: () = msg_send![&layer, setShadowOpacity: 0.6f32];
        let _: () = msg_send![&layer, setShadowRadius: 24.0f64];
        let shadow_offset = NSSize::new(0.0, -6.0);
        let _: () = msg_send![&layer, setShadowOffset: shadow_offset];
        let black = NSColor::colorWithSRGBRed_green_blue_alpha(0.0, 0.0, 0.0, 1.0);
        let cg_color: *const std::ffi::c_void = msg_send![&black, CGColor];
        let _: () = msg_send![&layer, setShadowColor: cg_color];
        // 1pt border #2A2D35
        let _: () = msg_send![&layer, setBorderWidth: stormforge::BORDER_WIDTH];
        let border_color = NSColor::colorWithSRGBRed_green_blue_alpha(
            42.0 / 255.0,
            45.0 / 255.0,
            53.0 / 255.0,
            1.0,
        );
        let cg_border: *const std::ffi::c_void = msg_send![&border_color, CGColor];
        let _: () = msg_send![&layer, setBorderColor: cg_border];
    }

    // --- Dark tint layer (#111318 at 75%) ---
    let tint_view = make_tint_view(
        mtm,
        sp.width,
        sp.min_height,
        sp.corner_radius,
        17.0,
        19.0,
        24.0,
        0.75,
    );

    // --- Amber dot (#F59E0B) ---
    let dot_view = {
        let dot_y = (sp.min_height - stormforge::DOT_DIAMETER) / 2.0;
        let dot_frame = NSRect::new(
            NSPoint::new(stormforge::DOT_X, dot_y),
            NSSize::new(stormforge::DOT_DIAMETER, stormforge::DOT_DIAMETER),
        );
        let view = objc2_app_kit::NSView::initWithFrame(
            objc2_app_kit::NSView::alloc(mtm),
            dot_frame,
        );
        view.setWantsLayer(true);
        unsafe {
            let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&view, layer];
            let amber = NSColor::colorWithSRGBRed_green_blue_alpha(
                245.0 / 255.0,
                158.0 / 255.0,
                11.0 / 255.0,
                1.0,
            );
            let cg_color: *const std::ffi::c_void = msg_send![&amber, CGColor];
            let _: () = msg_send![&layer, setBackgroundColor: cg_color];
            let _: () = msg_send![&layer, setCornerRadius: stormforge::DOT_DIAMETER / 2.0];
        }
        view
    };

    // --- Labels ---
    let text_width = sp.width - sp.text_left_inset - sp.text_right_pad;

    let listening_label = {
        let label = make_label(
            mtm,
            sp.text_left_inset,
            sp.vertical_pad,
            text_width,
            sp.min_height - 2.0 * sp.vertical_pad,
        );
        label.setStringValue(&NSString::from_str("Listening..."));
        // SF Pro Semibold for listening label (grey, italic if possible)
        let weight = unsafe { NSFontWeightSemibold };
        let font = NSFont::systemFontOfSize_weight(stormforge::LISTENING_FONT_SIZE, weight);
        // Try to get italic variant via font descriptor
        unsafe {
            let descriptor: Retained<objc2_app_kit::NSFontDescriptor> =
                msg_send![&font, fontDescriptor];
            let italic_trait: u32 = 1; // NSFontItalicTrait
            let italic_desc: Retained<objc2_app_kit::NSFontDescriptor> =
                msg_send![&descriptor, fontDescriptorWithSymbolicTraits: italic_trait];
            let maybe_font: Option<Retained<NSFont>> = msg_send![
                objc2::class!(NSFont),
                fontWithDescriptor: &*italic_desc,
                size: stormforge::LISTENING_FONT_SIZE
            ];
            if let Some(italic_font) = maybe_font {
                label.setFont(Some(&italic_font));
            } else {
                label.setFont(Some(&font));
            }
        }
        // Grey #9CA3AF
        let grey = NSColor::colorWithSRGBRed_green_blue_alpha(
            156.0 / 255.0,
            163.0 / 255.0,
            175.0 / 255.0,
            1.0,
        );
        label.setTextColor(Some(&grey));
        label.setAlignment(NSTextAlignment::Left);
        label
    };

    let text_label = {
        let label = make_label(
            mtm,
            sp.text_left_inset,
            sp.vertical_pad,
            text_width,
            sp.min_height - 2.0 * sp.vertical_pad,
        );
        label.setStringValue(&NSString::from_str(""));
        // SF Pro Semibold 15pt
        let weight = unsafe { NSFontWeightSemibold };
        let font = NSFont::systemFontOfSize_weight(stormforge::FONT_SIZE, weight);
        label.setFont(Some(&font));
        // Text color #F8FAFC
        let text_color = NSColor::colorWithSRGBRed_green_blue_alpha(
            248.0 / 255.0,
            250.0 / 255.0,
            252.0 / 255.0,
            1.0,
        );
        label.setTextColor(Some(&text_color));
        enable_wrapping(&label);
        label
    };

    // Assemble
    effect_view.addSubview(&tint_view);
    effect_view.addSubview(&dot_view);
    effect_view.addSubview(&listening_label);
    effect_view.addSubview(&text_label);
    panel.setContentView(Some(&effect_view));

    panel.orderOut(None);
    app.finishLaunching();

    let mut pulse_bright = true;
    let mut pulse_last_toggle = Instant::now();
    let mut is_pulsing = false;
    let mut has_transcript = false;

    let mut fade_in_remaining: usize = 0;
    let mut fade_out_remaining: usize = 0;
    let mut fade_last_step = Instant::now();

    loop {
        drain_events(&app);

        // Dot pulse: alternate opacity between 0.6 and 1.0
        if is_pulsing && pulse_last_toggle.elapsed() >= stormforge::PULSE_HALF_CYCLE {
            pulse_bright = !pulse_bright;
            pulse_last_toggle = Instant::now();
            let opacity = if pulse_bright {
                stormforge::DOT_OPACITY_BRIGHT
            } else {
                stormforge::DOT_OPACITY_DIM
            };
            set_view_opacity(&dot_view, opacity);
        }

        // Fade-in
        if fade_in_remaining > 0
            && fade_last_step.elapsed() >= Duration::from_millis(stormforge::FADE_IN_STEP_MS)
        {
            fade_in_remaining -= 1;
            let progress =
                1.0 - (fade_in_remaining as f64 / stormforge::FADE_IN_STEPS as f64);
            unsafe {
                let _: () = msg_send![&panel, setAlphaValue: progress];
            }
            fade_last_step = Instant::now();
        }

        // Fade-out
        if fade_out_remaining > 0
            && fade_last_step.elapsed() >= Duration::from_millis(stormforge::FADE_OUT_STEP_MS)
        {
            fade_out_remaining -= 1;
            let progress =
                fade_out_remaining as f64 / stormforge::FADE_OUT_STEPS as f64;
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
                set_view_opacity(&dot_view, stormforge::DOT_OPACITY_BRIGHT);

                listening_label.setHidden(false);
                text_label.setStringValue(&NSString::from_str(""));
                text_label.setHidden(true);

                // Re-center dot vertically for min height
                let dot_y = (sp.min_height - stormforge::DOT_DIAMETER) / 2.0;
                dot_view.setFrame(NSRect::new(
                    NSPoint::new(stormforge::DOT_X, dot_y),
                    NSSize::new(stormforge::DOT_DIAMETER, stormforge::DOT_DIAMETER),
                ));

                resize_overlay_generic(
                    &panel,
                    &effect_view,
                    &[&tint_view],
                    &[&text_label, &listening_label],
                    &sp,
                    screen_frame,
                    sp.min_height,
                    None,
                );

                fade_out_remaining = 0;
                unsafe {
                    let _: () = msg_send![&panel, setAlphaValue: 0.0f64];
                }
                panel.orderFront(None);
                fade_in_remaining = stormforge::FADE_IN_STEPS;
                fade_last_step = Instant::now();
            }
            Ok(OverlayMsg::UpdateText(text)) => {
                if text.is_empty() {
                    if has_transcript {
                        has_transcript = false;
                        is_pulsing = true;
                        pulse_bright = true;
                        pulse_last_toggle = Instant::now();
                        set_view_opacity(&dot_view, stormforge::DOT_OPACITY_BRIGHT);
                        listening_label.setHidden(false);
                        text_label.setHidden(true);
                    }
                    text_label.setStringValue(&NSString::from_str(""));
                    resize_overlay_generic(
                        &panel,
                        &effect_view,
                        &[&tint_view],
                        &[&text_label, &listening_label],
                        &sp,
                        screen_frame,
                        sp.min_height,
                        None,
                    );
                    // Re-center dot
                    let dot_y = (sp.min_height - stormforge::DOT_DIAMETER) / 2.0;
                    dot_view.setFrame(NSRect::new(
                        NSPoint::new(stormforge::DOT_X, dot_y),
                        NSSize::new(stormforge::DOT_DIAMETER, stormforge::DOT_DIAMETER),
                    ));
                } else {
                    if !has_transcript {
                        has_transcript = true;
                        is_pulsing = false;
                        set_view_opacity(&dot_view, stormforge::DOT_OPACITY_BRIGHT);
                        listening_label.setHidden(true);
                        text_label.setHidden(false);
                    }
                    text_label.setStringValue(&NSString::from_str(&text));
                    let new_height = compute_needed_height(&text_label, &sp);
                    resize_overlay_generic(
                        &panel,
                        &effect_view,
                        &[&tint_view],
                        &[&text_label, &listening_label],
                        &sp,
                        screen_frame,
                        new_height,
                        None,
                    );
                    // Re-center dot at new height
                    let dot_y = (new_height - stormforge::DOT_DIAMETER) / 2.0;
                    dot_view.setFrame(NSRect::new(
                        NSPoint::new(stormforge::DOT_X, dot_y),
                        NSSize::new(stormforge::DOT_DIAMETER, stormforge::DOT_DIAMETER),
                    ));
                }
            }
            Ok(OverlayMsg::Hide) => {
                is_pulsing = false;
                fade_in_remaining = 0;
                fade_out_remaining = stormforge::FADE_OUT_STEPS;
                fade_last_step = Instant::now();
            }
            Ok(OverlayMsg::Quit) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

// =============================================================================
// Uru Terminal
// =============================================================================

fn run_uru(
    mtm: MainThreadMarker,
    app: Retained<NSApplication>,
    panel: Retained<NSPanel>,
    screen_frame: NSRect,
    sp: StyleParams,
    receiver: mpsc::Receiver<OverlayMsg>,
) {
    // --- Plain black background (no vibrancy) ---
    let bg_view = {
        let local_frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(sp.width, sp.min_height),
        );
        let view = objc2_app_kit::NSView::initWithFrame(
            objc2_app_kit::NSView::alloc(mtm),
            local_frame,
        );
        view.setWantsLayer(true);
        unsafe {
            let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&view, layer];
            // #000000 at 88% opacity
            let bg_color =
                NSColor::colorWithSRGBRed_green_blue_alpha(0.0, 0.0, 0.0, 0.88);
            let cg_color: *const std::ffi::c_void = msg_send![&bg_color, CGColor];
            let _: () = msg_send![&layer, setBackgroundColor: cg_color];
            let _: () = msg_send![&layer, setCornerRadius: sp.corner_radius];
            let _: () = msg_send![&layer, setMasksToBounds: true];
            // Shadow: minimal
            let _: () = msg_send![&layer, setShadowOpacity: 0.3f32];
            let _: () = msg_send![&layer, setShadowRadius: 8.0f64];
            let shadow_offset = NSSize::new(0.0, -2.0);
            let _: () = msg_send![&layer, setShadowOffset: shadow_offset];
            let black = NSColor::colorWithSRGBRed_green_blue_alpha(0.0, 0.0, 0.0, 1.0);
            let cg_shadow: *const std::ffi::c_void = msg_send![&black, CGColor];
            let _: () = msg_send![&layer, setShadowColor: cg_shadow];
            let _: () = msg_send![&layer, setMasksToBounds: false];
        }

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

        view
    };

    // --- Cursor label (blinking `_`) ---
    let cursor_label = {
        let cursor_height = sp.min_height - 2.0 * sp.vertical_pad;
        let label = NSTextField::initWithFrame(
            NSTextField::alloc(mtm),
            NSRect::new(
                NSPoint::new(uru::CURSOR_X, sp.vertical_pad),
                NSSize::new(20.0, cursor_height),
            ),
        );
        label.setEditable(false);
        label.setBezeled(false);
        label.setDrawsBackground(false);
        label.setSelectable(false);
        label.setStringValue(&NSString::from_str("_"));
        // SF Mono 16pt bold
        {
            let bold_weight: f64 = 0.4; // NSFontWeightBold
            let font = NSFont::monospacedSystemFontOfSize_weight(uru::CURSOR_FONT_SIZE, bold_weight);
            label.setFont(Some(&font));
        }
        let white = NSColor::colorWithSRGBRed_green_blue_alpha(1.0, 1.0, 1.0, 1.0);
        label.setTextColor(Some(&white));
        label.setAlignment(NSTextAlignment::Left);
        label
    };

    // --- Transcript text label ---
    let text_width = sp.width - sp.text_left_inset - sp.text_right_pad;
    let text_label = {
        let label = make_label(
            mtm,
            sp.text_left_inset,
            sp.vertical_pad,
            text_width,
            sp.min_height - 2.0 * sp.vertical_pad,
        );
        label.setStringValue(&NSString::from_str(""));
        // SF Mono 14pt medium
        let weight = unsafe { NSFontWeightMedium };
        let font = NSFont::monospacedSystemFontOfSize_weight(uru::FONT_SIZE, weight);
        label.setFont(Some(&font));
        // Pure white
        let white = NSColor::colorWithSRGBRed_green_blue_alpha(1.0, 1.0, 1.0, 1.0);
        label.setTextColor(Some(&white));
        enable_wrapping(&label);
        label
    };

    // Assemble
    bg_view.addSubview(&cursor_label);
    bg_view.addSubview(&text_label);
    panel.setContentView(Some(&bg_view));

    panel.orderOut(None);
    app.finishLaunching();

    let mut blink_on = true;
    let mut blink_last_toggle = Instant::now();
    let mut is_blinking = false;
    let mut has_transcript = false;

    loop {
        drain_events(&app);

        // Cursor blink
        if is_blinking
            && blink_last_toggle.elapsed()
                >= Duration::from_millis(uru::BLINK_HALF_CYCLE_MS)
        {
            blink_on = !blink_on;
            blink_last_toggle = Instant::now();
            let ch = if blink_on { "_" } else { " " };
            cursor_label.setStringValue(&NSString::from_str(ch));
        }

        match receiver.recv_timeout(Duration::from_millis(16)) {
            Ok(OverlayMsg::Show) => {
                has_transcript = false;
                is_blinking = true;
                blink_on = true;
                blink_last_toggle = Instant::now();
                cursor_label.setStringValue(&NSString::from_str("_"));
                cursor_label.setHidden(false);

                text_label.setStringValue(&NSString::from_str(""));
                text_label.setHidden(true);

                resize_uru(
                    &panel,
                    &bg_view,
                    &text_label,
                    &cursor_label,
                    &sp,
                    screen_frame,
                    sp.min_height,
                );

                // Instant show (no fade)
                unsafe {
                    let _: () = msg_send![&panel, setAlphaValue: 1.0f64];
                }
                panel.orderFront(None);
            }
            Ok(OverlayMsg::UpdateText(text)) => {
                if text.is_empty() {
                    if has_transcript {
                        has_transcript = false;
                        is_blinking = true;
                        blink_on = true;
                        blink_last_toggle = Instant::now();
                        cursor_label.setStringValue(&NSString::from_str("_"));
                        cursor_label.setHidden(false);
                        text_label.setHidden(true);
                    }
                    text_label.setStringValue(&NSString::from_str(""));
                    resize_uru(
                        &panel,
                        &bg_view,
                        &text_label,
                        &cursor_label,
                        &sp,
                        screen_frame,
                        sp.min_height,
                    );
                } else {
                    if !has_transcript {
                        has_transcript = true;
                        is_blinking = false;
                        // Stop blink, show `>`
                        cursor_label.setStringValue(&NSString::from_str(">"));
                        text_label.setHidden(false);
                    }
                    text_label.setStringValue(&NSString::from_str(&text));
                    let new_height = compute_needed_height(&text_label, &sp);
                    resize_uru(
                        &panel,
                        &bg_view,
                        &text_label,
                        &cursor_label,
                        &sp,
                        screen_frame,
                        new_height,
                    );
                }
            }
            Ok(OverlayMsg::Hide) => {
                is_blinking = false;
                // Instant hide (no fade)
                panel.orderOut(None);
            }
            Ok(OverlayMsg::Quit) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Resize helper for Uru (no vibrancy view, no stripe).
fn resize_uru(
    panel: &NSPanel,
    bg_view: &objc2_app_kit::NSView,
    text_label: &NSTextField,
    cursor_label: &NSTextField,
    sp: &StyleParams,
    screen_frame: NSRect,
    new_height: f64,
) {
    let new_height = clamp_height(new_height, sp.min_height);

    let x = (screen_frame.size.width - sp.width) / 2.0;
    let y = BOTTOM_MARGIN;
    let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(sp.width, new_height));
    panel.setFrame_display(frame, true);

    let content_frame = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(sp.width, new_height),
    );
    bg_view.setFrame(content_frame);
    unsafe {
        let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&*bg_view, layer];
        let _: () = msg_send![&layer, setCornerRadius: sp.corner_radius];
    }

    let text_width = sp.width - sp.text_left_inset - sp.text_right_pad;
    let text_height = (new_height - 2.0 * sp.vertical_pad).max(1.0);
    let text_frame = NSRect::new(
        NSPoint::new(sp.text_left_inset, sp.vertical_pad),
        NSSize::new(text_width, text_height),
    );
    text_label.setFrame(text_frame);

    // Re-position cursor vertically
    let cursor_frame = NSRect::new(
        NSPoint::new(uru::CURSOR_X, sp.vertical_pad),
        NSSize::new(20.0, text_height),
    );
    cursor_label.setFrame(cursor_frame);
}

// =============================================================================
// Shared helpers
// =============================================================================

/// Create an NSVisualEffectView with HUD material, dark appearance, corner radius.
fn make_vibrancy_view(
    mtm: MainThreadMarker,
    width: f64,
    height: f64,
    corner_radius: f64,
) -> Retained<NSVisualEffectView> {
    let local_frame = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(width, height),
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

    // Corner radius
    unsafe {
        let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&view, layer];
        let _: () = msg_send![&layer, setCornerRadius: corner_radius];
        let _: () = msg_send![&layer, setMasksToBounds: false];
    }

    view
}

/// Create a tint overlay view with given RGB (0-255 scale) and alpha.
fn make_tint_view(
    mtm: MainThreadMarker,
    width: f64,
    height: f64,
    corner_radius: f64,
    r: f64,
    g: f64,
    b: f64,
    alpha: f64,
) -> Retained<objc2_app_kit::NSView> {
    let local_frame = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(width, height),
    );
    let view = objc2_app_kit::NSView::initWithFrame(
        objc2_app_kit::NSView::alloc(mtm),
        local_frame,
    );
    view.setWantsLayer(true);
    unsafe {
        let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&view, layer];
        let tint_color = NSColor::colorWithSRGBRed_green_blue_alpha(
            r / 255.0,
            g / 255.0,
            b / 255.0,
            alpha,
        );
        let cg_color: *const std::ffi::c_void = msg_send![&tint_color, CGColor];
        let _: () = msg_send![&layer, setBackgroundColor: cg_color];
        let _: () = msg_send![&layer, setCornerRadius: corner_radius];
        let _: () = msg_send![&layer, setMasksToBounds: true];
    }
    view
}

/// Create a basic non-editable text label.
fn make_label(
    mtm: MainThreadMarker,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Retained<NSTextField> {
    let label = NSTextField::initWithFrame(
        NSTextField::alloc(mtm),
        NSRect::new(NSPoint::new(x, y), NSSize::new(width, height)),
    );
    label.setEditable(false);
    label.setBezeled(false);
    label.setDrawsBackground(false);
    label.setSelectable(false);
    label.setAlignment(NSTextAlignment::Left);
    label
}

/// Enable word wrapping on a label.
fn enable_wrapping(label: &NSTextField) {
    label.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    unsafe {
        let cell: Option<Retained<objc2_app_kit::NSCell>> = msg_send![&*label, cell];
        if let Some(cell) = cell {
            let _: () = msg_send![&cell, setWraps: true];
        }
    }
    label.setMaximumNumberOfLines(0);
}

fn set_view_opacity(view: &objc2_app_kit::NSView, opacity: f32) {
    unsafe {
        let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&*view, layer];
        let _: () = msg_send![&layer, setOpacity: opacity];
    }
}

fn clamp_height(h: f64, min: f64) -> f64 {
    if h.is_finite() && h >= min {
        if h <= OVERLAY_MAX_HEIGHT {
            h
        } else {
            OVERLAY_MAX_HEIGHT
        }
    } else {
        min
    }
}

/// Estimate text height using a character-width heuristic.
fn compute_needed_height(label: &NSTextField, sp: &StyleParams) -> f64 {
    let text_width = sp.width - sp.text_left_inset - sp.text_right_pad;
    if text_width <= 0.0 || !text_width.is_finite() {
        return sp.min_height;
    }

    let nsstr: Retained<NSString> = label.stringValue();
    let text = nsstr.to_string();

    if text.is_empty() {
        return sp.min_height;
    }

    let chars_per_line = (text_width / sp.avg_char_width).max(1.0) as usize;

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

    let h = line_count as f64 * sp.line_height + 2.0 * sp.vertical_pad;

    if h.is_finite() && h > sp.min_height {
        if h < OVERLAY_MAX_HEIGHT {
            h
        } else {
            OVERLAY_MAX_HEIGHT
        }
    } else {
        sp.min_height
    }
}

/// Generic resize for styles that use an effect_view (Bifrost, Stormforge).
/// `full_size_views` are views that span the full content rect.
/// `text_labels` are labels positioned at text_left_inset.
/// `stripe` is an optional (view, width) for a left-edge stripe.
fn resize_overlay_generic(
    panel: &NSPanel,
    effect_view: &NSVisualEffectView,
    full_size_views: &[&objc2_app_kit::NSView],
    text_labels: &[&NSTextField],
    sp: &StyleParams,
    screen_frame: NSRect,
    new_height: f64,
    stripe: Option<(&objc2_app_kit::NSView, f64)>,
) {
    let new_height = clamp_height(new_height, sp.min_height);

    let x = (screen_frame.size.width - sp.width) / 2.0;
    let y = BOTTOM_MARGIN;
    let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(sp.width, new_height));
    panel.setFrame_display(frame, true);

    let content_frame = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(sp.width, new_height),
    );
    effect_view.setFrame(content_frame);
    unsafe {
        let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&**effect_view, layer];
        let _: () = msg_send![&layer, setCornerRadius: sp.corner_radius];
    }

    for view in full_size_views {
        view.setFrame(content_frame);
        unsafe {
            let layer: Retained<objc2_quartz_core::CALayer> = msg_send![&***view, layer];
            let _: () = msg_send![&layer, setCornerRadius: sp.corner_radius];
            let _: () = msg_send![&layer, setMasksToBounds: true];
        }
    }

    if let Some((stripe_view, stripe_width)) = stripe {
        let stripe_frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(stripe_width, new_height),
        );
        stripe_view.setFrame(stripe_frame);
        unsafe {
            let layer: Retained<objc2_quartz_core::CALayer> =
                msg_send![&*stripe_view, layer];
            let _: () = msg_send![&layer, setCornerRadius: sp.corner_radius];
            let corner_mask: usize = 1 | 4;
            let _: () = msg_send![&layer, setMaskedCorners: corner_mask];
        }
    }

    let text_width = sp.width - sp.text_left_inset - sp.text_right_pad;
    let text_height = (new_height - 2.0 * sp.vertical_pad).max(1.0);
    let text_frame = NSRect::new(
        NSPoint::new(sp.text_left_inset, sp.vertical_pad),
        NSSize::new(text_width, text_height),
    );
    for label in text_labels {
        label.setFrame(text_frame);
    }
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
