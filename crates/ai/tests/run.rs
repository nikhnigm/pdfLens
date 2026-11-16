//! The process runner against stand-in providers built from the operating system's own tools, so
//! these tests never need Claude or Codex.
#![cfg(not(target_arch = "wasm32"))]

use pdfcraft_ai::{AiError, Invocation, run};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// A provider that runs `script` in the platform shell.
fn fake(script: &str) -> Invocation {
    if cfg!(windows) {
        Invocation { program: "cmd".into(), args: vec!["/C".into(), script.into()] }
    } else {
        Invocation { program: "sh".into(), args: vec!["-c".into(), script.into()] }
    }
}

fn sleeper() -> Invocation {
    if cfg!(windows) { fake("ping -n 30 127.0.0.1 >NUL") } else { fake("sleep 30") }
}

#[test]
fn stdin_reaches_the_provider_and_stdout_comes_back() {
    // `more` echoes stdin on Windows (no quotes needed: Rust re-quotes arguments for cmd).
    let inv = if cfg!(windows) { fake("more") } else { fake("cat") };
    assert_eq!(run(&inv, "hello", Duration::from_secs(10), &AtomicBool::new(false)).unwrap().trim(), "hello");
}

#[test]
fn missing_program_is_reported_by_name() {
    let inv = Invocation { program: "pdfcraft-no-such-tool".into(), args: vec![] };
    let e = run(&inv, "", Duration::from_secs(5), &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(&e, AiError::NotFound(p) if p.contains("pdfcraft-no-such-tool")), "{e:?}");
}

#[test]
fn nonzero_exit_reports_stderr_tail() {
    let inv = fake("echo please log in 1>&2 && exit 3");
    let e = run(&inv, "", Duration::from_secs(10), &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(&e, AiError::Failed { code: Some(3), stderr_tail, .. } if stderr_tail.contains("please log in")), "{e:?}");
}

#[test]
fn timeout_kills_the_process() {
    let t = Instant::now();
    let r = run(&sleeper(), "", Duration::from_secs(1), &AtomicBool::new(false));
    assert!(matches!(r, Err(AiError::Timeout(_))), "{r:?}");
    assert!(t.elapsed() < Duration::from_secs(10));
}

#[test]
fn cancel_stops_the_lookup() {
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        flag.store(true, Ordering::Relaxed);
    });
    let t = Instant::now();
    let r = run(&sleeper(), "", Duration::from_secs(30), &cancel);
    assert!(matches!(r, Err(AiError::Cancelled)), "{r:?}");
    assert!(t.elapsed() < Duration::from_secs(5));
}

#[test]
fn huge_output_is_capped() {
    // About 1 MB of output; only the first 64 KiB are kept, and the provider isn't blocked.
    let inv = if cfg!(windows) {
        fake("for /L %i in (1,1,20000) do @echo 0123456789012345678901234567890123456789012345678")
    } else {
        fake("yes 0123456789012345678901234567890123456789012345678 | head -n 20000")
    };
    let out = run(&inv, "", Duration::from_secs(60), &AtomicBool::new(false)).unwrap();
    assert!(!out.is_empty() && out.len() <= 65_536, "{}", out.len());
}

#[test]
fn define_uses_cache_on_second_call() {
    use pdfcraft_ai::{Cache, ProviderConfig, ProviderKind, Request, define};
    use std::sync::Mutex;
    let custom = if cfg!(windows) { "cmd /C echo Brief." } else { "sh -c \"echo Brief.\"" };
    let cfg = ProviderConfig { kind: ProviderKind::Custom, custom: custom.into(), ..ProviderConfig::default() };
    let req = Request { word: "ephemeral".into(), sentence: "It was ephemeral.".into(), language: "English".into() };
    let cache = Mutex::new(Cache::default());
    let first = define(&req, &cfg, Some(&cache), &AtomicBool::new(false)).unwrap();
    assert_eq!((first.text.as_str(), first.cached), ("Brief.", false));
    let second = define(&req, &cfg, Some(&cache), &AtomicBool::new(false)).unwrap();
    assert_eq!((second.text.as_str(), second.cached), ("Brief.", true));
    let uncached = define(&req, &cfg, None, &AtomicBool::new(false)).unwrap();
    assert!(!uncached.cached);
}
