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
use crate::ytdlp::{Binaries, validate_url};

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

    // stdout parser: progress lines + after_move filepath lines (one per
    // completed file, so playlists yield many).
    let emit = Arc::new(emit);
    let stdout_handle = {
        let job_id = job_id.clone();
        let emit = emit.clone();
        std::thread::spawn(move || {
            let mut filepaths: Vec<String> = Vec::new();
            let mut playlist_total: Option<u32> = None;
            if let Some(stdout) = stdout {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if let Some(rest) = line.strip_prefix("YTDLP|") {
                        if let Some(event) = parse_progress_line(&job_id, rest) {
                            if event.playlist_count.is_some() {
                                playlist_total = event.playlist_count;
                            }
                            emit(event);
                        }
                    } else if !line.trim().is_empty() {
                        // --print after_move:filepath output
                        filepaths.push(line.trim().to_string());
                    }
                }
            }
            (filepaths, playlist_total)
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

    let (filepaths, playlist_total) = stdout_handle.join().unwrap_or_default();
    stderr_handle.join().ok();
    jobs.lock().unwrap().remove(&job_id);

    let terminal = decide_terminal(
        &job_id,
        control.cancelled.load(Ordering::SeqCst),
        status.map(|s| s.success()).unwrap_or(false),
        filepaths,
        playlist_total,
        &stderr_buf.lock().unwrap(),
    );
    emit(terminal);
}

/// Decide the terminal event from how the process exited and what it
/// actually produced. Crucially, yt-dlp exits non-zero when ANY playlist
/// entry fails (private/deleted/region-locked videos are common) even
/// though it downloads everything else — that is a partial success, not
/// a failed download.
fn decide_terminal(
    job_id: &str,
    cancelled: bool,
    success: bool,
    filepaths: Vec<String>,
    playlist_total: Option<u32>,
    stderr: &str,
) -> ProgressEvent {
    let mut event = ProgressEvent::blank(job_id);
    if cancelled {
        event.stage = "cancelled".to_string();
        return event;
    }
    if success {
        event.stage = "completed".to_string();
        event.filepath = filepaths.last().cloned();
        return event;
    }
    if !filepaths.is_empty() {
        // Non-zero exit but files landed: partial success.
        event.stage = "completed".to_string();
        event.filepath = filepaths.last().cloned();
        event.partial_downloaded = Some(filepaths.len() as u32);
        event.partial_total = playlist_total;
        event.error = Some(stderr_tail(stderr));
        return event;
    }
    event.stage = "error".to_string();
    event.error = Some(stderr_tail(stderr));
    event
}

impl ProgressEvent {
    fn blank(job_id: &str) -> Self {
        Self {
            job_id: job_id.to_string(),
            stage: String::new(),
            percent: None,
            speed: None,
            eta: None,
            downloaded_bytes: None,
            total_bytes: None,
            filepath: None,
            error: None,
            playlist_index: None,
            playlist_count: None,
            partial_downloaded: None,
            partial_total: None,
        }
    }
}

/// Parse one `YTDLP|`-prefixed progress-template line.
fn parse_progress_line(job_id: &str, rest: &str) -> Option<ProgressEvent> {
    let parts: Vec<&str> = rest.split('|').collect();
    // 6 core fields, plus playlist_index/playlist_count when the template
    // emits them (always appended; NA for single videos).
    if parts.len() != 8 {
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
        playlist_index: opt_u32(parts[6]),
        playlist_count: opt_u32(parts[7]),
        partial_downloaded: None,
        partial_total: None,
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

fn opt_u32(s: &str) -> Option<u32> {
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
        .or_else(|| {
            std::env::home_dir().map(|h| h.join("Downloads").to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| ".".to_string());

    // Playlist jobs drop --no-playlist and save into a subfolder named
    // after the playlist (documented output-template field playlist_title).
    let output_template = if request.playlist {
        "%(playlist_title)s/%(title)s.%(ext)s"
    } else {
        "%(title)s.%(ext)s"
    };

    let mut args = vec![
        "--newline".to_string(),
        // --print implies --quiet, which suppresses progress; re-enable it.
        "--progress".to_string(),
        "--progress-delta".to_string(),
        "0.5".to_string(),
        "--progress-template".to_string(),
        "download:YTDLP|%(progress.status)s|%(progress.downloaded_bytes)s|\
         %(progress.total_bytes,progress.total_bytes_estimate)s|\
         %(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s|\
         %(info.playlist_index)s|%(info.n_entries)s"
            .to_string(),
        "--print".to_string(),
        "after_move:filepath".to_string(),
        "--no-simulate".to_string(),
        "--no-warnings".to_string(),
        "-P".to_string(),
        output_dir,
        "-o".to_string(),
        output_template.to_string(),
    ];

    if !request.playlist {
        args.push("--no-playlist".to_string());
    }

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
            playlist: false,
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
            "downloading|1024|223779|  0.5%|  64.91KiB/s|00:03|NA|NA",
        )
        .expect("valid line");
        assert_eq!(ev.stage, "downloading");
        assert_eq!(ev.downloaded_bytes, Some(1024));
        assert_eq!(ev.total_bytes, Some(223779));
        assert_eq!(ev.percent.as_deref(), Some("0.5%"));
        assert_eq!(ev.speed.as_deref(), Some("64.91KiB/s"));
        assert_eq!(ev.eta.as_deref(), Some("00:03"));
        assert_eq!(ev.playlist_index, None);
        assert_eq!(ev.playlist_count, None);
    }

    #[test]
    fn parses_playlist_progress() {
        let ev = parse_progress_line(
            "job-1",
            "downloading|1024|223779| 12.0%|  1.0MiB/s|00:09|3|12",
        )
        .expect("valid playlist line");
        assert_eq!(ev.playlist_index, Some(3));
        assert_eq!(ev.playlist_count, Some(12));
    }

    #[test]
    fn stream_finished_is_not_completion() {
        let ev = parse_progress_line("job-1", "finished|1|1|100.0%|NA|NA|NA|NA").unwrap();
        assert_eq!(ev.stage, "stream-finished");
        assert_eq!(ev.speed, None);
        assert_eq!(ev.eta, None);
    }

    #[test]
    fn rejects_malformed_lines() {
        assert!(parse_progress_line("job-1", "downloading|1|2").is_none());
    }

    #[test]
    fn playlist_mode_drops_no_playlist_and_uses_folder_template() {
        let mut req = request(Resolution::Height { height: 720 });
        req.playlist = true;
        let args = build_args(&test_bins(), &req);
        assert!(!args.contains(&"--no-playlist".to_string()));
        let pos = args.iter().position(|a| a == "-o").expect("-o present");
        assert_eq!(args[pos + 1], "%(playlist_title)s/%(title)s.%(ext)s");
    }

    #[test]
    fn single_mode_forces_no_playlist() {
        let args = build_args(&test_bins(), &request(Resolution::Best));
        assert!(args.contains(&"--no-playlist".to_string()));
        let pos = args.iter().position(|a| a == "-o").expect("-o present");
        assert_eq!(args[pos + 1], "%(title)s.%(ext)s");
    }

    #[test]
    fn clean_exit_is_completed() {
        let ev = decide_terminal("job-1", false, true, vec!["/tmp/a.mp4".into()], None, "");
        assert_eq!(ev.stage, "completed");
        assert_eq!(ev.filepath.as_deref(), Some("/tmp/a.mp4"));
        assert_eq!(ev.partial_downloaded, None);
    }

    #[test]
    fn cancelled_wins_over_everything() {
        let ev = decide_terminal("job-1", true, false, vec!["/tmp/a.mp4".into()], None, "");
        assert_eq!(ev.stage, "cancelled");
    }

    #[test]
    fn nonzero_exit_with_files_is_partial_success() {
        // The exact 403 scenario: yt-dlp skipped one playlist entry but
        // downloaded the rest, exiting 1. Not an error state.
        let files: Vec<String> = (1..=37).map(|i| format!("/tmp/{i}.mp4")).collect();
        let ev = decide_terminal(
            "job-1",
            false,
            false,
            files,
            Some(40),
            "ERROR: unable to download video data: HTTP Error 403: Forbidden",
        );
        assert_eq!(ev.stage, "completed");
        assert_eq!(ev.partial_downloaded, Some(37));
        assert_eq!(ev.partial_total, Some(40));
        assert!(ev.error.is_some(), "stderr tail kept as a note");
    }

    #[test]
    fn nonzero_exit_with_no_files_is_a_real_error() {
        let ev = decide_terminal("job-1", false, false, vec![], None, "ERROR: boom");
        assert_eq!(ev.stage, "error");
        assert_eq!(ev.error.as_deref(), Some("ERROR: boom"));
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
                    playlist: false,
                },
                move |e| {
                    tx.send(e).ok();
                },
            )
            .expect("download must start");
        assert!(job_id.starts_with("job-"));

        let deadline = std::time::Instant::now() + Duration::from_secs(180);
        let terminal = loop {
            let ev = rx
                // The managed yt-dlp is a PyInstaller binary with ~8s
                // startup, plus metadata extraction before first progress.
                .recv_timeout(Duration::from_secs(30))
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
        let path = terminal
            .filepath
            .expect("completed download has a filepath");
        assert!(
            std::path::Path::new(&path).is_file(),
            "merged file must exist on disk: {path}"
        );
    }
}
