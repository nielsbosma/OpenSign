//! Parsing a hostile PDF can take all the memory or time there is, and a parser bug can bring down
//! whatever runs it. So once `enable` has been called, the functions here run in a child process:
//! the server binary again, with `WORKER_SUBCOMMAND`. It gets no environment, so it holds no
//! secrets, and has a memory limit and a deadline. If it dies, one request fails.
//!
//! Without `enable` (tests, libraries) they run in this process. Crate users only ever get these
//! versions, so no caller can skip the worker by accident. Sealing's later steps (evidence page,
//! signature field, validation data) stay in the server: they only read what came out of here.

use std::borrow::Cow;
use std::io::{BufWriter, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Condvar, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::detect::{DetectedField, PageText};
use crate::hidden::HiddenText;
use crate::sanitize::{Limits, Sanitized};
use crate::stamp::Stamp;
use crate::tags::{TagScan, TextTag};
use crate::verify::SignatureCheck;
use crate::{PdfError, Result};

pub const WORKER_SUBCOMMAND: &str = "pdf-worker";

const TIMEOUT: Duration = Duration::from_secs(60);
const QUEUE_WAIT: Duration = Duration::from_secs(30);
const MAX_MESSAGE_BYTES: u64 = 1024 * 1024 * 1024;
const STACK_BYTES: usize = 64 * 1024 * 1024;
#[cfg(unix)]
const MEMORY_BYTES: u64 = 2 * 1024 * 1024 * 1024;
#[cfg(unix)]
const CPU_SECONDS: u64 = 120;

const FAILED: &str = "This PDF could not be processed";
const BUSY: &str = "The server is busy. Try again in a moment.";

/// A PDF on the wire, as base64 inside the JSON message.
struct Blob<'a>(Cow<'a, [u8]>);

impl<'a> Blob<'a> {
    fn of(bytes: &'a [u8]) -> Self {
        Self(Cow::Borrowed(bytes))
    }
}

impl Serialize for Blob<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(&self.0))
    }
}

impl<'de, 'a> Deserialize<'de> for Blob<'a> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = Cow::<str>::deserialize(deserializer)?;
        STANDARD.decode(text.as_bytes()).map(|bytes| Self(Cow::Owned(bytes))).map_err(serde::de::Error::custom)
    }
}

/// What the integration test and the start-up check ask of a worker.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Probe {
    Ping,
    Abort,
    Spin,
    Allocate,
}

