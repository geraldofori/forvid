"use client";

import { useEffect, useRef, useState } from "react";
import {
  cancelDownload,
  fetchMetadata,
  onDownloadProgress,
  startDownload,
  type ProgressEvent,
  type VideoInfo,
} from "@/lib/tauri";

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
  const [job, setJob] = useState<JobState>({ phase: "idle" });

  const jobIdRef = useRef<string | null>(null);
  // Guards against out-of-order metadata responses when the URL changes fast.
  const metaSeq = useRef(0);

  // Single progress listener for the app's lifetime; events for jobs that
  // are not the current one (e.g. a stale job id) are ignored.
  useEffect(() => {
    const unlisten = onDownloadProgress((event) => {
      if (event.jobId !== jobIdRef.current) return;
      if (TERMINAL_STAGES.has(event.stage)) {
        jobIdRef.current = null;
        if (event.stage === "completed") {
          setJob({ phase: "completed", filepath: event.filepath ?? "" });
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
    const trimmed = url.trim();
    if (!/^https?:\/\/.+/.test(trimmed)) return;

    const seq = ++metaSeq.current;
    const timer = setTimeout(async () => {
      setMetaLoading(true);
      try {
        const info = await fetchMetadata(trimmed);
        if (seq === metaSeq.current) setMeta(info);
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
    try {
      const id = await startDownload(trimmed, { kind: "best" });
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

  const downloading = job.phase === "downloading";

  return (
    <section className="downloader">
      <header className="brand">
        <h1>Forvid</h1>
        <p>Paste a link. Pick a quality. Get the video.</p>
      </header>

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
      {meta && !downloading && job.phase === "idle" && (
        <p className="hint ready">Ready: {meta.title}</p>
      )}

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
          <button className="btn ghost" onClick={reset}>
            Download another
          </button>
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
