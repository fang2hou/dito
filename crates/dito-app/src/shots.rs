//! Dev-only: render every settings page (light and dark) to PNG, then quit.
//! Build with `--features screenshots`; run `dito --mock --log-sink --screenshots /tmp/shots`.

use std::path::PathBuf;
use std::time::Duration;

use dito_core::config::Appearance;
use gpui_kit::*;

use crate::settings::{self, Page};
use crate::state::AppGlobal;

pub fn run(dir: PathBuf, cx: &mut App) {
    std::fs::create_dir_all(&dir).expect("screenshot dir");
    let st = cx.global::<AppGlobal>().0.clone();
    settings::open(st.clone(), Page::General, cx);
    // `DITO_SHOTS_ONLY=hud,demo` renders just those groups (pages, extra, onboarding, hud, demo, min).
    let only = std::env::var("DITO_SHOTS_ONLY").ok();
    let opt_in = {
        let only = only.clone();
        move |group: &str| only.as_ref().is_some_and(|o| o.split(',').any(|g| g == group))
    };
    let want = move |group: &str| only.as_ref().is_none_or(|o| o.split(',').any(|g| g == group));
    cx.spawn(async move |cx| {
        let page_looks: &[Appearance] = if want("pages") { &[Appearance::Light, Appearance::Dark] } else { &[] };
        for &appearance in page_looks {
            let _ = st.cfg.update(|c| c.general.appearance = appearance);
            for page in Page::ALL {
                cx.update(|cx| settings::open(st.clone(), page, cx));
                cx.background_executor().timer(Duration::from_millis(900)).await;
                let name = format!(
                    "{}-{}.png",
                    page.english().to_lowercase(),
                    if appearance == Appearance::Dark { "dark" } else { "light" }
                );
                let path = dir.join(name);
                let res = cx.update(|cx| {
                    let (window, _) = settings::handle(cx)?;
                    window.update(cx, |_, window, _| window.render_to_image().map_err(|e| e.to_string())).ok()
                });
                match res {
                    Some(Ok(img)) => {
                        if let Err(e) = img.save(&path) {
                            eprintln!("save {}: {e}", path.display());
                        } else {
                            println!("wrote {}", path.display());
                        }
                    }
                    Some(Err(e)) => eprintln!("render {}: {e}", path.display()),
                    None => eprintln!("no window for {}", path.display()),
                }
            }
        }
        if opt_in("navrepro") {
            cx.background_executor().timer(Duration::from_millis(700)).await;
            for _ in 0..3 {
                for index in [0, 1, 1, 0, 0, 1, 1, 0, 1, 2, 2, 2, 1] {
                    cx.update(|cx| {
                        let (w, _) = settings::handle(cx).unwrap();
                        let _ = cx.update_window(w.into(), |_, w, cx| {
                            let position = point(px(100.), px(175. + index as f32 * 48.));
                            w.dispatch_event(
                                PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }),
                                cx,
                            );
                            w.refresh();
                            w.draw(cx).clear(cx);
                            w.dispatch_event(
                                PlatformInput::MouseDown(MouseDownEvent {
                                    position,
                                    button: MouseButton::Left,
                                    click_count: 1,
                                    ..Default::default()
                                }),
                                cx,
                            );
                            w.dispatch_event(
                                PlatformInput::MouseUp(MouseUpEvent {
                                    position,
                                    button: MouseButton::Left,
                                    click_count: 1,
                                    ..Default::default()
                                }),
                                cx,
                            );
                        });
                    });
                    cx.background_executor().timer(Duration::from_millis(130)).await;
                }
            }
            cx.background_executor().timer(Duration::from_millis(600)).await;
        }
        if opt_in("models") {
            model_shots(&st, &dir, cx).await;
        }
        if want("extra") {
            extra_shots(&st, &dir, cx).await;
        }
        if want("onboarding") {
            onboarding_shots(&st, &dir, cx).await;
        }
        if want("hud") {
            hud_shots(&st, &dir, cx).await;
        }
        if want("demo") {
            demo_shots(&dir, cx).await;
        }
        if opt_in("hudstylepreview") {
            hud_style_preview_shots(&st, &dir, cx).await;
        }
        if opt_in("hudoutcome") {
            hud_outcome_shots(&st, &dir, cx).await;
        }
        if opt_in("hudwave") {
            hud_wave_shots(&st, &dir, cx).await;
        }
        if opt_in("hudmotion") {
            hud_motion_shots(&st, &dir, cx).await;
        }
        if opt_in("settingshud") {
            hud_settings_shots(&st, &dir, cx).await;
        }
        if opt_in("hudheight") {
            hud_height_shots(&st, &dir, cx).await;
        }
        if opt_in("hudlive") {
            hud_live_shots(&st, &dir, cx).await;
        }
        if opt_in("demolive") {
            demo_live_shots(&dir, cx).await;
        }
        if opt_in("appearance") {
            appearance_hover_shots(&st, &dir, cx).await;
        }
        if opt_in("health") {
            health_shots(&st, &dir, cx).await;
        }
        if want("min") {
            min_size_shots(&st, &dir, cx).await;
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}

fn save(dir: &std::path::Path, name: &str, img: image::RgbaImage) {
    let path = dir.join(name);
    match img.save(&path) {
        Ok(()) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("save {}: {e}", path.display()),
    }
}

async fn onboarding_shots(st: &std::sync::Arc<crate::state::AppState>, dir: &std::path::Path, cx: &mut AsyncApp) {
    crate::onboarding_demo::freeze(Some(crate::onboarding_demo::Frozen::Still));
    for appearance in [Appearance::Light, Appearance::Dark] {
        let _ = st.cfg.update(|c| c.general.appearance = appearance);
        let tag = if appearance == Appearance::Dark { "dark" } else { "light" };
        let st2 = st.clone();
        let Some((window, view)) = cx.update(|cx| crate::onboarding::open_with_view(st2, cx)) else { continue };
        for n in 0..7 {
            let v = view.clone();
            cx.update(|cx| v.update(cx, |o, cx| o.show_step(n, cx)));
            cx.background_executor().timer(Duration::from_millis(500)).await;
            if let Some(Ok(img)) =
                cx.update(|cx| window.update(cx, |_, w, _| w.render_to_image().map_err(|e| e.to_string())).ok())
            {
                save(dir, &format!("onboarding-{n}-{tag}.png"), img);
            }
        }
        let v = view.clone();
        cx.update(|cx| v.update(cx, |o, cx| o.show_provider_form(cx)));
        cx.background_executor().timer(Duration::from_millis(500)).await;
        if let Some(Ok(img)) =
            cx.update(|cx| window.update(cx, |_, w, _| w.render_to_image().map_err(|e| e.to_string())).ok())
        {
            save(dir, &format!("onboarding-provider-{tag}.png"), img);
        }
        let v = view.clone();
        cx.update(|cx| v.update(cx, |o, cx| o.show_step(3, cx)));
        let before = st.cfg.get().general.shortcut.clone();
        st.hotkeys.capture.send_replace(crate::hotkeys::CaptureState::Waiting);
        cx.background_executor().timer(Duration::from_millis(500)).await;
        if let Some(Ok(img)) =
            cx.update(|cx| window.update(cx, |_, w, _| w.render_to_image().map_err(|e| e.to_string())).ok())
        {
            save(dir, &format!("onboarding-3-capturing-{tag}.png"), img);
        }
        let _ = st.cfg.update(|c| c.general.shortcut = "Control+Option+Space".into());
        st.hotkeys.capture.send_replace(crate::hotkeys::CaptureState::Idle);
        cx.background_executor().timer(Duration::from_millis(500)).await;
        if let Some(Ok(img)) =
            cx.update(|cx| window.update(cx, |_, w, _| w.render_to_image().map_err(|e| e.to_string())).ok())
        {
            save(dir, &format!("onboarding-3-custom-{tag}.png"), img);
        }
        let _ = st.cfg.update(|c| c.general.shortcut = before);
        let v = view.clone();
        cx.update(|cx| {
            v.update(cx, |o, cx| {
                o.set_mode(dito_core::config::RecordMode::Hold, cx);
                o.show_step(2, cx);
            })
        });
        cx.background_executor().timer(Duration::from_millis(500)).await;
        if let Some(Ok(img)) =
            cx.update(|cx| window.update(cx, |_, w, _| w.render_to_image().map_err(|e| e.to_string())).ok())
        {
            save(dir, &format!("onboarding-2-hold-{tag}.png"), img);
        }
        let v = view.clone();
        cx.update(|cx| v.update(cx, |o, cx| o.show_step(6, cx)));
        cx.background_executor().timer(Duration::from_millis(500)).await;
        if let Some(Ok(img)) =
            cx.update(|cx| window.update(cx, |_, w, _| w.render_to_image().map_err(|e| e.to_string())).ok())
        {
            save(dir, &format!("onboarding-6-hold-{tag}.png"), img);
        }
        let _ = cx.update(|cx| window.update(cx, |_, w, _| w.remove_window()));
    }
    crate::onboarding_demo::freeze(None);
}

/// Exercise the same action as the Settings control, including replacement and side-effect checks.
async fn hud_style_preview_shots(
    st: &std::sync::Arc<crate::state::AppState>,
    dir: &std::path::Path,
    cx: &mut AsyncApp,
) {
    use dito_core::controller::{Command, HudState};
    use dito_core::store::HistoryFilter;
    let history_before = st.store.list_history(&HistoryFilter::default()).unwrap().len();
    let _ = st.cfg.update(|c| c.general.hud_appearance = Appearance::System);
    let (window, settings_view) = cx.update(|cx| {
        settings::open(st.clone(), Page::General, cx);
        let (window, view) = settings::handle(cx).unwrap();
        window.update(cx, |_, w, _| w.resize(size(px(1120.), px(1120.)))).unwrap();
        assert!(view.read(cx).hud_preview(cx).is_none(), "opening Settings must not start a preview");
        (window, view)
    });
    for (appearance, tag) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
        let _ = st.cfg.update(|c| c.general.appearance = appearance);
        settings_view.update(cx, |settings, cx| settings.set_hud_appearance(appearance, cx));
        let preview = cx.update(|cx| settings_view.read(cx).hud_preview(cx).expect("selection starts inline preview"));
        let start = std::time::Instant::now();
        let mut timestamps = String::new();
        let mut frame = 0;
        while start.elapsed().as_secs_f32() < 5.1 {
            let deadline = Duration::from_secs_f32((frame + 1) as f32 / 30.);
            cx.background_executor()
                .timer(deadline.saturating_sub(start.elapsed()).max(Duration::from_millis(2)))
                .await;
            let img = cx
                .update(|cx| {
                    AnyWindowHandle::from(window)
                        .update(cx, |_, window, cx| {
                            window.refresh();
                            window.draw(cx).clear(cx);
                            window.render_to_image()
                        })
                        .unwrap()
                })
                .unwrap();
            timestamps.push_str(&format!("{frame}\t{}\n", start.elapsed().as_secs_f64()));
            if frame == 0 {
                save(dir, &format!("inline-settings-{tag}.png"), img.clone());
            }
            // Fixed screenshot viewport: include the HUD row and its neighboring icon switcher.
            let row = image::imageops::crop_imm(&img, 288, 868, 776, 65).to_image();
            save(dir, &format!("inline-preview-{tag}-{frame:03}.png"), row);
            frame += 1;
        }
        std::fs::write(dir.join(format!("inline-preview-{tag}-times.tsv")), timestamps).unwrap();
        cx.update(|cx| {
            assert!(!preview.read(cx).active(), "animation ends after one cycle");
            assert!(settings_view.read(cx).hud_preview(cx).is_none(), "expired preview is hidden");
        });
        assert_eq!(*st.dictation.hud().borrow(), HudState::Hidden);
        assert_eq!(st.store.list_history(&HistoryFilter::default()).unwrap().len(), history_before);
        assert_eq!(st.cfg.get().general.appearance, appearance);
        settings_view.update(cx, |settings, cx| settings.set_hud_appearance(appearance, cx));
        cx.update(|cx| assert!(settings_view.read(cx).hud_preview(cx).is_none(), "same selection does not replay"));
        println!("inline style preview verified: {tag}, animation ended, no dictation/history, appearance independent");
    }
    settings_view.update(cx, |settings, cx| settings.set_hud_appearance(Appearance::Dark, cx));
    let first = cx.update(|cx| settings_view.read(cx).hud_preview(cx).unwrap());
    settings_view.update(cx, |settings, cx| settings.set_hud_appearance(Appearance::System, cx));
    let replacement = cx.update(|cx| settings_view.read(cx).hud_preview(cx).unwrap());
    assert_ne!(first.entity_id(), replacement.entity_id(), "rapid selection replaces the inline view");
    st.dictation.send(Command::KeyDown);
    cx.background_executor().timer(Duration::from_millis(100)).await;
    cx.update(|cx| assert!(settings_view.read(cx).hud_preview(cx).is_none(), "real listening takes priority"));
    st.dictation.send(Command::Cancel);
    cx.background_executor().timer(Duration::from_millis(100)).await;
    println!("inline style preview verified: replacement and real-session priority");
}

