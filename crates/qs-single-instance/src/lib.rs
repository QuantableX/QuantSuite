//! A second launch must never be able to wedge itself on the first one.
//!
//! This is `tauri-plugin-single-instance`'s Windows implementation with one
//! call changed, and that call is the whole reason the crate exists.
//!
//! Upstream hands the second process's argv to the running instance with
//! `SendMessageW(hwnd, WM_COPYDATA, ..)` and then exits. `SendMessageW` blocks
//! until the *receiving* thread pumps the message, and it has no timeout. The
//! situation in which a user launches the app a second time is, very often,
//! precisely the situation in which the first instance's main thread has
//! stopped pumping — so the second process blocks there forever and never
//! reaches its `exit(0)`.
//!
//! What that looked like in practice, from the hang of 2026-08-26: the running
//! instance wedged at 07:20:31, a relaunch at 07:21:43 froze in this handshake
//! with two threads and no CPU, the single-instance mutex stayed held, and the
//! tray answered nothing. Every further click on the icon added another stuck
//! process. One hung main thread turned into an app that could not be started
//! again either.
//!
//! `SendMessageTimeoutW` with `SMTO_ABORTIFHUNG` bounds it: the handshake
//! either lands or gives up, and the second process exits in both cases.
//!
//! Off Windows the plugin is upstream's, unchanged — the bug is a Win32 one and
//! has no counterpart there.

use tauri::{plugin::TauriPlugin, AppHandle, Manager, Runtime};

pub(crate) type SingleInstanceCallback<R> =
    dyn FnMut(&AppHandle<R>, Vec<String>, String) + Send + Sync + 'static;

/// Same signature as the upstream plugin's `init`, so swapping between them is
/// a one-line change at the call site.
#[cfg(windows)]
pub fn init<R, F>(f: F) -> TauriPlugin<R>
where
    R: Runtime,
    F: FnMut(&AppHandle<R>, Vec<String>, String) + Send + Sync + 'static,
{
    windows_impl::init(Box::new(f))
}

#[cfg(not(windows))]
pub fn init<R, F>(f: F) -> TauriPlugin<R>
where
    R: Runtime,
    F: FnMut(&AppHandle<R>, Vec<String>, String) + Send + Sync + 'static,
{
    tauri_plugin_single_instance::init(f)
}

/// Release the mutex and tear down the receiver window. Wired to `RunEvent::Exit`.
pub fn destroy<R: Runtime, M: Manager<R>>(manager: &M) {
    #[cfg(windows)]
    windows_impl::destroy(manager);
    #[cfg(not(windows))]
    tauri_plugin_single_instance::destroy(manager);
}

