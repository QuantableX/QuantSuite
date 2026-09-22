//! Pipe a real session through stdin/stdout for renderer diagnostics without
//! starting Tauri. Usage: terminal_bridge <shell/program> [arguments...]
use qs_pty::{PtyEvent, PtyOptions, PtySession};
use std::io::{Read, Write};

fn main() -> Result<(), String> {
    let mut arguments = std::env::args().skip(1);
    let mut session = PtySession::spawn(
        &PtyOptions {
            shell: arguments.next(),
            args: arguments.collect(),
            cols: 100,
            rows: 30,
            ..PtyOptions::default()
        },
        Box::new(|event| {
            if let PtyEvent::Output(text) = event {
                let mut output = std::io::stdout().lock();
                let _ = output.write_all(text.as_bytes());
                let _ = output.flush();
            }
        }),
    )?;
    let mut buffer = [0; 4096];
    let mut input = std::io::stdin().lock();
    loop {
        let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        session
            .writer()
            .lock()
            .map_err(|e| e.to_string())?
            .write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
    }
    session.kill()
}