/// Follow the real success clock from the processing fill through icon drawing and exit.
async fn hud_outcome_shots(st: &std::sync::Arc<crate::state::AppState>, dir: &std::path::Path, cx: &mut AsyncApp) {
    for (appearance, tag, bg) in
        [(Appearance::Light, "light", [255, 255, 255, 255]), (Appearance::Dark, "dark", [0x38, 0x35, 0x2F, 255])]
    {
        let _ = st.cfg.update(|c| c.general.hud_appearance = appearance);
        for (copy, name) in [(false, "check"), (true, "copy")] {
            let st2 = st.clone();
            let window = cx.update(|cx| {
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                            None,
                            size(
                                px((crate::hud::WIN_W + 2. * crate::hud::WIN_PAD) * crate::hud::HUD_SCALE),
                                px((crate::hud::WIN_H + 2. * crate::hud::WIN_PAD) * crate::hud::HUD_SCALE),
                            ),
                            cx,
                        ))),
                        titlebar: None,
                        window_background: WindowBackgroundAppearance::Transparent,
                        ..Default::default()
                    },
                    |_, cx| cx.new(|cx| crate::hud::HudView::outcome_preview(st2, copy, cx)),
                )
                .unwrap()
            });
            let start = std::time::Instant::now();
            let mut timestamps = String::new();
            for frame in 0..if copy { 66 } else { 54 } {
                let deadline = Duration::from_secs_f32((frame + 1) as f32 / 30.);
                cx.background_executor()
                    .timer(deadline.saturating_sub(start.elapsed()).max(Duration::from_millis(2)))
                    .await;
                let img = cx
                    .update(|cx| {
                        AnyWindowHandle::from(window)
                            .update(cx, |_, w, cx| {
                                w.refresh();
                                w.draw(cx).clear(cx);
                                w.render_to_image()
                            })
                            .unwrap()
                    })
                    .unwrap();
                let mut canvas = image::RgbaImage::from_pixel(img.width(), img.height(), image::Rgba(bg));
                image::imageops::overlay(&mut canvas, &img, 0, 0);
                timestamps.push_str(&format!("{frame}\t{}\n", start.elapsed().as_secs_f64()));
                save(dir, &format!("outcome-{tag}-{name}-{frame:03}.png"), canvas);
            }
            std::fs::write(dir.join(format!("outcome-{tag}-{name}-times.tsv")), timestamps).unwrap();
            let _ = cx.update(|cx| window.update(cx, |_, w, _| w.remove_window()));
        }
    }
}