#[derive(Serialize, Deserialize)]
enum Request<'a> {
    Probe(Probe),
    Sanitize(Blob<'a>, Limits),
    Verify(Blob<'a>),
    ExtractText(Blob<'a>),
    HiddenText(Blob<'a>),
    PageSizes(Blob<'a>),
    TextTags(Blob<'a>),
    DetectFields(Blob<'a>),
    PageText(Blob<'a>),
    Merge(Vec<Blob<'a>>),
    Stamp(Blob<'a>, Vec<Stamp>),
}

struct Worker {
    exe: PathBuf,
    running: Mutex<usize>,
    freed: Condvar,
    max_running: usize,
}

static WORKER: OnceLock<Worker> = OnceLock::new();

/// Sends every later call to a child process running `exe` and checks that it answers.
pub fn enable_worker(exe: PathBuf) -> Result<()> {
    let max_running = std::thread::available_parallelism().map_or(2, |n| n.get().max(2));
    WORKER.get_or_init(|| Worker { exe, running: Mutex::new(0), freed: Condvar::new(), max_running });
    probe(Probe::Ping, Duration::from_secs(10))
}

#[doc(hidden)]
pub fn probe(probe: Probe, timeout: Duration) -> Result<()> {
    let worker = WORKER.get().ok_or_else(|| PdfError::Rejected(FAILED.into()))?;
    worker.call(&Request::Probe(probe), timeout)
}

struct Slot<'a>(&'a Worker);

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        let mut running = self.0.running.lock().unwrap_or_else(|e| e.into_inner());
        *running -= 1;
        self.0.freed.notify_one();
    }
}

/// Kills the child however the call ends.
struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Worker {
    /// So that many hostile uploads at once cannot add up to more memory than the machine has.
    fn slot(&self) -> Result<Slot<'_>> {
        let running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        let (mut running, _) =
            self.freed.wait_timeout_while(running, QUEUE_WAIT, |running| *running >= self.max_running).unwrap_or_else(|e| e.into_inner());
        if *running >= self.max_running {
            return Err(PdfError::Rejected(BUSY.into()));
        }
        *running += 1;
        Ok(Slot(self))
    }

    fn call<T: DeserializeOwned>(&self, request: &Request, timeout: Duration) -> Result<T> {
        let _slot = self.slot()?;
        let failed = |reason: &str| {
            tracing::warn!(reason, "a PDF worker failed");
            PdfError::Rejected(FAILED.into())
        };
        let mut child = Command::new(&self.exe)
            .arg(WORKER_SUBCOMMAND)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map(Running)
            .map_err(|_| failed("it could not start"))?;
        let (Some(stdin), Some(stdout)) = (child.0.stdin.take(), child.0.stdout.take()) else { return Err(failed("no pipes")) };
        let deadline = Instant::now() + timeout;

        let answer = std::thread::scope(|scope| {
            scope.spawn(move || {
                let mut stdin = BufWriter::new(stdin);
                let _ = serde_json::to_writer(&mut stdin, request);
                let _ = stdin.flush();
            });
            let (send, receive) = mpsc::channel();
            scope.spawn(move || {
                let mut answer = Vec::new();
                let read = stdout.take(MAX_MESSAGE_BYTES + 1).read_to_end(&mut answer);
                let _ = send.send(read.map(|_| answer));
            });
            let answer = receive.recv_timeout(timeout);
            // Unblocks the threads above when it is still running.
            if answer.is_err() {
                let _ = child.0.kill();
            }
            answer
        });
        let answer = match answer {
            Ok(Ok(answer)) if answer.len() as u64 <= MAX_MESSAGE_BYTES => answer,
            Ok(_) => return Err(failed("its answer could not be read")),
            Err(_) => return Err(failed("it took too long")),
        };
        loop {
            match child.0.try_wait() {
                Ok(Some(status)) if status.success() => break,
                Ok(Some(_)) | Err(_) => return Err(failed("it stopped early")),
                Ok(None) if Instant::now() >= deadline => return Err(failed("it took too long")),
                Ok(None) => std::thread::sleep(Duration::from_millis(2)),
            }
        }
        serde_json::from_slice::<Result<T, PdfError>>(&answer).map_err(|_| failed("its answer could not be parsed"))?
    }
}

fn worker() -> Option<&'static Worker> {
    WORKER.get()
}

pub fn sanitize(input: &[u8], limits: Limits) -> Result<Sanitized> {
    let Some(worker) = worker() else { return crate::sanitize::sanitize(input, limits) };
    let (pdf, clean): (Blob, Sanitized) = worker.call(&Request::Sanitize(Blob::of(input), limits), TIMEOUT)?;
    Ok(Sanitized { pdf: pdf.0.into_owned(), ..clean })
}

/// Checks every signature and document time stamp in the file, in the order they were added.
pub fn verify_signatures(pdf: &[u8]) -> Result<Vec<SignatureCheck>> {
    let Some(worker) = worker() else { return crate::verify::verify_signatures(pdf) };
    worker.call(&Request::Verify(Blob::of(pdf)), TIMEOUT)
}

/// The text of each page, in order, for review by a language model. Best effort: PDFs drawn as
/// images or with unusual font encodings give little or nothing.
pub fn extract_text(pdf: &[u8]) -> Result<Vec<String>> {
    let Some(worker) = worker() else { return crate::geometry::extract_text(pdf) };
    worker.call(&Request::ExtractText(Blob::of(pdf)), TIMEOUT)
}

/// The pages that draw text a reader would not see. Pages whose content cannot be read are skipped.
pub fn hidden_text(pdf: &[u8]) -> Result<Vec<HiddenText>> {
    let Some(worker) = worker() else { return crate::hidden::hidden_text(pdf) };
    worker.call(&Request::HiddenText(Blob::of(pdf)), TIMEOUT)
}

/// Displayed width and height of each page, in order.
pub fn page_sizes(pdf: &[u8]) -> Result<Vec<(f64, f64)>> {
    let Some(worker) = worker() else { return crate::geometry::page_sizes(pdf) };
    worker.call(&Request::PageSizes(Blob::of(pdf)), TIMEOUT)
}

/// Finds DocuSeal-style `{{Name;type=…}}` tags and removes their text from the page content.
pub fn take_text_tags(pdf: &[u8]) -> Result<TagScan> {
    let Some(worker) = worker() else { return crate::tags::take_text_tags(pdf) };
    let (pdf, tags, problems): (Blob, Vec<TextTag>, Vec<String>) = worker.call(&Request::TextTags(Blob::of(pdf)), TIMEOUT)?;
    Ok(TagScan { pdf: pdf.0.into_owned(), tags, problems })
}

/// Places on the pages that look like they want a field.
pub fn detect_fields(pdf: &[u8]) -> Result<Vec<DetectedField>> {
    let Some(worker) = worker() else { return crate::detect::detect_fields(pdf) };
    worker.call(&Request::DetectFields(Blob::of(pdf)), TIMEOUT)
}

/// The lines of text on each page, with their positions.
pub fn page_text(pdf: &[u8]) -> Result<Vec<PageText>> {
    let Some(worker) = worker() else { return crate::detect::page_text(pdf) };
    worker.call(&Request::PageText(Blob::of(pdf)), TIMEOUT)
}

/// Concatenates sanitized PDFs into one, in order.
pub fn merge(files: &[Vec<u8>]) -> Result<Vec<u8>> {
    let Some(worker) = worker() else { return crate::merge::merge(files) };
    let merged: Blob = worker.call(&Request::Merge(files.iter().map(|f| Blob::of(f)).collect()), TIMEOUT)?;
    Ok(merged.0.into_owned())
}

/// Draws the stamps on the pages.
pub fn stamp(pdf: &[u8], stamps: &[Stamp]) -> Result<Vec<u8>> {
    let Some(worker) = worker() else { return crate::stamp::stamp(pdf, stamps) };
    let stamped: Blob = worker.call(&Request::Stamp(Blob::of(pdf), stamps.to_vec()), TIMEOUT)?;
    Ok(stamped.0.into_owned())
}

/// The child's main function. Reads one request from stdin, writes one answer to stdout and exits.
pub fn worker_main() -> ! {
    #[cfg(unix)]
    limit_resources();
    std::panic::set_hook(Box::new(|_| {}));
    // Deeply nested objects recurse; on the main thread's small stack they would fail on files
    // that are merely unusual.
    let served = std::thread::Builder::new().stack_size(STACK_BYTES).spawn(serve).map(std::thread::JoinHandle::join);
    std::process::exit(if matches!(served, Ok(Ok(Ok(())))) { 0 } else { 1 })
}

#[cfg(unix)]
fn limit_resources() {
    use rlimit::Resource;
    let limits =
        [(Resource::AS, MEMORY_BYTES), (Resource::CPU, CPU_SECONDS), (Resource::CORE, 0), (Resource::FSIZE, 0), (Resource::NOFILE, 16)];
    for (resource, limit) in limits {
        let hard = resource.get().map_or(limit, |(_, hard)| hard.min(limit));
        // Better no answer than an answer from a worker without its limits.
        if resource.set(hard, hard).is_err() {
            std::process::exit(1);
        }
    }
}

fn serve() -> std::io::Result<()> {
    let mut input = Vec::new();
    std::io::stdin().lock().take(MAX_MESSAGE_BYTES).read_to_end(&mut input)?;
    let request: Request = serde_json::from_slice(&input)?;
    drop(input);
    let mut out = BufWriter::new(std::io::stdout().lock());
    let mut reply = |answer: &dyn erased::Answer| answer.write(&mut out);
    match request {
        Request::Probe(probe) => {
            run_probe(probe);
            reply(&Ok::<(), PdfError>(()))
        }
        Request::Sanitize(pdf, limits) => reply(&crate::sanitize::sanitize(&pdf.0, limits).map(|mut clean| {
            let pdf = std::mem::take(&mut clean.pdf);
            (Blob(Cow::Owned(pdf)), clean)
        })),
        Request::Verify(pdf) => reply(&crate::verify::verify_signatures(&pdf.0)),
        Request::ExtractText(pdf) => reply(&crate::geometry::extract_text(&pdf.0)),
        Request::HiddenText(pdf) => reply(&crate::hidden::hidden_text(&pdf.0)),
        Request::PageSizes(pdf) => reply(&crate::geometry::page_sizes(&pdf.0)),
        Request::TextTags(pdf) => {
            reply(&crate::tags::take_text_tags(&pdf.0).map(|scan| (Blob(Cow::Owned(scan.pdf)), scan.tags, scan.problems)))
        }
        Request::DetectFields(pdf) => reply(&crate::detect::detect_fields(&pdf.0)),
        Request::PageText(pdf) => reply(&crate::detect::page_text(&pdf.0)),
        Request::Merge(files) => {
            let files: Vec<Vec<u8>> = files.into_iter().map(|f| f.0.into_owned()).collect();
            reply(&crate::merge::merge(&files).map(|merged| Blob(Cow::Owned(merged))))
        }
        Request::Stamp(pdf, stamps) => reply(&crate::stamp::stamp(&pdf.0, &stamps).map(|stamped| Blob(Cow::Owned(stamped)))),
    }?;
    out.flush()
}

fn run_probe(probe: Probe) {
    match probe {
        Probe::Ping => {}
        Probe::Abort => std::process::abort(),
        Probe::Spin => loop {
            std::hint::spin_loop();
        },
        Probe::Allocate => {
            let mut held = Vec::new();
            loop {
                held.push(vec![1u8; 64 * 1024 * 1024]);
                std::hint::black_box(&held);
            }
        }
    }
}

/// Lets `serve` send answers of different types through one writer.
mod erased {
    use std::io::Write;

    use serde::Serialize;

    pub trait Answer {
        fn write(&self, out: &mut dyn Write) -> std::io::Result<()>;
    }

    impl<T: Serialize> Answer for T {
        fn write(&self, out: &mut dyn Write) -> std::io::Result<()> {
            serde_json::to_writer(out, self).map_err(std::io::Error::other)
        }
    }
}
