//! The download engine.
//!
//! Spawns yt-dlp as a child process and parses its machine-oriented stdout.
//! Per yt-dlp's docs, programs should NOT parse the default human output;
//! instead we use the documented machine interfaces:
//!
//! - `--newline --progress --progress-template "download:YTDLP|..."` emits
//!   one `YTDLP|`-prefixed line per progress update.
//! - `--print after_move:filepath` prints the final filename after all
//!   post-processing (merging) is complete.
//!
//! Process IO runs on dedicated threads (stdout reader, stderr collector)
//! while a supervisor polls for exit; this keeps pipe buffers draining so
//! the child never blocks, and lets `cancel` kill the process at any time.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::models::{DownloadRequest, ProgressEvent, Resolution};
use crate::ytdlp::{validate_url, Binaries};

/// Tauri event the frontend listens on for all download updates.
pub const PROGRESS_EVENT: &str = "download-progress";

static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);

struct JobControl {
    child: Mutex<Option<Child>>,
    cancelled: AtomicBool,
}

/// Tracks running download jobs. v1 enforces a single active download.
#[derive(Default)]
pub struct DownloadManager {
    jobs: Arc<Mutex<HashMap<String, Arc<JobControl>>>>,
}

impl DownloadManager {
    /// Spawn yt-dlp for `request`, returning the new job id immediately.
    /// Progress events are delivered to `emit` (the command layer wraps
    /// this in a Tauri event emit; tests can collect them directly).
    pub fn start(
        &self,
        request: DownloadRequest,
        emit: impl Fn(ProgressEvent) + Send + Sync + 'static,
    ) -> Result<String, String> {
        if !self.jobs.lock().unwrap().is_empty() {
            return Err("A download is already in progress.".to_string());
        }
        validate_url(request.url.trim())?;

        let bins = Binaries::resolve()?;
        let args = build_args(&bins, &request);

        let child = Command::new(&bins.ytdlp)
            .args(&args)
            .env("PATH", bins.child_path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to spawn yt-dlp: {e}"))?;

        let job_id = format!("job-{}", JOB_COUNTER.fetch_add(1, Ordering::Relaxed));
        let control = Arc::new(JobControl {
            child: Mutex::new(Some(child)),
            cancelled: AtomicBool::new(false),
        });
        self.jobs
            .lock()
            .unwrap()
            .insert(job_id.clone(), control.clone());

        let jobs = self.jobs.clone();
        let job_id_for_task = job_id.clone();
        std::thread::spawn(move || supervise(emit, job_id_for_task, control, jobs));

        Ok(job_id)
    }

    /// Cancel a running job by killing its yt-dlp process.
    pub fn cancel(&self, job_id: &str) -> Result<(), String> {
        let jobs = self.jobs.lock().unwrap();
        let control = jobs
            .get(job_id)
            .ok_or_else(|| "No such download (it may have already finished).".to_string())?;
        control.cancelled.store(true, Ordering::SeqCst);
        let mut guard = control.child.lock().unwrap();
        if let Some(child) = guard.as_mut() {
            child
                .kill()
                .map_err(|e| format!("failed to stop download: {e}"))?;
        }
        Ok(())
    }
}

/// Read the child pipes, wait for exit, emit the terminal event, and free
/// the job slot. Runs on its own thread for the lifetime of one download.
fn supervise(
    emit: impl Fn(ProgressEvent) + Send + Sync + 'static,
    job_id: String,
    control: Arc<JobControl>,
    jobs: Arc<Mutex<HashMap<String, Arc<JobControl>>>>,
) {
    let (stdout, stderr) = {
        let mut guard = control.child.lock().unwrap();
        let child = guard.as_mut().expect("child present at job start");
        (child.stdout.take(), child.stderr.take())
    };

    // stderr collector: kept for error reporting.
    let stderr_buf = Arc::new(Mutex::new(String::new()));
    let stderr_handle = {
        let buf = stderr_buf.clone();
        std::thread::spawn(move || {
            if let Some(stderr) = stderr {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let mut b = buf.lock().unwrap();
                    b.push_str(&line);
                    b.push('\n');
                }
            }
        })
    };