/// Native waveform recordings against white/dark content, driven through the live normalization filter.
async fn hud_wave_shots(st: &std::sync::Arc<crate::state::AppState>, dir: &std::path::Path, cx: &mut AsyncApp) {
    for (appearance, tag, bg) in
        [(Appearance::Light, "light", [255, 255, 255, 255]), (Appearance::Dark, "dark", [0x38, 0x35, 0x2F, 255])]
    {
        let _ = st.cfg.update(|c| c.general.hud_appearance = appearance);
        for (gain, volume) in [(0.0006, "quiet"), (0.06, "normal")] {
            let st2 = st.clone();
            let window = cx.update(|cx| {
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                            None,
                            size(
                                px((crate::hud::WIN_W + 2. * crate::hud::WIN_PAD) * crate::hud::HUD_SCALE),
                                px((crate::hud::WIN_H + 2. * crate::hud::WIN_PAD) * crate::hud::HUD_SCALE),
                            ),
                            cx,
                        ))),
                        titlebar: None,
                        window_background: WindowBackgroundAppearance::Transparent,
                        ..Default::default()
                    },
                    |_, cx| cx.new(|cx| crate::hud::HudView::waveform_preview(st2, gain, cx)),
                )
                .expect("native waveform window")
            });
            let start = std::time::Instant::now();
            let mut frames = std::collections::HashSet::new();
            let mut timestamps = String::new();
            for frame in 0..120 {
                let deadline = Duration::from_secs_f32((frame + 1) as f32 / 30.);
                cx.background_executor()
                    .timer(deadline.saturating_sub(start.elapsed()).max(Duration::from_millis(2)))
                    .await;
                let img = cx
                    .update(|cx| {
                        AnyWindowHandle::from(window)
                            .update(cx, |_, w, cx| {
                                // Occluded native windows may not commit frames; explicitly draw
                                // from current live state before reading the offscreen image.
                                w.refresh();
                                w.draw(cx).clear(cx);
                                w.render_to_image()
                            })
                            .expect("wave window")
                    })
                    .expect("native waveform frame");
                let mut canvas = image::RgbaImage::from_pixel(img.width(), img.height(), image::Rgba(bg));
                image::imageops::overlay(&mut canvas, &img, 0, 0);
                {
                    let (w, h) = canvas.dimensions();
                    frames.insert(image::imageops::crop_imm(&canvas, w / 2 - 20, h - 65, 40, 40).to_image().into_raw());
                }
                timestamps.push_str(&format!("{frame}\t{}\n", start.elapsed().as_secs_f64()));
                save(dir, &format!("wave-{tag}-{volume}-{frame:03}.png"), canvas);
            }
            if !dito_platform::system::reduce_motion() {
                assert!(
                    frames.len() > 10,
                    "{tag}/{volume}: waveform did not follow input ({} distinct frames)",
                    frames.len()
                );
            }
            let _ = std::fs::write(dir.join(format!("wave-{tag}-{volume}-times.tsv")), timestamps);
            println!("native waveform verified: {tag}/{volume}, {} distinct speaking frames", frames.len());
            let _ = cx.update(|cx| window.update(cx, |_, w, _| w.remove_window()));
        }
    }
}

/// Capture the real, continuously ticking native renderer; no frozen timestamps or HTML animation.
async fn hud_motion_shots(st: &std::sync::Arc<crate::state::AppState>, dir: &std::path::Path, cx: &mut AsyncApp) {
    for (appearance, tag, bg) in
        [(Appearance::Light, "light", [0xF7, 0xF5, 0xF0, 255]), (Appearance::Dark, "dark", [0x38, 0x35, 0x2F, 255])]
    {
        let _ = st.cfg.update(|c| c.general.hud_appearance = appearance);
        let st2 = st.clone();
        let opened = cx.update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(
                            px((crate::hud::WIN_W + 2. * crate::hud::WIN_PAD) * crate::hud::HUD_SCALE),
                            px((crate::hud::WIN_H + 2. * crate::hud::WIN_PAD) * crate::hud::HUD_SCALE),
                        ),
                        cx,
                    ))),
                    titlebar: None,
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                |_, cx| cx.new(|cx| crate::hud::HudView::motion_preview(st2, cx)),
            )
            .ok()
        });
        let Some(window) = opened else { continue };
        let start = std::time::Instant::now();
        let mut timestamps = String::new();
        let mut held_labels = std::collections::HashSet::new();
        let mut crisp_brush = false;
        for frame in 0..120 {
            let deadline = Duration::from_secs_f32((frame + 1) as f32 / 30.);
            cx.background_executor()
                .timer(deadline.saturating_sub(start.elapsed()).max(Duration::from_millis(2)))
                .await;
            if let Some(Ok(img)) = cx.update(|cx| {
                AnyWindowHandle::from(window)
                    .update(cx, |_, w, cx| {
                        w.refresh();
                        w.draw(cx).clear(cx);
                        w.render_to_image().map_err(|e| e.to_string())
                    })
                    .ok()
            }) {
                let mut canvas = image::RgbaImage::from_pixel(img.width(), img.height(), image::Rgba(bg));
                image::imageops::overlay(&mut canvas, &img, 0, 0);
                if start.elapsed().as_secs_f32() >= 1.6 {
                    let (w, h) = canvas.dimensions();
                    let scaled = |v: f32| (v * crate::hud::HUD_SCALE).round() as u32;
                    let sample = image::imageops::crop_imm(
                        &canvas,
                        w / 2 - scaled(18.),
                        h - scaled(39.),
                        scaled(36.),
                        scaled(18.),
                    )
                    .to_image();
                    held_labels.insert(sample.into_raw());
                    // At this row the label cannot contribute: a crisp front must remain
                    // visibly stronger than its low-opacity halo in both themes.
                    crisp_brush |= (w / 2 - scaled(6.)..w / 2 + scaled(6.)).any(|x| {
                        let red = canvas.get_pixel(x, h - scaled(22.))[0];
                        if appearance == Appearance::Dark { red > 150 } else { red < 200 }
                    });
                }
                save(dir, &format!("hud-motion-{tag}-{frame:03}.png"), canvas);
                timestamps.push_str(&format!("{frame}\t{}\n", start.elapsed().as_secs_f64()));
            }
        }
        assert!(crisp_brush, "{tag}: progress front was lost in its halo");
        if !dito_platform::system::reduce_motion() {
            assert!(held_labels.len() > 10, "{tag}: text sweep did not animate after progress stopped");
        }
        println!("native motion verified: {tag}, crisp brush, {} distinct held-fill label frames", held_labels.len());
        let _ = std::fs::write(dir.join(format!("hud-motion-{tag}-times.tsv")), timestamps);
        let _ = cx.update(|cx| window.update(cx, |_, w, _| w.remove_window()));
    }
}

/// Exercise independent interface/HUD selections without touching the user's config (run with --mock --data-dir).
async fn hud_settings_shots(st: &std::sync::Arc<crate::state::AppState>, dir: &std::path::Path, cx: &mut AsyncApp) {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for hud in [Appearance::Light, Appearance::Dark] {
            let _ = st.cfg.update(|c| {
                c.general.appearance = appearance;
                c.general.hud_appearance = hud;
            });
            cx.update(|cx| {
                settings::open(st.clone(), Page::General, cx);
                if let Some((window, _)) = settings::handle(cx) {
                    let _ = window.update(cx, |_, w, _| w.resize(size(px(1120.), px(1120.))));
                }
            });
            let interface_tag = if appearance == Appearance::Dark { "dark" } else { "light" };
            let hud_tag = if hud == Appearance::Dark { "dark" } else { "light" };
            page_shot(cx, dir, &format!("settings-{interface_tag}-hud-{hud_tag}.png")).await;
        }
    }
}

