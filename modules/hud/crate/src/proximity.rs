//! Native proximity keeps working while the WebView is hidden/throttled.
//! Visibility and opening are separate: this module never expands a panel.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{Manager, WebviewWindow, WindowEvent};

const ENTER_DISTANCE: f64 = 64.0;
const EXIT_DISTANCE: f64 = 96.0;
const EXIT_DELAY: Duration = Duration::from_millis(250);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Rect {
    fn contains(self, (x, y): (f64, f64)) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    fn distance(self, (x, y): (f64, f64)) -> f64 {
        let dx = (self.x - x).max(x - self.x - self.width).max(0.0);
        let dy = (self.y - y).max(y - self.y - self.height).max(0.0);
        dx.hypot(dy)
    }
}

#[derive(Clone, Copy, Debug)]
struct Area {
    tab: Rect,
    monitor: Rect,
    scale: f64,
}

impl Area {
    fn distance(self, cursor: Option<(f64, f64)>) -> f64 {
        cursor
            .filter(|point| self.monitor.contains(*point))
            .map(|point| self.tab.distance(point) / self.scale)
            .unwrap_or(f64::INFINITY)
    }
}

#[derive(Clone, Copy, Debug, Default)]
enum Placement {
    #[default]
    Unplaced,
    Tucked(Area),
    Expanded,
}

#[derive(Default)]
struct Visibility {
    placement: Placement,
    paused: bool,
    near: bool,
    outside_since: Option<Duration>,
}

impl Visibility {
    fn place(&mut self, placement: Placement) {
        *self = Self {
            placement,
            ..Self::default()
        };
    }

    fn desired(
        &mut self,
        enabled: bool,
        cursor: Option<(f64, f64)>,
        now: Duration,
    ) -> Option<bool> {
        // A tray/close request outranks pending placement and proximity.
        if !enabled {
            self.near = false;
            self.outside_since = None;
            return Some(false);
        }
        if self.paused {
            return None;
        }
        let area = match self.placement {
            Placement::Unplaced => return Some(false),
            Placement::Expanded => return Some(true),
            Placement::Tucked(area) => area,
        };
        let distance = area.distance(cursor);
        if distance
            <= if self.near {
                EXIT_DISTANCE
            } else {
                ENTER_DISTANCE
            }
        {
            self.near = true;
            self.outside_since = None;
        } else if self.near {
            let since = self.outside_since.get_or_insert(now);
            if now.saturating_sub(*since) >= EXIT_DELAY {
                self.near = false;
                self.outside_since = None;
            }
        }
        Some(self.near)
    }
}

struct Overlay {
    window: WebviewWindow,
    alive: Arc<AtomicBool>,
    visibility: Visibility,
}

struct Controller {
    enabled: bool,
    overlays: HashMap<String, Overlay>,
}

struct State {
    controller: Mutex<Controller>,
    queued: AtomicBool,
    stopped: AtomicBool,
    started: Instant,
    #[cfg(test)]
    cursor_override: Mutex<Option<(f64, f64)>>,
}

/// All visibility decisions and window effects run on the UI thread. A
/// queued tick reads the current state, never an old "hide" decision that
/// might arrive after a click has already opened the HUD.
fn refresh(controller: &mut Controller, app: &tauri::AppHandle, now: Duration) {
    controller
        .overlays
        .retain(|_, overlay| overlay.alive.load(Ordering::Relaxed));
    if controller.overlays.is_empty() {
        return;
    }
    let cursor = app.cursor_position().ok().map(|p| (p.x, p.y));
    #[cfg(test)]
    let cursor = app
        .state::<State>()
        .cursor_override
        .lock()
        .unwrap()
        .or(cursor);
    for overlay in controller.overlays.values_mut() {
        let Some(visible) = overlay.visibility.desired(controller.enabled, cursor, now) else {
            continue;
        };
        if overlay.window.is_visible().unwrap_or(false) == visible {
            continue;
        }
        let result = if visible {
            // Both overlays are built with focused(false). Keep show/hide in
            // Tauri so its cached window flags stay in sync with the HWND.
            overlay.window.show()
        } else {
            overlay.window.hide()
        };
        if let Err(e) = result {
            log::warn!(
                "hud: proximity visibility for '{}': {e}",
                overlay.window.label()
            );
        }
    }
}