#[cfg(windows)]
mod windows_impl {
    use super::SingleInstanceCallback;
    use std::ffi::CStr;
    use tauri::{
        plugin::{self, TauriPlugin},
        AppHandle, Manager, RunEvent, Runtime,
    };
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM,
        },
        System::{
            DataExchange::COPYDATASTRUCT,
            LibraryLoader::GetModuleHandleW,
            Threading::{CreateMutexW, ReleaseMutex},
        },
        UI::WindowsAndMessaging::{
            self as w32wm, CreateWindowExW, DefWindowProcW, DestroyWindow, FindWindowW,
            RegisterClassExW, SendMessageTimeoutW, CREATESTRUCTW, GWLP_USERDATA, GWL_STYLE,
            SMTO_ABORTIFHUNG, SMTO_NORMAL, WINDOW_LONG_PTR_INDEX, WM_COPYDATA, WM_CREATE,
            WM_DESTROY, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            WS_EX_TRANSPARENT, WS_OVERLAPPED, WS_POPUP, WS_VISIBLE,
        },
    };

    const WMCOPYDATA_SINGLE_INSTANCE_DATA: usize = 1542;

    /// How long to wait for the running instance to take the handshake.
    ///
    /// A healthy instance answers in single-digit milliseconds — it only has to
    /// unhide a window. `SMTO_ABORTIFHUNG` already returns at once when Windows
    /// has flagged the target as not responding, which it does after about five
    /// seconds of a stalled message loop; this timeout is what covers the gap
    /// before that flag is set. Generous, and bounded, which is the point.
    const HANDSHAKE_TIMEOUT_MS: u32 = 3_000;

    struct MutexHandle(isize);
    struct TargetWindowHandle(isize);

    struct UserData<R: Runtime> {
        app: AppHandle<R>,
        callback: Box<SingleInstanceCallback<R>>,
    }

    impl<R: Runtime> UserData<R> {
        unsafe fn from_hwnd_raw(hwnd: HWND) -> *mut Self {
            GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Self
        }

        unsafe fn from_hwnd<'a>(hwnd: HWND) -> &'a mut Self {
            &mut *Self::from_hwnd_raw(hwnd)
        }

        fn run_callback(&mut self, args: Vec<String>, cwd: String) {
            (self.callback)(&self.app, args, cwd)
        }
    }

    pub fn init<R: Runtime>(callback: Box<SingleInstanceCallback<R>>) -> TauriPlugin<R> {
        plugin::Builder::new("single-instance")
            .setup(|app, _api| {
                let id = app.config().identifier.clone();

                let class_name = encode_wide(format!("{id}-sic"));
                let window_name = encode_wide(format!("{id}-siw"));
                let mutex_name = encode_wide(format!("{id}-sim"));

                let hmutex =
                    unsafe { CreateMutexW(std::ptr::null(), true.into(), mutex_name.as_ptr()) };

                if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                    unsafe {
                        let hwnd = FindWindowW(class_name.as_ptr(), window_name.as_ptr());

                        if !hwnd.is_null() {
                            let cwd = std::env::current_dir().unwrap_or_default();
                            let cwd = cwd.to_str().unwrap_or_default();
                            let args = std::env::args().collect::<Vec<String>>().join("|");
                            let data = format!("{cwd}|{args}\0");

                            let bytes = data.as_bytes();
                            let cds = COPYDATASTRUCT {
                                dwData: WMCOPYDATA_SINGLE_INSTANCE_DATA,
                                cbData: bytes.len() as _,
                                lpData: bytes.as_ptr() as _,
                            };

                            // The one changed line — see the module docs.
                            let mut answer: usize = 0;
                            let delivered = SendMessageTimeoutW(
                                hwnd,
                                WM_COPYDATA,
                                0,
                                &cds as *const _ as _,
                                SMTO_NORMAL | SMTO_ABORTIFHUNG,
                                HANDSHAKE_TIMEOUT_MS,
                                &mut answer,
                            ) != 0;

                            if !delivered {
                                // Setup order puts this plugin ahead of qs-core,
                                // so the logger it would otherwise rely on does
                                // not exist yet. Installing it is idempotent.
                                qs_core::diagnostics::ensure_logger();
                                log::error!(
                                    "second launch: the running instance did not take the \
                                     handshake within {HANDSHAKE_TIMEOUT_MS}ms — its main thread \
                                     is hung. Exiting this process rather than blocking on it; \
                                     end quantsuite.exe and start it again."
                                );
                            }

                            app.cleanup_before_exit();
                            std::process::exit(0);
                        }
                    }
                } else {
                    app.manage(MutexHandle(hmutex as _));

                    let userdata = UserData { app: app.clone(), callback };
                    let userdata = Box::into_raw(Box::new(userdata));
                    let hwnd = create_event_target_window::<R>(&class_name, &window_name, userdata);
                    app.manage(TargetWindowHandle(hwnd as _));
                }

                Ok(())
            })
            .on_event(|app, event| {
                if let RunEvent::Exit = event {
                    destroy(app);
                }
            })
            .build()
    }

    pub fn destroy<R: Runtime, M: Manager<R>>(manager: &M) {
        if let Some(hmutex) = manager.try_state::<MutexHandle>() {
            unsafe {
                ReleaseMutex(hmutex.0 as _);
                CloseHandle(hmutex.0 as _);
            }
        }
        if let Some(hwnd) = manager.try_state::<TargetWindowHandle>() {
            unsafe { DestroyWindow(hwnd.0 as _) };
        }
    }

    unsafe extern "system" fn single_instance_window_proc<R: Runtime>(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_CREATE => {
                let create_struct = &*(lparam as *const CREATESTRUCTW);
                let userdata = create_struct.lpCreateParams as *const UserData<R>;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, userdata as _);
                0
            }

            WM_COPYDATA => {
                let cds_ptr = lparam as *const COPYDATASTRUCT;
                if (*cds_ptr).dwData == WMCOPYDATA_SINGLE_INSTANCE_DATA {
                    let userdata = UserData::<R>::from_hwnd(hwnd);
                    let data = CStr::from_ptr((*cds_ptr).lpData as _).to_string_lossy();
                    let mut s = data.split('|');
                    let cwd = s.next().unwrap_or_default();
                    let args = s.map(|s| s.to_string()).collect();
                    userdata.run_callback(args, cwd.to_string());
                }
                1
            }

            WM_DESTROY => {
                let userdata = UserData::<R>::from_hwnd_raw(hwnd);
                drop(Box::from_raw(userdata));
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    fn create_event_target_window<R: Runtime>(
        class_name: &[u16],
        window_name: &[u16],
        userdata: *const UserData<R>,
    ) -> HWND {
        unsafe {
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(single_instance_window_proc::<R>),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: GetModuleHandleW(std::ptr::null()),
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class_name.as_ptr(),
                hIconSm: std::ptr::null_mut(),
            };

            RegisterClassExW(&class);

            let hwnd = CreateWindowExW(
                // WS_EX_TOOLWINDOW keeps this receiver out of the taskbar, where
                // it would otherwise surface after a few hours or an explorer
                // restart. Upstream's note, and it still applies.
                WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
                class_name.as_ptr(),
                window_name.as_ptr(),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                userdata as _,
            );
            SetWindowLongPtrW(hwnd, GWL_STYLE, (WS_VISIBLE | WS_POPUP) as isize);
            hwnd
        }
    }

    fn encode_wide(string: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
        std::os::windows::prelude::OsStrExt::encode_wide(string.as_ref())
            .chain(std::iter::once(0))
            .collect()
    }

    #[cfg(target_pointer_width = "32")]
    #[allow(non_snake_case)]
    unsafe fn SetWindowLongPtrW(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX, value: isize) -> isize {
        w32wm::SetWindowLongW(hwnd, index, value as _) as _
    }

    #[cfg(target_pointer_width = "64")]
    #[allow(non_snake_case)]
    unsafe fn SetWindowLongPtrW(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX, value: isize) -> isize {
        w32wm::SetWindowLongPtrW(hwnd, index, value)
    }

    #[cfg(target_pointer_width = "32")]
    #[allow(non_snake_case)]
    unsafe fn GetWindowLongPtrW(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX) -> isize {
        w32wm::GetWindowLongW(hwnd, index) as _
    }

    #[cfg(target_pointer_width = "64")]
    #[allow(non_snake_case)]
    unsafe fn GetWindowLongPtrW(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX) -> isize {
        w32wm::GetWindowLongPtrW(hwnd, index)
    }
}