    // stdout parser: progress lines + the final after_move filepath line.
    let emit = Arc::new(emit);
    let stdout_handle = {
        let job_id = job_id.clone();
        let emit = emit.clone();
        std::thread::spawn(move || {
            let mut filepath: Option<String> = None;
            if let Some(stdout) = stdout {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if let Some(rest) = line.strip_prefix("YTDLP|") {
                        if let Some(event) = parse_progress_line(&job_id, rest) {
                            emit(event);
                        }
                    } else if !line.trim().is_empty() {
                        // --print after_move:filepath output
                        filepath = Some(line.trim().to_string());
                    }
                }
            }
            filepath
        })
    };

    // Wait for process exit (poll so `cancel` can lock the child mutex).
    let status: Option<ExitStatus> = loop {
        {
            let mut guard = control.child.lock().unwrap();
            if let Some(child) = guard.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => break Some(status),
                    Ok(None) => {}
                    Err(_) => break None,
                }
            } else {
                break None;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    };

    let filepath = stdout_handle.join().unwrap_or(None);
    stderr_handle.join().ok();
    jobs.lock().unwrap().remove(&job_id);

    let terminal = if control.cancelled.load(Ordering::SeqCst) {
        ProgressEvent::terminal(&job_id, "cancelled", None, None)
    } else if status.map(|s| s.success()).unwrap_or(false) {
        ProgressEvent::terminal(&job_id, "completed", filepath, None)
    } else {
        let stderr = stderr_buf.lock().unwrap().clone();
        ProgressEvent::terminal(&job_id, "error", None, Some(stderr_tail(&stderr)))
    };
    emit(terminal);
}

impl ProgressEvent {
    fn terminal(
        job_id: &str,
        stage: &str,
        filepath: Option<String>,
        error: Option<String>,
    ) -> Self {
        Self {
            job_id: job_id.to_string(),
            stage: stage.to_string(),
            percent: None,
            speed: None,
            eta: None,
            downloaded_bytes: None,
            total_bytes: None,
            filepath,
            error,
        }
    }
}

/// Parse one `YTDLP|`-prefixed progress-template line.
fn parse_progress_line(job_id: &str, rest: &str) -> Option<ProgressEvent> {
    let parts: Vec<&str> = rest.split('|').collect();
    if parts.len() != 6 {
        return None;
    }
    let status = parts[0].trim();
    Some(ProgressEvent {
        job_id: job_id.to_string(),
        // Per-stream "finished" is not job completion; rename so the
        // frontend can distinguish it from the terminal `completed` stage.
        stage: if status == "finished" {
            "stream-finished".to_string()
        } else {
            status.to_string()
        },
        percent: opt_str(parts[3]),
        speed: opt_str(parts[4]),
        eta: opt_str(parts[5]),
        downloaded_bytes: opt_u64(parts[1]),
        total_bytes: opt_u64(parts[2]),
        filepath: None,
        error: None,
    })
}

fn opt_str(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() || t == "NA" {
        None
    } else {
        Some(t.to_string())
    }
}

fn opt_u64(s: &str) -> Option<u64> {
    let t = s.trim();
    if t.is_empty() || t == "NA" {
        None
    } else {
        t.parse().ok()
    }
}

fn stderr_tail(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(6);
    lines[start..].join("\n")
}