fn change(
    app: &tauri::AppHandle,
    update: impl FnOnce(&mut Controller) + Send + 'static,
) -> Result<(), String> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let state = handle.state::<State>();
        if state.stopped.load(Ordering::Relaxed) {
            return;
        }
        if let Ok(mut controller) = state.controller.lock() {
            update(&mut controller);
            refresh(&mut controller, &handle, state.started.elapsed());
        };
    })
    .map_err(|e| e.to_string())
}

pub fn init(app: &tauri::AppHandle) {
    app.manage(State {
        controller: Mutex::new(Controller {
            enabled: true,
            overlays: HashMap::new(),
        }),
        queued: AtomicBool::new(false),
        stopped: AtomicBool::new(false),
        started: Instant::now(),
        #[cfg(test)]
        cursor_override: Mutex::new(None),
    });
    let handle = app.clone();
    qs_core::shutdown::on_shutdown(
        app,
        "hud:proximity",
        Box::new(move || {
            handle
                .state::<State>()
                .stopped
                .store(true, Ordering::Relaxed);
        }),
    );
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(POLL_INTERVAL);
        let state = handle.state::<State>();
        if state.stopped.load(Ordering::Relaxed) {
            break;
        }
        if state
            .controller
            .try_lock()
            .map(|c| c.overlays.is_empty() || !c.enabled)
            .unwrap_or(true)
        {
            continue;
        }
        // At most one pending tick, even while the UI thread is busy.
        if state.queued.swap(true, Ordering::Relaxed) {
            continue;
        }
        let tick_app = handle.clone();
        if handle
            .run_on_main_thread(move || {
                let state = tick_app.state::<State>();
                if !state.stopped.load(Ordering::Relaxed) {
                    if let Ok(mut controller) = state.controller.lock() {
                        refresh(&mut controller, &tick_app, state.started.elapsed());
                    }
                }
                state.queued.store(false, Ordering::Relaxed);
            })
            .is_err()
        {
            break;
        }
    });
}

pub fn register(window: &WebviewWindow) -> Result<(), String> {
    let alive = Arc::new(AtomicBool::new(true));
    let lifetime = alive.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            lifetime.store(false, Ordering::Relaxed);
        }
    });
    let window_handle = window.clone();
    change(window.app_handle(), move |controller| {
        controller.overlays.insert(
            window_handle.label().to_string(),
            Overlay {
                window: window_handle,
                alive,
                visibility: Visibility::default(),
            },
        );
    })
}

pub fn pause(window: &WebviewWindow) -> Result<(), String> {
    let label = window.label().to_string();
    change(window.app_handle(), move |controller| {
        if let Some(overlay) = controller.overlays.get_mut(&label) {
            overlay.visibility.paused = true;
        }
    })
}

fn place(window: &WebviewWindow, placement: Placement) -> Result<(), String> {
    let label = window.label().to_string();
    change(window.app_handle(), move |controller| {
        if let Some(overlay) = controller.overlays.get_mut(&label) {
            overlay.visibility.place(placement);
        }
    })
}

pub fn tucked(window: &WebviewWindow, monitor: &tauri::Monitor) -> Result<(), String> {
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    place(
        window,
        Placement::Tucked(Area {
            tab: Rect {
                x: position.x as f64,
                y: position.y as f64,
                width: size.width as f64,
                height: size.height as f64,
            },
            monitor: Rect {
                x: monitor.position().x as f64,
                y: monitor.position().y as f64,
                width: monitor.size().width as f64,
                height: monitor.size().height as f64,
            },
            scale: monitor.scale_factor(),
        }),
    )
}

pub fn expanded(window: &WebviewWindow) -> Result<(), String> {
    place(window, Placement::Expanded)
}

pub fn set_enabled(app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
    change(app, move |controller| {
        controller.enabled = enabled;
    })
}