/// Capture both position modes and themes against an isolated mock configuration.
async fn hud_height_shots(st: &std::sync::Arc<crate::state::AppState>, dir: &std::path::Path, cx: &mut AsyncApp) {
    use dito_core::config::HudPosition;
    for appearance in [Appearance::Light, Appearance::Dark] {
        for position in [HudPosition::Centered, HudPosition::Pointer] {
            let _ = st.cfg.update(|c| {
                c.general.appearance = appearance;
                c.general.hud_position = position;
                c.general.hud_height_level = Some(0);
            });
            cx.update(|cx| {
                settings::open(st.clone(), Page::General, cx);
                if let Some((window, _)) = settings::handle(cx) {
                    let _ = window.update(cx, |_, w, _| w.resize(size(px(1120.), px(1200.))));
                }
            });
            let theme = if appearance == Appearance::Dark { "dark" } else { "light" };
            let mode = if position == HudPosition::Pointer { "pointer" } else { "centered" };
            page_shot(cx, dir, &format!("hud-height-{theme}-{mode}.png")).await;
            if appearance == Appearance::Light && position == HudPosition::Centered {
                verify_hud_height_clicks(st, cx).await;
            }
        }
    }
}

/// The slider's fill must be invisible at the lowest level and end in a straight vertical edge at
/// every other one (the left end follows the track's own rounding). Measured from the rendered
/// window, cropped to the track, ignoring the knob's own columns.
async fn verify_hud_height_fill(st: &std::sync::Arc<crate::state::AppState>, cx: &mut AsyncApp) {
    use dito_core::config::Appearance;
    let original = st.cfg.get();
    for appearance in [Appearance::Light, Appearance::Dark] {
        let _ = st.cfg.update(|c| c.general.appearance = appearance);
        cx.background_executor().timer(Duration::from_millis(120)).await;
        let level = st.cfg.get().general.hud_height_level();
        let img = cx.update(|cx| {
            let (window, _view) = settings::handle(cx).expect("HUD height settings window");
            window.update(cx, |_, w, _cx| w.render_to_image().map_err(|e| e.to_string())).ok()
        });
        let Some(Ok(img)) = img else { continue };
        let indicator = cx.update(|cx| {
            let (_, view) = settings::handle(cx).expect("HUD height settings window");
            view.read(cx).hud_height_bounds(cx)
        });
        // The slider reports its indicator's bounds: inset by half the knob on each side, so
        // expand them back out to the full 220 × 28 track.
        let bounds = Bounds::new(
            point(indicator.origin.x - px(14.), indicator.origin.y),
            size(indicator.size.width + px(28.), indicator.size.height),
        );
        let (x0, y0) = (bounds.left().as_f32() as u32, bounds.top().as_f32() as u32);
        let (w, h) = (bounds.size.width.as_f32() as u32, bounds.size.height.as_f32() as u32);
        if img.width() < x0 + w || img.height() < y0 + h {
            continue;
        }
        let dark = appearance == Appearance::Dark;
        let fill = |p: image::Rgba<u8>| {
            if dark { p[0] > 190 && p[1] > 150 && p[2] < 190 } else { p[0] < 110 && p[1] < 110 && p[2] < 110 }
        };
        // The straight edge sits on the thumb's centre line: at the rows near the track's top and
        // bottom the thumb's circle has curved away, so the edge is in the open. It must reach the
        // centre line at both rows equally; at the lowest level no fill may exist anywhere right of
        // the thumb.
        let center = 14. + (w as f32 - 28.) * (level as f32 + 2.) / 14.;
        let knob_right = x0 as f32 + center + 14.;
        let rightmost =
            |row: u32, from: u32| (from..x0 + w - 1).rev().find(|&x| fill(*img.get_pixel(x, row))).unwrap_or(from);
        let top_row = y0 + 3;
        let bottom_row = y0 + h - 4;
        if level <= -2 {
            let from = knob_right as u32 + 3;
            for row in [y0 + h / 2, top_row, bottom_row] {
                assert_eq!(
                    rightmost(row, from),
                    from,
                    "{appearance:?} level {level}: fill visible at the lowest level"
                );
            }
        } else {
            // The straight edge hides under the thumb; what the eye sees is that the fill still
            // reaches the thumb's silhouette near the top and bottom rows. A rounded trailing edge
            // would leave a gray gap there (the bug this guards against).
            let silhouette_at_row = center - 9.;
            for row in [top_row, bottom_row] {
                let reach = rightmost(row, x0 + 2) as f32 - x0 as f32;
                assert!(
                    reach + 3. >= silhouette_at_row,
                    "{appearance:?} level {level}: fill falls short of the thumb (reach {reach}, need {silhouette_at_row})"
                );
            }
            // The left end keeps the track's rounding: near the top and bottom rows the fill starts
            // noticeably further right than on the track's centre row.
            let leftmost = |row: u32, to: u32| (x0 + 1..to).find(|&x| fill(*img.get_pixel(x, row))).unwrap_or(to);
            let clear_of_knob = (x0 as f32 + center - 15.) as u32;
            let inset_top = leftmost(top_row, clear_of_knob) as f32 - x0 as f32;
            let inset_mid = leftmost(y0 + h / 2, clear_of_knob) as f32 - x0 as f32;
            assert!(
                inset_top - inset_mid >= 3.,
                "{appearance:?} level {level}: left end not rounded (top inset {inset_top}, mid {inset_mid})"
            );
        }
    }
    let _ = st.cfg.update(|c| {
        c.general.appearance = original.general.appearance;
    });
}

async fn verify_hud_height_clicks(st: &std::sync::Arc<crate::state::AppState>, cx: &mut AsyncApp) {
    for expected in [-2, 12] {
        cx.update(|cx| {
            let (window, view) = settings::handle(cx).expect("HUD height settings window");
            cx.update_window(window.into(), |_, w, cx| {
                let bounds = view.read(cx).hud_height_bounds(cx);
                assert!(bounds.size.width > px(100.), "slider has laid out its hit region");
                let x = if expected == -2 { bounds.left() + px(1.) } else { bounds.right() - px(1.) };
                let position = point(x, bounds.center().y);
                w.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }), cx);
                w.refresh();
                w.draw(cx).clear(cx);
                w.dispatch_event(
                    PlatformInput::MouseDown(MouseDownEvent {
                        button: MouseButton::Left,
                        position,
                        click_count: 1,
                        ..Default::default()
                    }),
                    cx,
                );
                w.dispatch_event(
                    PlatformInput::MouseUp(MouseUpEvent {
                        button: MouseButton::Left,
                        position,
                        click_count: 1,
                        ..Default::default()
                    }),
                    cx,
                );
                w.refresh();
                w.draw(cx).clear(cx);
            })
            .expect("dispatch slider clicks");
        });
        cx.background_executor().timer(Duration::from_millis(100)).await;
        assert_eq!(st.cfg.get().general.hud_height_level(), expected, "track click selects the endpoint");
    }
    // Pixel-check the fill on both themes: empty at the lowest level, straight trailing edge otherwise.
    verify_hud_height_fill(st, cx).await;
    // Let the animated thumb reach its end before grabbing it.
    cx.background_executor().timer(Duration::from_millis(350)).await;
    let bounds = cx.update(|cx| {
        let (_, view) = settings::handle(cx).expect("HUD height settings window");
        view.read(cx).hud_height_bounds(cx)
    });
    let at = |fraction: f32| point(bounds.left() + bounds.size.width * fraction, bounds.center().y);
    cx.update(|cx| {
        let (window, _) = settings::handle(cx).unwrap();
        cx.update_window(window.into(), |_, w, cx| {
            let position = at(1.);
            w.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }), cx);
            w.refresh();
            w.draw(cx).clear(cx);
            w.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
        })
        .unwrap();
    });
    for fraction in [0.9, 0.65, 0.4, 0.15, 0.] {
        cx.update(|cx| {
            let (window, _) = settings::handle(cx).unwrap();
            cx.update_window(window.into(), |_, w, cx| {
                w.dispatch_event(
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position: at(fraction),
                        pressed_button: Some(MouseButton::Left),
                        ..Default::default()
                    }),
                    cx,
                );
                w.refresh();
                w.draw(cx).clear(cx);
            })
            .unwrap();
        });
        cx.background_executor().timer(Duration::from_millis(20)).await;
    }
    cx.update(|cx| {
        let (window, _) = settings::handle(cx).unwrap();
        cx.update_window(window.into(), |_, w, cx| {
            w.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: at(0.),
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
        })
        .unwrap();
    });
    cx.background_executor().timer(Duration::from_millis(100)).await;
    assert_eq!(st.cfg.get().general.hud_height_level(), -2, "dragging the thumb reaches the minimum");
    cx.update(|cx| {
        let (window, _) = settings::handle(cx).unwrap();
        cx.update_window(window.into(), |_, w, cx| {
            w.dispatch_event(
                PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: Keystroke::parse("right").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
        })
        .unwrap();
    });
    cx.background_executor().timer(Duration::from_millis(100)).await;
    assert_eq!(st.cfg.get().general.hud_height_level(), -1, "arrow keys move one level");
    let _ = st.cfg.update(|c| c.general.hud_position = dito_core::config::HudPosition::Pointer);
    cx.background_executor().timer(Duration::from_millis(100)).await;
    assert_eq!(st.cfg.get().general.hud_height_level(), -1, "pointer mode preserves the height");
    let _ = st.cfg.update(|c| c.general.hud_position = dito_core::config::HudPosition::Centered);
    cx.background_executor().timer(Duration::from_millis(100)).await;
    assert_eq!(st.cfg.get().general.hud_height_level(), -1, "returning restores the previous height");
    let _ = st.cfg.update(|c| c.general.hud_height_level = Some(0));
    println!("verified HUD height slider clicks, dragging, keyboard and mode persistence");
}

