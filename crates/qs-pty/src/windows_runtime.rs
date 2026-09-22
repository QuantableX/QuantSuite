//! Load the shipped ConPTY before portable-pty resolves `conpty.dll` by name.
//! The in-box Windows console host can emit DEC 2026 boundaries before the
//! redraw itself, exposing intermediate cursor positions even in xterm 6.
use libloading::os::windows::{
    Library, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use std::sync::OnceLock;

// Keep the module loaded for the lifetime of every PTY. Windows resolves the
// subsequent basename load in portable-pty to this already-loaded module.
static RUNTIME: OnceLock<Result<Library, String>> = OnceLock::new();

pub fn load() -> Result<(), String> {
    RUNTIME
        .get_or_init(|| {
            let executable = std::env::current_exe().map_err(|e| e.to_string())?;
            let directory = executable.parent().ok_or("missing executable directory")?;
            let runtime = directory.join("conpty").join(std::env::consts::ARCH);
            let dll = runtime.join("conpty.dll");
            if !runtime.join("OpenConsole.exe").is_file() {
                return Err(format!("Missing terminal runtime: {}", runtime.display()));
            }
            // SAFETY: absolute application-owned path, architecture selected at
            // compile time, and dependencies searched only beside it/in System32.
            // Never search the terminal's workspace for native libraries.
            unsafe {
                Library::load_with_flags(
                    &dll,
                    LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
                )
            }
            .map_err(|e| format!("Could not load terminal runtime {}: {e}", dll.display()))
        })
        .as_ref()
        .map(|_| ())
        .map_err(Clone::clone)
}