pub fn toggle(app: &tauri::AppHandle) -> Result<(), String> {
    change(app, |controller| {
        controller.enabled = !controller.enabled;
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn area(scale: f64, right: bool, origin: (f64, f64)) -> Area {
        Area {
            tab: Rect {
                x: origin.0 + if right { 1900.0 * scale } else { 0.0 },
                y: origin.1 + 500.0 * scale,
                width: 20.0 * scale,
                height: 88.0 * scale,
            },
            monitor: Rect {
                x: origin.0,
                y: origin.1,
                width: 1920.0 * scale,
                height: 1080.0 * scale,
            },
            scale,
        }
    }
    fn at(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn top_tab_uses_its_horizontal_footprint_and_target_monitor() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let origin = (-3840.0, -2160.0);
            let mut a = area(scale, false, origin);
            a.tab = Rect {
                x: origin.0 + (1920.0 - 88.0) * scale / 2.0,
                y: origin.1,
                width: 88.0 * scale,
                height: 20.0 * scale,
            };
            let mut v = Visibility::default();
            v.place(Placement::Tucked(a));
            let x = a.tab.x + 44.0 * scale;
            assert_eq!(
                v.desired(true, Some((x, a.tab.y - 1.0)), at(0)),
                Some(false)
            );
            assert_eq!(
                v.desired(true, Some((x, a.tab.y + 83.0 * scale)), at(50)),
                Some(true)
            );
            assert_eq!(
                v.desired(true, Some((x, a.tab.y + 110.0 * scale)), at(100)),
                Some(true)
            );
            assert_eq!(
                v.desired(true, Some((x, a.tab.y + 140.0 * scale)), at(150)),
                Some(true)
            );
            assert_eq!(
                v.desired(true, Some((x, a.tab.y + 140.0 * scale)), at(400)),
                Some(false)
            );
        }
    }

    #[test]
    fn hidden_tab_reveals_nearby_without_expanding_at_every_dpi_and_side() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for right in [false, true] {
                let area = area(scale, right, (-3840.0, -1080.0));
                let mut v = Visibility::default();
                v.place(Placement::Tucked(area));
                let cursor = |distance: f64| {
                    Some((
                        area.tab.x
                            + if right {
                                -distance * scale
                            } else {
                                (20.0 + distance) * scale
                            },
                        area.tab.y + 44.0 * scale,
                    ))
                };
                assert_eq!(v.desired(true, cursor(120.0), at(0)), Some(false));
                assert_eq!(v.desired(true, cursor(63.0), at(50)), Some(true));
                assert!(matches!(v.placement, Placement::Tucked(_)));
                assert_eq!(v.desired(true, cursor(80.0), at(100)), Some(true));
                assert_eq!(v.desired(true, cursor(120.0), at(150)), Some(true));
                assert_eq!(v.desired(true, cursor(120.0), at(399)), Some(true));
                assert_eq!(v.desired(true, cursor(120.0), at(400)), Some(false));
                assert_eq!(v.desired(true, cursor(80.0), at(450)), Some(false));
            }
        }
    }

    #[test]
    fn leaving_then_returning_cancels_hide_and_open_panels_ignore_distance() {
        let area = area(1.0, false, (0.0, 0.0));
        let mut v = Visibility::default();
        v.place(Placement::Tucked(area));
        assert_eq!(v.desired(true, Some((40.0, 540.0)), at(0)), Some(true));
        assert_eq!(v.desired(true, None, at(50)), Some(true));
        assert_eq!(v.desired(true, Some((40.0, 540.0)), at(200)), Some(true));
        assert_eq!(v.desired(true, None, at(300)), Some(true));
        v.place(Placement::Expanded);
        assert_eq!(v.desired(true, None, at(1000)), Some(true));
        assert_eq!(v.desired(false, None, at(1050)), Some(false));
        assert_eq!(v.desired(true, None, at(1100)), Some(true));
    }

    #[test]
    fn manual_hide_placement_and_another_monitor_never_reveal_the_tab() {
        let mut v = Visibility::default();
        assert_eq!(v.desired(true, Some((0.0, 540.0)), at(0)), Some(false));
        v.place(Placement::Tucked(area(1.0, false, (0.0, 0.0))));
        assert_eq!(v.desired(true, Some((-1.0, 540.0)), at(0)), Some(false));
        assert_eq!(v.desired(false, Some((0.0, 540.0)), at(0)), Some(false));
        v.paused = true;
        assert_eq!(v.desired(true, Some((0.0, 540.0)), at(0)), None);
        assert_eq!(v.desired(false, Some((0.0, 540.0)), at(0)), Some(false));
        v.place(Placement::Tucked(area(1.5, true, (-2880.0, -1620.0))));
        assert_eq!(v.desired(true, Some((0.0, 540.0)), at(0)), Some(false));
    }

    // Real Wry event loop and WebView2, isolated/offscreen with no suite
    // plugins, user config or server. Run separately because a process may
    // only create one native event loop. Cursor injection avoids moving the
    // user's actual pointer during the test. Run with:
    // cargo test -p tauri-plugin-hud --lib --features native-tests native_visibility_round_trip -- --ignored --test-threads=1
    #[test]
    #[ignore = "native WebView2 smoke test; run explicitly on Windows"]
    #[cfg(all(windows, feature = "native-tests"))]
    fn native_visibility_round_trip() {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let (tx, rx) = std::sync::mpsc::channel();
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = "com.quantable.hud.proximity-test".into();
        let app = tauri::Builder::default()
            .any_thread()
            .setup(move |app| {
                init(app.handle());
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let w = tauri::WebviewWindowBuilder::new(
                            &handle,
                            super::super::HUD_WINDOW,
                            tauri::WebviewUrl::External("about:blank".parse().unwrap()),
                        )
                        .title("QuantHUD proximity test")
                        .visible(false)
                        .focused(false)
                        .position(-20000.0, -20000.0)
                        .inner_size(20.0, 88.0)
                        .resizable(false)
                        .decorations(false)
                        .shadow(false)
                        .transparent(true)
                        .skip_taskbar(true)
                        .data_directory(
                            std::env::temp_dir()
                                .join(format!("qs-hud-proximity-test-{}", std::process::id())),
                        )
                        .build()
                        .unwrap();
                        w.set_size(tauri::PhysicalSize::new(20, 88)).unwrap();
                        register(&w).unwrap();
                        let state = handle.state::<State>();
                        let set_cursor = |point| {
                            *state.cursor_override.lock().unwrap() = Some(point);
                        };
                        let settle = || {
                            let (sent, received) = std::sync::mpsc::channel();
                            handle
                                .run_on_main_thread(move || {
                                    sent.send(()).unwrap();
                                })
                                .unwrap();
                            received.recv_timeout(Duration::from_secs(5)).unwrap();
                        };
                        let expected = |visible| {
                            for _ in 0..40 {
                                if w.is_visible().unwrap() == visible {
                                    return;
                                }
                                std::thread::sleep(Duration::from_millis(25));
                            }
                            panic!("native visibility did not become {visible}");
                        };
                        set_cursor((500.0, 500.0));
                        let mut a = area(1.0, false, (0.0, 0.0));
                        place(&w, Placement::Tucked(a)).unwrap();
                        settle();
                        expected(false);
                        let foreground = unsafe { GetForegroundWindow() };
                        set_cursor((40.0, 540.0));
                        expected(true);
                        assert_eq!(
                            unsafe { GetForegroundWindow() },
                            foreground,
                            "proximity stole focus"
                        );
                        assert_eq!(
                            w.outer_size().unwrap().width,
                            20,
                            "proximity expanded the window"
                        );
                        set_cursor((500.0, 500.0));
                        expected(false);
                        place(&w, Placement::Expanded).unwrap();
                        settle();
                        expected(true);
                        std::thread::sleep(Duration::from_millis(350));
                        expected(true);
                        set_enabled(&handle, false).unwrap();
                        settle();
                        expected(false);
                        set_cursor((40.0, 540.0));
                        place(&w, Placement::Tucked(a)).unwrap();
                        settle();
                        expected(false);
                        toggle(&handle).unwrap();
                        settle();
                        expected(true);
                        pause(&w).unwrap();
                        settle();
                        set_cursor((500.0, 500.0));
                        std::thread::sleep(Duration::from_millis(350));
                        expected(true);
                        a.tab.x = 1900.0;
                        place(&w, Placement::Tucked(a)).unwrap();
                        settle();
                        expected(false);
                        set_cursor((1880.0, 540.0));
                        expected(true);
                        assert_eq!(
                            unsafe { GetForegroundWindow() },
                            foreground,
                            "a later proximity reveal stole focus"
                        );
                        // Exercise the new dimensions and real Windows region
                        // as well as visibility, without moving onto the desktop.
                        let work = super::super::layout::Frame {
                            x: -20000,
                            y: -20000,
                            width: 1920,
                            height: 1080,
                        };
                        let tab = super::super::layout::frame(
                            work,
                            1.0,
                            super::super::layout::Edge::Top,
                            false,
                        );
                        let top = Area {
                            tab: Rect {
                                x: tab.x as f64,
                                y: tab.y as f64,
                                width: tab.width as f64,
                                height: tab.height as f64,
                            },
                            monitor: Rect {
                                x: work.x as f64,
                                y: work.y as f64,
                                width: work.width as f64,
                                height: work.height as f64,
                            },
                            scale: 1.0,
                        };
                        pause(&w).unwrap();
                        w.set_size(tauri::PhysicalSize::new(tab.width, tab.height))
                            .unwrap();
                        w.set_position(tauri::PhysicalPosition::new(tab.x, tab.y))
                            .unwrap();
                        super::super::trigger_region::apply(
                            &w,
                            1.0,
                            super::super::layout::Edge::Top,
                            false,
                        )
                        .unwrap();
                        set_cursor((work.x as f64 + 500.0, work.y as f64 + 500.0));
                        place(&w, Placement::Tucked(top)).unwrap();
                        settle();
                        expected(false);
                        set_cursor((tab.x as f64 + 44.0, tab.y as f64 + 50.0));
                        expected(true);
                        assert_eq!(w.outer_size().unwrap(), tauri::PhysicalSize::new(88, 20));
                        assert_eq!(
                            w.outer_position().unwrap(),
                            tauri::PhysicalPosition::new(tab.x, tab.y)
                        );
                        let panel = super::super::layout::frame(
                            work,
                            1.0,
                            super::super::layout::Edge::Top,
                            true,
                        );
                        pause(&w).unwrap();
                        w.set_position(tauri::PhysicalPosition::new(panel.x, panel.y))
                            .unwrap();
                        w.set_size(tauri::PhysicalSize::new(panel.width, panel.height))
                            .unwrap();
                        super::super::trigger_region::apply(
                            &w,
                            1.0,
                            super::super::layout::Edge::Top,
                            true,
                        )
                        .unwrap();
                        expanded(&w).unwrap();
                        settle();
                        expected(true);
                        set_cursor((500.0, 500.0));
                        std::thread::sleep(Duration::from_millis(350));
                        expected(true);
                        assert_eq!(w.outer_size().unwrap(), tauri::PhysicalSize::new(1920, 232));
                        assert_eq!(
                            unsafe { GetForegroundWindow() },
                            foreground,
                            "top reveal stole focus"
                        );
                        w.destroy().unwrap();
                        settle();
                        std::thread::sleep(Duration::from_millis(100));
                        assert!(state.controller.lock().unwrap().overlays.is_empty());
                    }));
                    handle
                        .state::<State>()
                        .stopped
                        .store(true, Ordering::Relaxed);
                    tx.send(outcome.is_ok()).unwrap();
                    handle.exit(if outcome.is_ok() { 0 } else { 1 });
                });
                Ok(())
            })
            .build(context)
            .unwrap();
        let exit_code = app.run_return(|_, event| {
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                api.prevent_exit();
            }
        });
        assert_eq!(exit_code, 0);
        assert!(rx.recv_timeout(Duration::from_secs(1)).unwrap());
    }
}
