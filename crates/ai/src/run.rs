//! Runs a provider: the prompt on stdin, the answer from stdout, in an empty folder, with a time
//! limit and a way to cancel. Output is bounded so a runaway program can't fill memory.

use crate::{AiError, Invocation};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The most of a provider's stdout kept, in bytes.
const MAX_STDOUT: usize = 64 * 1024;

/// The most of a provider's stderr kept, in bytes (its end, where the error usually is).
const MAX_STDERR: usize = 2 * 1024;

/// The longest error excerpt shown to the user, in characters.
const MAX_ERROR: usize = 300;

/// How often the runner checks for exit, timeout and cancel.
const POLL: Duration = Duration::from_millis(50);

/// Runs `inv` with `stdin` as its input and returns what it printed (at most 64 KiB).
///
/// Errors: [`AiError::NotFound`] when the program isn't installed, [`AiError::Timeout`] after
/// `timeout`, [`AiError::Cancelled`] once `cancel` is set, and [`AiError::Failed`] with the end of
/// its stderr when it exits with an error.
pub fn run(inv: &Invocation, stdin: &str, timeout: Duration, cancel: &AtomicBool) -> Result<String, AiError> {
    let dir = WorkDir::new()?;
    let mut child = spawn(inv, &dir.0)?;
    let feed = child.stdin.take().map(|mut pipe| {
        let input = stdin.to_owned();
        // A provider that exits without reading its input closes the pipe: not an error here.
        std::thread::spawn(move || {
            let _ = pipe.write_all(input.as_bytes());
        })
    });
    let out = child.stdout.take().map(|pipe| reader(pipe, MAX_STDOUT, false));
    let err = child.stderr.take().map(|pipe| reader(pipe, MAX_STDERR, true));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if cancel.load(Ordering::Relaxed) {
            stop(&mut child);
            return Err(AiError::Cancelled);
        }
        if started.elapsed() >= timeout {
            stop(&mut child);
            return Err(AiError::Timeout(timeout.as_secs()));
        }
        std::thread::sleep(POLL);
    };
    if let Some(feed) = feed {
        let _ = feed.join();
    }
    let stdout = collect(out)?;
    let stderr = collect(err)?;
    if !status.success() {
        return Err(AiError::Failed { program: inv.program.clone(), code: status.code(), stderr_tail: excerpt(&stderr) });
    }
    let mut text = String::from_utf8_lossy(&stdout).into_owned();
    while text.len() > MAX_STDOUT {
        text.pop();
    }
    Ok(text)
}

/// Starts `inv` in `dir`. On Windows a bare `claude` or `codex` is often an npm `.cmd` shim, which
/// `Command` doesn't find by itself, so a name without a path or extension is tried as `.cmd` too.
fn spawn(inv: &Invocation, dir: &std::path::Path) -> Result<Child, AiError> {
    match command(&inv.program, &inv.args, dir).spawn() {
        Ok(child) => Ok(child),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && cfg!(windows) && !inv.program.contains(['\\', '/', '.']) => {
            command(&format!("{}.cmd", inv.program), &inv.args, dir).spawn().map_err(|e| spawn_error(&inv.program, e))
        }
        Err(e) => Err(spawn_error(&inv.program, e)),
    }
}

fn command(program: &str, args: &[String], dir: &std::path::Path) -> Command {
    let mut c = Command::new(program);
    c.args(args).current_dir(dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console window flashes up for a console program.
        c.creation_flags(0x0800_0000);
    }
    c
}

fn spawn_error(program: &str, e: std::io::Error) -> AiError {
    match e.kind() {
        std::io::ErrorKind::NotFound => AiError::NotFound(program.to_owned()),
        // Rust refuses arguments a `.cmd` or `.bat` file can't receive safely.
        std::io::ErrorKind::InvalidInput => AiError::BadCommand(e.to_string()),
        _ => AiError::Io(e),
    }
}

/// Kills a provider that is being abandoned. Its output readers are left to finish on their own:
/// a program it started may still hold the pipes open.
fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Reads `pipe` to its end on a thread, keeping at most `cap` bytes: the first ones, or with
/// `keep_end` the last ones. The rest is read and dropped so the program never blocks on output.
fn reader(mut pipe: impl Read + Send + 'static, cap: usize, keep_end: bool) -> JoinHandle<std::io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let (mut kept, mut chunk) = (Vec::new(), [0u8; 8192]);
        loop {
            let n = pipe.read(&mut chunk)?;
            let Some(read) = chunk.get(..n).filter(|r| !r.is_empty()) else { return Ok(kept) };
            if keep_end {
                kept.extend_from_slice(read);
                let excess = kept.len().saturating_sub(cap);
                kept.drain(..excess);
            } else {
                let room = cap.saturating_sub(kept.len());
                kept.extend_from_slice(read.get(..room.min(n)).unwrap_or_default());
            }
        }
    })
}

fn collect(handle: Option<JoinHandle<std::io::Result<Vec<u8>>>>) -> Result<Vec<u8>, AiError> {
    match handle.map(JoinHandle::join) {
        None => Ok(Vec::new()),
        Some(Ok(read)) => Ok(read?),
        Some(Err(_)) => Err(AiError::Io(std::io::Error::other("reading the provider's output failed"))),
    }
}

/// The last non-empty lines of `stderr`, at most [`MAX_ERROR`] characters.
fn excerpt(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let tail = lines.get(lines.len().saturating_sub(2)..).unwrap_or_default().join(" ");
    let skip = tail.chars().count().saturating_sub(MAX_ERROR);
    let tail: String = tail.chars().skip(skip).collect();
    if tail.is_empty() { "no error message".to_owned() } else { tail }
}

/// An empty folder for the provider to run in, removed afterwards.
struct WorkDir(PathBuf);

impl WorkDir {
    fn new() -> std::io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
        let dir = std::env::temp_dir().join(format!("pdfcraft-ai-{}-{n}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