/// Every page at the settings window's minimum width (900): catches narrow-layout overflow that
/// the default 1120×860 shots cannot see. Height goes past the 640 minimum so below-fold cards
/// (e.g. the models storage card) are captured too.
async fn min_size_shots(st: &std::sync::Arc<crate::state::AppState>, dir: &std::path::Path, cx: &mut AsyncApp) {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let _ = st.cfg.update(|c| c.general.appearance = appearance);
        let tag = if appearance == Appearance::Dark { "dark" } else { "light" };
        for page in Page::ALL {
            let st2 = st.clone();
            cx.update(|cx| settings::open(st2, page, cx));
            cx.update(|cx| {
                if let Some((window, _)) = settings::handle(cx) {
                    let _ = window.update(cx, |_, w, _| w.resize(size(px(900.), px(820.))));
                }
            });
            page_shot(cx, dir, &format!("{}-min-{tag}.png", page.english().to_lowercase())).await;
        }
    }
}

async fn hud_shots(st: &std::sync::Arc<crate::state::AppState>, dir: &std::path::Path, cx: &mut AsyncApp) {
    use dito_core::config::RecordMode;
    use dito_core::controller::{ErrorAction, HudState};
    use dito_core::error::ErrorKind;
    let toggle = HudState::Listening { mode: RecordMode::Toggle };
    let err = |kind, action| HudState::Error { kind, detail: String::new(), action };
    // (name, state, seconds into it, hovering the zone, hovered button, the state before was hold-mode listening)
    let cases: Vec<(&str, HudState, f32, bool, u8, bool)> = vec![
        ("listening", toggle.clone(), 1.0, false, 0, false),
        ("countdown", toggle.clone(), 240.2, false, 0, false),
        ("countdown-warning", toggle.clone(), 290.0, false, 0, false),
        ("listening-hover", toggle.clone(), 1.0, true, 0, false),
        ("listening-lang", toggle.clone(), 1.0, true, 0, false),
        ("hover-finish", toggle.clone(), 1.0, true, 2, false),
        ("hover-cancel", toggle.clone(), 1.0, true, 1, false),
        ("hold", HudState::Listening { mode: RecordMode::Hold }, 1.0, false, 0, false),
        ("hold-countdown", HudState::Listening { mode: RecordMode::Hold }, 240.2, false, 0, false),
        ("hold-release-0", HudState::Recognizing, 0.0, false, 0, true),
        ("hold-release-mid", HudState::Recognizing, 0.07, false, 0, true),
        ("hold-release-end", HudState::Recognizing, 0.25, false, 0, true),
        ("recognizing", HudState::Recognizing, 1.48, false, 0, false),
        ("recognizing-hover", HudState::Recognizing, 1.48, true, 0, false),
        ("polishing", HudState::Polishing, 1.03, false, 0, false),
        ("polishing-late", HudState::Polishing, 5.0, false, 0, false),
        ("recognizing-early", HudState::Recognizing, 0.15, false, 0, false),
        ("inserted", HudState::Inserted, 0.0, false, 0, false),
        ("copied", HudState::Copied, 0.0, false, 0, false),
        ("error-auth", err(ErrorKind::Auth, ErrorAction::Retry), 0.0, false, 0, false),
        ("error-mic", err(ErrorKind::Mic, ErrorAction::OpenSettings), 0.0, false, 0, false),
        ("error-retry-hover", err(ErrorKind::NoSpeech, ErrorAction::Retry), 0.0, true, 2, false),
        ("error-settings-hover", err(ErrorKind::Auth, ErrorAction::OpenSettings), 0.0, true, 2, false),
        ("error-close-hover", err(ErrorKind::Mic, ErrorAction::OpenSettings), 0.0, true, 1, false),
        ("copy-flow", HudState::Copied, 0.47, false, 0, false),
        ("copy-overshoot", HudState::Copied, 0.56, false, 0, false),
        ("copy-settled", HudState::Copied, 0.90, false, 0, false),
    ];
    for (name, state, t, hover, btn, from_hold) in cases {
        for dark in [true, false] {
            let _ =
                st.cfg.update(|c| c.general.hud_appearance = if dark { Appearance::Dark } else { Appearance::Light });
            let st2 = st.clone();
            let state2 = state.clone();
            let opened = cx.update(|cx| {
                let opts = WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(
                            px((crate::hud::WIN_W + 2. * crate::hud::WIN_PAD) * crate::hud::HUD_SCALE),
                            px((crate::hud::WIN_H + 2. * crate::hud::WIN_PAD) * crate::hud::HUD_SCALE),
                        ),
                        cx,
                    ))),
                    titlebar: None,
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                };
                cx.open_window(opts, |_, cx| {
                    cx.new(|_| crate::hud::HudView::preview(st2, state2, t, hover, btn, from_hold))
                })
                .ok()
            });
            let Some(window) = opened else { continue };
            cx.background_executor().timer(Duration::from_millis(350)).await;
            if let Some(Ok(img)) =
                cx.update(|cx| window.update(cx, |_, w, _| w.render_to_image().map_err(|e| e.to_string())).ok())
            {
                let bg = if dark { image::Rgba([0x38, 0x35, 0x2F, 255]) } else { image::Rgba([0xF7, 0xF5, 0xF0, 255]) };
                let mut canvas = image::RgbaImage::from_pixel(img.width(), img.height(), bg);
                image::imageops::overlay(&mut canvas, &img, 0, 0);
                save(dir, &format!("hud-{name}-{}.png", if dark { "dark" } else { "light" }), canvas);
            }
            let _ = cx.update(|cx| window.update(cx, |_, w, _| w.remove_window()));
        }
    }
}

/// Width in design points of the capsule in a HUD window shot (the opaque run of the row through its middle).
fn capsule_width_pt(img: &image::RgbaImage) -> f32 {
    use crate::hud::{HUD_SCALE, WIN_PAD};
    let y = (img.height() as f32 - (7. + WIN_PAD + 13.) * HUD_SCALE).round() as u32;
    let opaque: Vec<u32> = (0..img.width()).filter(|&x| img.get_pixel(x, y)[3] >= 250).collect();
    match (opaque.first(), opaque.last()) {
        (Some(a), Some(b)) => (b - a + 1) as f32 / HUD_SCALE,
        _ => 0.,
    }
}

