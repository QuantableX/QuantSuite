use std::{env, fs, path::PathBuf};

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("target architecture");
    let source = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("../../vendor/conpty")
        .join(&arch);
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let profile = out.ancestors().nth(3).expect("Cargo profile directory");
    // Cargo binaries, integration tests and examples all resolve the runtime
    // relative to their own executable. The installer uses the same layout.
    for file in ["conpty.dll", "OpenConsole.exe"] {
        let input = source.join(file);
        println!("cargo:rerun-if-changed={}", input.display());
        let bytes = fs::read(&input).expect("vendored Microsoft ConPTY runtime");
        for base in [
            profile.to_path_buf(),
            profile.join("deps"),
            profile.join("examples"),
        ] {
            let target = base.join("conpty").join(&arch);
            fs::create_dir_all(&target).expect("create terminal runtime directory");
            let destination = target.join(file);
            // A running dev app holds the DLL open. Don't rewrite identical
            // files on an unrelated rebuild (Windows rejects that write).
            if fs::read(&destination).ok().as_deref() != Some(bytes.as_slice()) {
                fs::write(&destination, &bytes).expect("stage terminal runtime");
            }
        }
    }
}
