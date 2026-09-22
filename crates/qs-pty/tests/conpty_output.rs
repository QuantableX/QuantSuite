#![cfg(windows)]

use qs_pty::{PtyEvent, PtyOptions, PtySession};
use std::io::Write;
use std::sync::mpsc;
use std::time::Duration;

/// Exercise the native console host, not a pre-recorded VT string. Older
/// in-box ConPTY flushes the sync markers before the screen changes they wrap.
#[test]
fn synchronized_redraw_keeps_text_inside_frame() {
    let (send, receive) = mpsc::channel();
    let mut session = PtySession::spawn(
        &PtyOptions {
            shell: Some("powershell.exe".into()),
            args: vec![
                "-NoLogo".into(), "-NoProfile".into(), "-NonInteractive".into(),
                "-Command".into(),
                "$e=[char]27; [Console]::Write(\"$e[?2026h$e[3;2HQS-SYNC-PROBE$e[12;4H$e[?2026l\"); Start-Sleep -Milliseconds 100".into(),
            ],
            ..PtyOptions::default()
        },
        Box::new(move |event| { let _ = send.send(event); }),
    ).expect("spawn console host");
    let mut output = String::new();
    loop {
        let timeout = if output.contains("QS-SYNC-PROBE") {
            Duration::from_millis(200)
        } else {
            Duration::from_secs(10)
        };
        match receive.recv_timeout(timeout) {
            Ok(PtyEvent::Output(text)) => {
                if text.contains("\x1b[c") {
                    session
                        .writer()
                        .lock()
                        .unwrap()
                        .write_all(b"\x1b[?1;2c")
                        .expect("terminal capabilities");
                }
                if text.contains("\x1b[6n") {
                    session
                        .writer()
                        .lock()
                        .unwrap()
                        .write_all(b"\x1b[1;1R")
                        .expect("initial cursor");
                }
                output.push_str(&text);
            }
            Ok(PtyEvent::Eof) => break,
            Ok(PtyEvent::Dropped { bytes }) => panic!("dropped {bytes} bytes"),
            Err(_) => break,
        }
    }
    let _ = session.kill();
    let begin = output
        .find("\x1b[?2026h")
        .unwrap_or_else(|| panic!("sync start missing: {output:?}"));
    let text = output.find("QS-SYNC-PROBE").expect("redraw text");
    let end = output[begin..]
        .find("\x1b[?2026l")
        .map(|i| begin + i)
        .expect("sync end");
    assert!(
        begin < text && text < end,
        "ConPTY reordered the redraw: {output:?}"
    );
    let cursor = output[text..end].find("\x1b[12;4H");
    assert!(
        cursor.is_some(),
        "input cursor was not restored inside the frame: {output:?}"
    );
}