/// Build the yt-dlp argument list for a download request.
/// Every flag here is from yt-dlp's documented interface.
fn build_args(bins: &Binaries, request: &DownloadRequest) -> Vec<String> {
    let output_dir = request
        .output_dir
        .clone()
        .or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/Downloads")))
        .unwrap_or_else(|| ".".to_string());

    let mut args = vec![
        "--no-playlist".to_string(),
        "--newline".to_string(),
        // --print implies --quiet, which suppresses progress; re-enable it.
        "--progress".to_string(),
        "--progress-delta".to_string(),
        "0.5".to_string(),
        "--progress-template".to_string(),
        "download:YTDLP|%(progress.status)s|%(progress.downloaded_bytes)s|\
         %(progress.total_bytes,progress.total_bytes_estimate)s|\
         %(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s"
            .to_string(),
        "--print".to_string(),
        "after_move:filepath".to_string(),
        "--no-simulate".to_string(),
        "--no-warnings".to_string(),
        "-P".to_string(),
        output_dir,
        "-o".to_string(),
        "%(title)s.%(ext)s".to_string(),
    ];

    if let Some(dir) = &bins.ffmpeg_dir {
        args.push("--ffmpeg-location".to_string());
        args.push(dir.to_string_lossy().into_owned());
    }

    match &request.resolution {
        Resolution::Best | Resolution::Height { .. } => {
            if let Resolution::Height { height } = &request.resolution {
                args.push("-S".to_string());
                args.push(format!("res:{height}"));
            }
            args.push("--merge-output-format".to_string());
            args.push("mp4".to_string());
        }
        Resolution::AudioMp3 => {
            // Documented preset alias: best audio, extracted to mp3.
            args.push("-t".to_string());
            args.push("mp3".to_string());
        }
    }

    args.push(request.url.trim().to_string());
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_bins() -> Binaries {
        Binaries {
            ytdlp: PathBuf::from("/opt/homebrew/bin/yt-dlp"),
            ffmpeg_dir: Some(PathBuf::from("/opt/homebrew/bin")),
            deno_dir: Some(PathBuf::from("/opt/homebrew/bin")),
        }
    }

    fn request(resolution: Resolution) -> DownloadRequest {
        DownloadRequest {
            url: "https://www.youtube.com/watch?v=jNQXAC9IVRw".to_string(),
            resolution,
            output_dir: Some("/tmp/forvid".to_string()),
        }
    }

    #[test]
    fn height_maps_to_res_sort() {
        let args = build_args(&test_bins(), &request(Resolution::Height { height: 1080 }));
        let pos = args.iter().position(|a| a == "-S").expect("-S present");
        assert_eq!(args[pos + 1], "res:1080");
        assert!(args.contains(&"mp4".to_string()));
        // URL is always last, always a separate argv entry
        assert_eq!(
            args.last().unwrap(),
            "https://www.youtube.com/watch?v=jNQXAC9IVRw"
        );
    }

    #[test]
    fn best_uses_default_format_selection() {
        let args = build_args(&test_bins(), &request(Resolution::Best));
        assert!(!args.contains(&"-S".to_string()));
        assert!(args.contains(&"mp4".to_string()));
    }

    #[test]
    fn audio_maps_to_mp3_preset() {
        let args = build_args(&test_bins(), &request(Resolution::AudioMp3));
        let pos = args.iter().position(|a| a == "-t").expect("-t present");
        assert_eq!(args[pos + 1], "mp3");
        assert!(!args.contains(&"--merge-output-format".to_string()));
    }

    #[test]
    fn ffmpeg_location_is_passed() {
        let args = build_args(&test_bins(), &request(Resolution::Best));
        let pos = args
            .iter()
            .position(|a| a == "--ffmpeg-location")
            .expect("ffmpeg-location present");
        assert_eq!(args[pos + 1], "/opt/homebrew/bin");
    }

    #[test]
    fn parses_progress_lines() {
        let ev = parse_progress_line(
            "job-1",
            "downloading|1024|223779|  0.5%|  64.91KiB/s|00:03",
        )
        .expect("valid line");
        assert_eq!(ev.stage, "downloading");
        assert_eq!(ev.downloaded_bytes, Some(1024));
        assert_eq!(ev.total_bytes, Some(223779));
        assert_eq!(ev.percent.as_deref(), Some("0.5%"));
        assert_eq!(ev.speed.as_deref(), Some("64.91KiB/s"));
        assert_eq!(ev.eta.as_deref(), Some("00:03"));
    }

    #[test]
    fn stream_finished_is_not_completion() {
        let ev = parse_progress_line("job-1", "finished|1|1|100.0%|NA|NA").unwrap();
        assert_eq!(ev.stage, "stream-finished");
        assert_eq!(ev.speed, None);
        assert_eq!(ev.eta, None);
    }

    #[test]
    fn rejects_malformed_lines() {
        assert!(parse_progress_line("job-1", "downloading|1|2").is_none());
    }

    /// Full end-to-end gate: downloads a real video through the engine,
    /// exercising spawn, progress parsing, ffmpeg merge, and the final
    /// after_move filepath. Ignored by default; run with:
    /// cargo test -- --ignored
    #[test]
    #[ignore = "hits the network and downloads a file; run explicitly"]
    fn downloads_a_real_video_end_to_end() {
        use std::sync::mpsc;

        let dir = std::env::temp_dir().join("forvid-e2e");
        std::fs::create_dir_all(&dir).unwrap();

        let manager = DownloadManager::default();
        let (tx, rx) = mpsc::channel::<ProgressEvent>();
        let job_id = manager
            .start(
                DownloadRequest {
                    url: "https://www.youtube.com/watch?v=jNQXAC9IVRw".to_string(),
                    resolution: Resolution::Height { height: 240 },
                    output_dir: Some(dir.to_string_lossy().into_owned()),
                },
                move |e| {
                    tx.send(e).ok();
                },
            )
            .expect("download must start");
        assert!(job_id.starts_with("job-"));

        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        let terminal = loop {
            let ev = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("events must keep arriving");
            if matches!(ev.stage.as_str(), "completed" | "cancelled" | "error") {
                break ev;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "download exceeded 120s deadline"
            );
        };

        assert_eq!(
            terminal.stage, "completed",
            "download failed: {:?}",
            terminal.error
        );
        let path = terminal.filepath.expect("completed download has a filepath");
        assert!(
            std::path::Path::new(&path).is_file(),
            "merged file must exist on disk: {path}"
        );
    }
}