/// The live HUD window (not a frozen preview) through a hold-mode dictation against the mock pipeline: the capsule
/// pops in narrow, stays narrow while the key is down, and widens when it is released. Prints the capsule's width
/// in design points per frame and saves some of the frames (`hud-live-hold-<n>.png`). Opt-in: `DITO_SHOTS_ONLY=hudlive`.
async fn hud_live_shots(st: &std::sync::Arc<crate::state::AppState>, dir: &std::path::Path, cx: &mut AsyncApp) {
    use dito_core::config::RecordMode;
    use dito_core::controller::Command;
    use std::time::Instant;
    let before = st.cfg.get().general.record_mode;
    let _ = st.cfg.update(|c| c.general.record_mode = RecordMode::Hold);
    // The real HUD opens its window once the state is no longer Hidden, so its pop-in starts on a listening capsule.
    st.dictation.send(Command::KeyDown);
    cx.background_executor().timer(Duration::from_millis(80)).await;
    let st2 = st.clone();
    let opened = cx.update(|cx| {
        let opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(
                    px((crate::hud::WIN_W + 2. * crate::hud::WIN_PAD) * crate::hud::HUD_SCALE),
                    px((crate::hud::WIN_H + 2. * crate::hud::WIN_PAD) * crate::hud::HUD_SCALE),
                ),
                cx,
            ))),
            titlebar: None,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };
        cx.open_window(opts, |_, cx| cx.new(|cx| crate::hud::HudView::new(st2, cx))).ok()
    });
    let Some(window) = opened else { return };
    let (opened_at, hold_for) = (Instant::now(), Duration::from_millis(1000));
    let mut released_at: Option<Instant> = None;
    let mut n = 0;
    while opened_at.elapsed() < hold_for + Duration::from_millis(900) {
        if released_at.is_none() && opened_at.elapsed() >= hold_for {
            st.dictation.send(Command::KeyUp);
            released_at = Some(Instant::now());
        }
        let shot = cx.update(|cx| window.update(cx, |_, w, _| w.render_to_image().map_err(|e| e.to_string())).ok());
        if let Some(Ok(img)) = shot {
            let since_open = opened_at.elapsed().as_millis();
            let since_release = released_at.map(|r| r.elapsed().as_millis() as i128).unwrap_or(-1);
            println!(
                "hud-live-hold frame {n:2}: +{since_open:4} ms open, {since_release:4} ms since release, capsule {:.1} pt",
                capsule_width_pt(&img)
            );
            if n % 3 == 0 {
                let bg = image::Rgba([0x38, 0x35, 0x2F, 255]);
                let mut canvas = image::RgbaImage::from_pixel(img.width(), img.height(), bg);
                image::imageops::overlay(&mut canvas, &img, 0, 0);
                save(dir, &format!("hud-live-hold-{n:02}.png"), canvas);
            }
            n += 1;
        }
        cx.background_executor().timer(Duration::from_millis(20)).await;
    }
    let _ = cx.update(|cx| window.update(cx, |_, w, _| w.remove_window()));
    let _ = st.cfg.update(|c| c.general.record_mode = before);
}

/// A bare window with the wizard's usage demo on a card-coloured ground.
struct DemoShot {
    p: crate::theme::Palette,
    lang: dito_core::i18n::Lang,
    mode: dito_core::config::RecordMode,
    key: SharedString,
}

impl Render for DemoShot {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        crate::ui::set_ui_language(self.lang);
        RENDERS.with(|r| r.set(r.get() + 1));
        div()
            .size_full()
            .bg(self.p.card)
            .p(px(24.))
            .font_family(crate::ui::cjk_family())
            .child(crate::onboarding_demo::usage_demo(&self.p, self.lang, self.mode, self.key.clone()))
    }
}

/// The usage demo on its own clock, not frozen: renders for a few seconds, switching the mode in between, and prints
/// what each frame shows (`demo-live-<n>.png` for some). Opt-in: `DITO_SHOTS_ONLY=demolive`.
async fn demo_live_shots(dir: &std::path::Path, cx: &mut AsyncApp) {
    use dito_core::config::RecordMode;
    use std::time::Instant;
    let lang = dito_core::i18n::Lang::Ja;
    let key = dito_platform::Shortcut::parse("RightOption").map(|s| dito_platform::keymap::label(&s, lang));
    let view = DemoShot {
        p: crate::theme::Palette::light(),
        lang,
        mode: RecordMode::Toggle,
        key: key.unwrap_or_default().into(),
    };
    let handle = cx.update(|cx| {
        let opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(624.), px(234.)), cx))),
            titlebar: None,
            ..Default::default()
        };
        cx.open_window(opts, |_, cx| cx.new(|_| view)).ok()
    });
    let Some(handle) = handle else { return };
    let start = Instant::now();
    let mut switched = false;
    let mut n = 0;
    while start.elapsed() < Duration::from_millis(7500) {
        if !switched && start.elapsed() >= Duration::from_millis(3000) {
            switched = true;
            let _ = cx.update(|cx| {
                handle.update(cx, |view, _, cx| {
                    view.mode = RecordMode::Hold;
                    cx.notify();
                })
            });
        }
        let (active, img) = cx.update(|cx| {
            handle
                .update(cx, |_, w, _| (w.is_window_active(), w.render_to_image().map_err(|e| e.to_string())))
                .ok()
                .unzip()
        });
        println!(
            "demo-live frame {n:2}: +{:4} ms, window active {active:?}, renders so far {}",
            start.elapsed().as_millis(),
            RENDERS.with(|r| r.get())
        );
        if let Some(Ok(img)) = img {
            save(dir, &format!("demo-live-{n:02}.png"), img);
        }
        n += 1;
        // Closer frames around the mode switch, where the cross-fade happens.
        let gap = if (3000..3400).contains(&start.elapsed().as_millis()) { 50 } else { 250 };
        cx.background_executor().timer(Duration::from_millis(gap)).await;
    }
    let _ = cx.update(|cx| handle.update(cx, |_, w, _| w.remove_window()));
}

thread_local! {
    /// How many times the demo window's view rendered (one per animation frame while the loop runs).
    static RENDERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The wizard's usage demo held at several moments of its loop, both modes, light and dark
/// (`demo-<mode>-<theme>-t<ms>.png`), the Reduce Motion still (`…-still.png`, the design's static frame), and the
/// still and the recognizing capsule in English and Chinese (`demo-<mode>-<theme>-<lang>-…`).
async fn demo_shots(dir: &std::path::Path, cx: &mut AsyncApp) {
    use crate::onboarding_demo::{Frozen, freeze};
    use dito_core::config::RecordMode;
    use dito_core::i18n::Lang;
    let toggle = [0.0, 0.66, 0.83, 1.4, 2.0, 3.04, 3.2, 4.0, 4.7, 4.88, 5.3, 5.7];
    let hold = [0.0, 0.66, 0.83, 1.4, 2.0, 3.04, 3.15, 3.2, 3.6, 4.2, 4.88, 5.3, 5.7];
    let key_label = |lang| {
        dito_platform::Shortcut::parse("RightOption")
            .map(|s| dito_platform::keymap::label(&s, lang))
            .unwrap_or_default()
    };
    let mut jobs = Vec::new();
    for (mode, name, times) in [(RecordMode::Toggle, "toggle", &toggle[..]), (RecordMode::Hold, "hold", &hold[..])] {
        for dark in [false, true] {
            jobs.push((mode, name, dark, Lang::Ja, Frozen::Still, "still".to_string()));
            jobs.extend(times.iter().map(|&at| {
                (mode, name, dark, Lang::Ja, Frozen::At(at), format!("t{:04}", (at * 1000.).round() as u32))
            }));
            for lang in [Lang::En, Lang::Zh] {
                jobs.push((mode, name, dark, lang, Frozen::Still, "still".to_string()));
                jobs.push((mode, name, dark, lang, Frozen::At(4.0), "t4000".to_string()));
            }
        }
    }
    let mut open: Option<((RecordMode, bool, Lang), AnyWindowHandle)> = None;
    for (mode, name, dark, lang, frozen, suffix) in jobs {
        let want = (mode, dark, lang);
        if open.as_ref().is_none_or(|(have, _)| *have != want) {
            if let Some((_, w)) = open.take() {
                let _ = cx.update(|cx| w.update(cx, |_, w, _| w.remove_window()));
            }
            let view = DemoShot {
                p: if dark { crate::theme::Palette::dark() } else { crate::theme::Palette::light() },
                lang,
                mode,
                key: key_label(lang).into(),
            };
            let handle = cx.update(|cx| {
                let opts = WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(624.), px(234.)), cx))),
                    titlebar: None,
                    ..Default::default()
                };
                cx.open_window(opts, |_, cx| cx.new(|_| view)).ok().map(AnyWindowHandle::from)
            });
            let Some(handle) = handle else { continue };
            open = Some((want, handle));
        }
        let Some((_, handle)) = &open else { continue };
        freeze(Some(frozen));
        let _ = cx.update(|cx| handle.update(cx, |_, w, _| w.refresh()));
        cx.background_executor().timer(Duration::from_millis(150)).await;
        if let Some(Ok(img)) =
            cx.update(|cx| handle.update(cx, |_, w, _| w.render_to_image().map_err(|e| e.to_string())).ok())
        {
            let theme = if dark { "dark" } else { "light" };
            let lang = if lang == Lang::Ja { String::new() } else { format!("-{}", lang.code()) };
            save(dir, &format!("demo-{name}-{theme}{lang}-{suffix}.png"), img);
        }
    }
    if let Some((_, w)) = open.take() {
        let _ = cx.update(|cx| w.update(cx, |_, w, _| w.remove_window()));
    }
    freeze(None);
}

