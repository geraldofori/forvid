"use client";

import { useEffect, useRef, useState } from "react";
import {
  cancelDownload,
  fetchMetadata,
  onDownloadProgress,
  startDownload,
  type ProgressEvent,
  type Resolution,
  type VideoInfo,
} from "@/lib/tauri";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import ClipboardChip from "./ClipboardChip";
import MetadataCard from "./MetadataCard";

/** Best-effort native notification; silently skipped if permission is denied. */
async function notifyCompletion(title: string, body: string) {
  try {
    let granted = await isPermissionGranted();
    if (!granted) {
      granted = (await requestPermission()) === "granted";
    }
    if (granted) sendNotification({ title, body });
  } catch {
    // Notifications unavailable in this context.
  }
}

type JobState =
  | { phase: "idle" }
  | { phase: "downloading"; event: ProgressEvent }
  | { phase: "completed"; filepath: string }
  | { phase: "cancelled" }
  | { phase: "error"; message: string };

const TERMINAL_STAGES = new Set(["completed", "cancelled", "error"]);

export default function Downloader() {
  const [url, setUrl] = useState("");
  const [meta, setMeta] = useState<VideoInfo | null>(null);
  const [metaError, setMetaError] = useState<string | null>(null);
  const [metaLoading, setMetaLoading] = useState(false);
  const [resolution, setResolution] = useState("best");
  const [outputDir, setOutputDir] = useState<string | null>(null);
  const [job, setJob] = useState<JobState>({ phase: "idle" });

  const jobIdRef = useRef<string | null>(null);
  // Guards against out-of-order metadata responses when the URL changes fast.
  const metaSeq = useRef(0);
  // Mirror of `meta` for the progress listener (registered once, so it
  // would otherwise close over stale state).
  const metaRef = useRef<VideoInfo | null>(null);
  useEffect(() => {
    metaRef.current = meta;
  }, [meta]);

  // Single progress listener for the app's lifetime. The engine enforces
  // one active download at a time, so every event belongs to the current
  // job — no job-id filtering (which could drop events emitted before the
  // start_download invoke promise resolves).
  useEffect(() => {
    const unlisten = onDownloadProgress((event) => {
      if (TERMINAL_STAGES.has(event.stage)) {
        jobIdRef.current = null;
        if (event.stage === "completed") {
          setJob({ phase: "completed", filepath: event.filepath ?? "" });
          notifyCompletion(
            "Download complete",
            metaRef.current?.title ?? event.filepath ?? "",
          );
        } else if (event.stage === "cancelled") {
          setJob({ phase: "cancelled" });
        } else {
          setJob({ phase: "error", message: event.error ?? "Unknown error" });
        }
      } else {
        setJob({ phase: "downloading", event });
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  // Debounced metadata lookup once the input looks like an http(s) URL.
  useEffect(() => {
    setMeta(null);
    setMetaError(null);
    setResolution("best"); // heights differ per video
    const trimmed = url.trim();
    if (!/^https?:\/\/.+/.test(trimmed)) return;

    const seq = ++metaSeq.current;
    const timer = setTimeout(async () => {
      setMetaLoading(true);
      try {
        const info = await fetchMetadata(trimmed);
        if (seq === metaSeq.current) {
          setMeta(info);
          // Default to a medium quality, not the best: the largest height
          // at or under 720p, else the smallest available, else Best.
          // (availableHeights is sorted descending.)
          const heights = info.availableHeights;
          const medium =
            heights.find((h) => h <= 720) ?? heights[heights.length - 1];
          setResolution(medium ? String(medium) : "best");
        }
      } catch (e) {
        if (seq === metaSeq.current) setMetaError(String(e));
      } finally {
        if (seq === metaSeq.current) setMetaLoading(false);
      }
    }, 700);
    return () => clearTimeout(timer);
  }, [url]);

  const handleDownload = async () => {
    const trimmed = url.trim();
    if (!trimmed || job.phase === "downloading") return;
    const resolutionArg: Resolution =
      resolution === "best"
        ? { kind: "best" }
        : resolution === "audioMp3"
          ? { kind: "audioMp3" }
          : { kind: "height", height: Number(resolution) };
    try {
      const id = await startDownload(trimmed, resolutionArg, outputDir);
      jobIdRef.current = id;
      setJob({
        phase: "downloading",
        event: {
          jobId: id,
          stage: "downloading",
          percent: null,
          speed: null,
          eta: null,
          downloadedBytes: null,
          totalBytes: null,
          filepath: null,
          error: null,
        },
      });
    } catch (e) {
      setJob({ phase: "error", message: String(e) });
    }
  };

  const handleCancel = async () => {
    if (!jobIdRef.current) return;
    try {
      await cancelDownload(jobIdRef.current);
    } catch {
      // The terminal event from the engine settles the UI either way.
    }
  };

  const reset = () => {
    setJob({ phase: "idle" });
    setMeta(null);
    setMetaError(null);
    setUrl("");
  };

  const pickFolder = async () => {
    try {
      const selected = await openDialog({ directory: true, multiple: false });
      if (typeof selected === "string") setOutputDir(selected);
    } catch {
      // Dialog cancelled or unavailable; keep current directory.
    }
  };

  // "/Users/gerald/Downloads" -> "~/Downloads" for display
  const displayDir = outputDir
    ? outputDir.replace(/^\/Users\/[^/]+/, "~")
    : "~/Downloads";

  const downloading = job.phase === "downloading";

  return (
    <section className="downloader">
      <header className="brand">
        <h1>Forvid</h1>
        <p>Paste a link. Pick a quality. Get the video.</p>
      </header>

      {job.phase === "idle" && <ClipboardChip onUse={setUrl} />}

      <input
        className="url-input"
        value={url}
        onChange={(e) => setUrl(e.target.value)}
        placeholder="Paste a video link (https://…)"
        spellCheck={false}
        autoFocus
        disabled={downloading}
      />

      {metaLoading && <p className="hint">Fetching video info…</p>}
      {metaError && <p className="error-text">{metaError}</p>}
      {meta && (job.phase === "idle" || downloading) && (
        <MetadataCard meta={meta} />
      )}

      <label className="field">
        <span className="field-label">Quality</span>
        <select
          className="select"
          value={resolution}
          onChange={(e) => setResolution(e.target.value)}
          disabled={!meta || downloading}
        >
          <option value="best">Best available</option>
          {meta?.availableHeights.map((h) => (
            <option key={h} value={String(h)}>
              {h}p
            </option>
          ))}
          <option value="audioMp3">Audio only · MP3</option>
        </select>
      </label>

      <div className="save-to">
        <span className="field-label">Save to</span>
        <code className="save-path" title={outputDir ?? undefined}>
          {displayDir}
        </code>
        <button
          className="save-change"
          onClick={pickFolder}
          disabled={downloading}
        >
          Change
        </button>
      </div>

      {downloading ? (
        <button className="btn danger" onClick={handleCancel}>
          Cancel download
        </button>
      ) : (
        <button
          className="btn primary"
          onClick={handleDownload}
          disabled={!meta || metaLoading}
        >
          Download
        </button>
      )}

      {job.phase === "downloading" && <ProgressView event={job.event} />}

      {job.phase === "completed" && (
        <div className="result success">
          <p className="result-title">Download complete</p>
          <code className="filepath">{job.filepath}</code>
          <div className="result-actions">
            {job.filepath && (
              <button
                className="btn ghost"
                onClick={() => revealItemInDir(job.filepath).catch(() => {})}
              >
                Reveal in Finder
              </button>
            )}
            <button className="btn ghost" onClick={reset}>
              Download another
            </button>
          </div>
        </div>
      )}

      {job.phase === "cancelled" && (
        <div className="result">
          <p className="result-title">Download cancelled</p>
          <button className="btn ghost" onClick={reset}>
            Start over
          </button>
        </div>
      )}

      {job.phase === "error" && (
        <div className="result failure">
          <p className="result-title">Download failed</p>
          <pre className="error-detail">{job.message}</pre>
          <button className="btn ghost" onClick={reset}>
            Try again
          </button>
        </div>
      )}
    </section>
  );
}

function percentOf(event: ProgressEvent): number {
  if (event.percent) {
    const n = parseFloat(event.percent);
    if (!Number.isNaN(n)) return Math.min(100, Math.max(0, n));
  }
  if (event.downloadedBytes != null && event.totalBytes) {
    return (event.downloadedBytes / event.totalBytes) * 100;
  }
  return 0;
}

function formatBytes(bytes: number | null): string {
  if (bytes == null) return "…";
  const units = ["B", "KiB", "MiB", "GiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 100 || unit === 0 ? 0 : 1)} ${units[unit]}`;
}

function ProgressView({ event }: { event: ProgressEvent }) {
  const pct = percentOf(event);
  const merging = event.stage === "stream-finished";
  return (
    <div className="progress">
      <div className="progress-track">
        <div className="progress-fill" style={{ width: `${pct}%` }} />
      </div>
      <div className="progress-stats">
        <span className="percent">
          {merging ? "Merging…" : event.percent ?? "…"}
        </span>
        <span className="muted">
          {formatBytes(event.downloadedBytes)} /{" "}
          {formatBytes(event.totalBytes)}
        </span>
        <span className="muted">
          {event.speed ?? ""} {event.eta ? `· ETA ${event.eta}` : ""}
        </span>
      </div>
      <p className="hint">
        High-quality downloads fetch the video and audio streams separately,
        so the bar can fill twice.
      </p>
    </div>
  );
}
