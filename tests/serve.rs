//! Runs the real `stb-web serve` process.

#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn serve_exits_cleanly_on_sigterm() {
    let database_path =
        std::env::temp_dir().join(format!("stb-web-serve-test-{}.db", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_stb-web"))
        .arg("serve")
        .env("STB_BIND", "127.0.0.1:0")
        .env("STB_DATABASE_PATH", &database_path)
        .env("STB_BASE_URL", "http://localhost")
        .env("STB_WHATSAPP_NUMBER", "6281234567890")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start stb-web");

    // Wait until the server says it is listening (EOF means it exited early).
    let stdout = child.stdout.take().unwrap();
    let listening = BufReader::new(stdout)
        .lines()
        .map_while(Result::ok)
        .any(|line| line.contains("listening"));
    assert!(listening, "server did not report that it is listening");

    let status = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("run kill");
    assert!(status.success());

    let deadline = Instant::now() + Duration::from_secs(10);
    let exit = loop {
        if let Some(exit) = child.try_wait().unwrap() {
            break exit;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!("server did not stop within 10 s of SIGTERM");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(exit.success(), "expected a clean exit, got {exit:?}");
}