async fn page_shot(cx: &mut AsyncApp, dir: &std::path::Path, name: &str) {
    cx.background_executor().timer(Duration::from_millis(700)).await;
    let img = cx.update(|cx| {
        let (window, _) = settings::handle(cx)?;
        window.update(cx, |_, w, _| w.render_to_image().map_err(|e| e.to_string())).ok()
    });
    if let Some(Ok(img)) = img {
        save(dir, name, img);
    }
}

/// Pages in non-default states: local models, shortcut recording.
async fn extra_shots(st: &std::sync::Arc<crate::state::AppState>, dir: &std::path::Path, cx: &mut AsyncApp) {
    use dito_core::config::Source;
    let original = st.cfg.get();
    for appearance in [Appearance::Light, Appearance::Dark] {
        let tag = if appearance == Appearance::Dark { "dark" } else { "light" };
        let _ = st.cfg.update(|c| {
            c.general.appearance = appearance;
            c.asr.source = Source::Local;
            c.polish.engine.source = Source::Local;
        });
        let st2 = st.clone();
        cx.update(|cx| settings::open(st2, Page::Models, cx));
        page_shot(cx, dir, &format!("models-local-{tag}.png")).await;
        let _ = st.cfg.update(|c| c.general.appearance = appearance);
        let st2 = st.clone();
        cx.update(|cx| settings::open(st2, Page::General, cx));
        st.hotkeys.capture.send_replace(crate::hotkeys::CaptureState::Waiting);
        page_shot(cx, dir, &format!("general-recording-{tag}.png")).await;
        st.hotkeys.capture.send_replace(crate::hotkeys::CaptureState::Idle);
        let st2 = st.clone();
        cx.update(|cx| settings::open(st2, Page::General, cx));
        cx.update(|cx| {
            if let Some((_, view)) = settings::handle(cx) {
                view.update(cx, |s, cx| s.open_general_select("ui-lang", cx));
            }
        });
        page_shot(cx, dir, &format!("general-select-open-{tag}.png")).await;
        cx.update(|cx| {
            if let Some((_, view)) = settings::handle(cx) {
                view.update(cx, |s, cx| s.open_general_select("mic", cx));
            }
        });
        page_shot(cx, dir, &format!("general-mic-open-{tag}.png")).await;
    }
    // Hover states: a nav item, a segment cell, a button, a select; then a provider card on Models.
    // force_hover writes the scope the id actually renders under: the shell (nav, health) or the
    // open page — no guessing a visit counter.
    let st2 = st.clone();
    let (shell, general) = cx.update(|cx| {
        settings::open(st2, Page::General, cx);
        (settings::shell_motion(cx), settings::page_motion(Page::General, cx))
    });
    let (shell, general) = (shell.expect("settings shell scope"), general.expect("general page scope"));
    let sub = |a: &str, b: &str| {
        gpui_kit::ElementId::from((
            gpui_kit::ElementId::from(a.to_string()),
            gpui_kit::SharedString::from(b.to_string()),
        ))
    };
    crate::ui::force_hover(&shell, gpui_kit::ElementId::from(("nav", 2usize)), true);
    crate::ui::force_hover(&general, sub("mode", "i1"), true);
    crate::ui::force_hover(&general, gpui_kit::ElementId::from("ui-lang".to_string()), true);
    page_shot(cx, dir, "hover-warmup.png").await;
    page_shot(cx, dir, "general-hover.png").await;
    crate::ui::force_hover(&shell, gpui_kit::ElementId::from(("nav", 2usize)), false);
    crate::ui::force_hover(&general, sub("mode", "i1"), false);
    crate::ui::force_hover(&general, gpui_kit::ElementId::from("ui-lang".to_string()), false);
    let st2 = st.clone();
    let models = cx
        .update(|cx| {
            settings::open(st2, Page::Models, cx);
            settings::page_motion(Page::Models, cx)
        })
        .expect("models page scope");
    crate::ui::force_hover(
        &models,
        gpui_kit::ElementId::from(gpui_kit::SharedString::from("prov-0-bedrock".to_string())),
        true,
    );
    crate::ui::force_hover(&models, gpui_kit::ElementId::from("test-0".to_string()), true);
    page_shot(cx, dir, "hover-warmup.png").await;
    page_shot(cx, dir, "models-hover.png").await;
    // Health dot hover: the halo + status card on the expanded logo, then on the rail's mark.
    // force_hover only writes the hover table — without a refresh the window would keep painting
    // the pre-hover element tree, so nudge a rebuild before each pair.
    crate::ui::force_hover(&shell, gpui_kit::ElementId::from("health-dot-hit".to_string()), true);
    cx.update(|cx| {
        if let Some((window, _)) = settings::handle(cx) {
            let _ = window.update(cx, |_, w, _| w.refresh());
        }
    });
    page_shot(cx, dir, "hover-warmup.png").await;
    page_shot(cx, dir, "health-hover.png").await;
    cx.update(|cx| {
        if let Some((window, _)) = settings::handle(cx) {
            let _ = window.update(cx, |_, w, _| w.resize(size(px(900.), px(820.))));
        }
    });
    page_shot(cx, dir, "hover-warmup.png").await;
    page_shot(cx, dir, "health-rail-hover.png").await;
    crate::ui::force_hover(&shell, gpui_kit::ElementId::from("health-dot-hit".to_string()), false);
    let _ = st.cfg.update(|c| {
        c.asr.source = original.asr.source;
        c.polish.engine.source = original.polish.engine.source;
    });
}

/// The logo-anchored health card: both sidebar widths, themes, languages and disabled polish.
async fn health_shots(st: &std::sync::Arc<crate::state::AppState>, dir: &std::path::Path, cx: &mut AsyncApp) {
    use dito_core::config::{DisplayLanguage, Source};
    let original = st.cfg.get();
    let hit = ElementId::from("health-dot-hit");
    // The health dot and card render under the settings shell scope; it survives navigation for
    // the window's whole life, so one handle serves every iteration.
    let shell = {
        let st2 = st.clone();
        cx.update(|cx| {
            settings::open(st2, Page::General, cx);
            settings::shell_motion(cx)
        })
        .expect("settings shell scope")
    };
    for (language, language_tag) in
        [(DisplayLanguage::Ja, "ja"), (DisplayLanguage::En, "en"), (DisplayLanguage::Zh, "zh")]
    {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let theme = if appearance == Appearance::Dark { "dark" } else { "light" };
            for (rail, enabled, source) in [
                (false, true, Source::Local),
                (true, false, Source::Local),
                (false, true, Source::Provider),
                (true, false, Source::Provider),
            ] {
                if source == Source::Provider && language != DisplayLanguage::Zh {
                    continue;
                }
                crate::ui::force_hover(&shell, hit.clone(), false);
                crate::ui::force_hover(&shell, ElementId::from("health-card-hit"), false);
                let _ = st.cfg.update(|c| {
                    c.general.appearance = appearance;
                    c.general.display_language = language;
                    c.asr.source = source;
                    c.polish.enabled = enabled;
                    c.polish.engine.source = source;
                });
                let st2 = st.clone();
                cx.update(|cx| {
                    settings::open(st2, Page::General, cx);
                    if let Some((window, _)) = settings::handle(cx) {
                        let _ =
                            window.update(cx, |_, w, _| w.resize(size(px(if rail { 900. } else { 1200. }), px(820.))));
                    }
                });
                // Settle the logo bounds and prime the closed animation before entering.
                cx.background_executor().timer(Duration::from_millis(400)).await;
                let dot = cx.update(|cx| {
                    let (_, view) = settings::handle(cx).expect("health settings window");
                    view.read(cx).health_hover_probe().0.expect("measured logo dot")
                });
                // Run the pointer lifecycle in one UI update, so native resize/mouse events
                // cannot replace the simulated pointer halfway through the test.
                health_verify_pointer(cx, dot.center());
                crate::ui::force_hover(&shell, hit.clone(), true);
                cx.update(|cx| {
                    if let Some((window, _)) = settings::handle(cx) {
                        let _ = window.update(cx, |_, w, _| w.refresh());
                    }
                });
                let mode = if rail { "rail-disabled" } else { "expanded" };
                let state = if source == Source::Provider { "configured" } else { "stopped" };
                page_shot(cx, dir, &format!("health-{language_tag}-{theme}-{mode}-{state}.png")).await;
                println!("verified health hover enter / card transfer / leave: {language_tag}-{theme}-{mode}-{state}");
            }
        }
    }
    crate::ui::force_hover(&shell, hit, false);
    crate::ui::force_hover(&shell, ElementId::from("health-card-hit"), false);
    let _ = st.cfg.update(|c| {
        c.general.appearance = original.general.appearance;
        c.general.display_language = original.general.display_language;
        c.asr.source = original.asr.source;
        c.polish.enabled = original.polish.enabled;
        c.polish.engine.source = original.polish.engine.source;
    });
}

fn health_verify_pointer(cx: &mut AsyncApp, dot: Point<Pixels>) {
    cx.update(|cx| {
        let (window, view) = settings::handle(cx).expect("health settings window");
        cx.update_window(window.into(), |_, w, cx| {
            let move_to = |w: &mut Window, cx: &mut App, position| {
                w.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }), cx);
                w.refresh();
                w.draw(cx).clear(cx);
            };
            move_to(w, cx, point(px(800.), px(700.)));
            move_to(w, cx, dot);
            assert!(view.read(cx).health_hover_probe().1, "dot opens the card");
            move_to(w, cx, dot + point(px(140.), px(50.)));
            assert!(view.read(cx).health_hover_probe().1, "moving into card preserves hover");
            move_to(w, cx, point(px(800.), px(700.)));
            assert!(!view.read(cx).health_hover_probe().1, "leaving the card clears hover");
        })
        .expect("dispatch health hover lifecycle");
    });
}

/// Compare the shared text/icon segment feedback, including hovering the selected cell.
async fn appearance_hover_shots(st: &std::sync::Arc<crate::state::AppState>, dir: &std::path::Path, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    let scope = cx.update(|cx| {
        settings::open(st.clone(), Page::General, cx);
        let (window, _) = settings::handle(cx).expect("settings window");
        window.update(cx, |_, w, _| w.resize(size(px(1200.), px(1400.)))).unwrap();
        settings::page_motion(Page::General, cx).expect("general scope")
    });
    for (appearance, theme) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
        st.cfg.update(|c| c.general.appearance = appearance).unwrap();
        for glyph in ["system", "light", "dark"] {
            for control in ["appearance", "hud-appearance"] {
                for other in ["system", "light", "dark"] {
                    crate::ui::force_hover(
                        &scope,
                        ElementId::from((ElementId::from(control), SharedString::from(other))),
                        false,
                    );
                }
            }
            crate::ui::force_hover(
                &scope,
                ElementId::from((ElementId::from("appearance"), SharedString::from(glyph))),
                true,
            );
            cx.update(|cx| {
                let (window, _) = settings::handle(cx).expect("settings window");
                window.update(cx, |_, w, _| w.refresh()).unwrap();
            });
            page_shot(cx, dir, &format!("appearance-{theme}-hover-{glyph}.png")).await;
        }
    }
    st.cfg.update(|c| c.general.appearance = original.general.appearance).unwrap();
}

async fn model_shots(st: &std::sync::Arc<crate::state::AppState>, dir: &std::path::Path, cx: &mut AsyncApp) {
    use dito_core::{
        config::{ProviderKind, Source},
        provider::configured_asr::AsrProfiles,
    };
    let original = st.cfg.get();
    for appearance in [Appearance::Light, Appearance::Dark] {
        let tag = if appearance == Appearance::Dark { "dark" } else { "light" };
        for (name, polish, local, preset, editor, help) in [
            ("local-asr", false, true, 0, None, false),
            ("local-polish", true, true, 0, None, false),
            ("funasr", false, false, 0, None, false),
            ("qwen-asr", false, false, 1, None, false),
            ("openrouter", false, false, 2, None, false),
            ("azure", true, false, 0, None, false),
            ("bedrock", true, false, 1, None, false),
            ("compatible", true, false, 2, None, false),
            ("editor", false, false, 1, Some(false), false),
            ("editor-error", false, false, 1, Some(true), false),
            ("help", false, false, 1, Some(false), true),
        ] {
            if std::env::var("DITO_MODEL_SHOTS")
                .ok()
                .is_some_and(|filter| !filter.split(',').any(|entry| entry == name))
            {
                continue;
            }
            let _ = st.cfg.update(|c| {
                c.general.display_language = dito_core::config::DisplayLanguage::Ja;
                c.general.appearance = appearance;
                c.asr.source = if local { Source::Local } else { Source::Provider };
                c.polish.engine.source = c.asr.source;
                c.polish.engine.provider.kind =
                    [ProviderKind::Azure, ProviderKind::Bedrock, ProviderKind::Compat][preset];
                let mut p = AsrProfiles { selected: preset, ..Default::default() };
                for v in &mut p.profiles {
                    v["connection"]["model"] = "my-asr-model".into();
                }
                c.asr.cloud_asr = Some(p);
            });
            let st2 = st.clone();
            cx.update(|cx| settings::open(st2, Page::Models, cx));
            cx.update(|cx| {
                let (w, view) = settings::handle(cx).unwrap();
                let _ = w.update(cx, |_, window, cx| {
                    view.update(cx, |s, cx| s.preview_models(polish, editor, help, window, cx))
                });
            });
            page_shot(cx, dir, &format!("models-{name}-{tag}.png")).await;
            if editor == Some(false) {
                cx.update(|cx| {
                    let (w, _) = settings::handle(cx).unwrap();
                    let _ = w.update(cx, |_, w, _| w.resize(size(px(1000.), px(680.))));
                });
                page_shot(cx, dir, &format!("models-{name}-compact-{tag}.png")).await;
                cx.update(|cx| {
                    let (w, _) = settings::handle(cx).unwrap();
                    let _ = w.update(cx, |_, w, _| w.resize(size(px(1120.), px(860.))));
                });
            }
        }
    }
    let _ = st.cfg.update(|c| *c = (*original).clone());
}
