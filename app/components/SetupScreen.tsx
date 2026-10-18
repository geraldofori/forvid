"use client";

import { useEffect, useState } from "react";
import {
  onToolsProgress,
  setupTools,
  type ToolsProgress,
} from "@/lib/tauri";

const TOOLS: { id: string; name: string; role: string }[] = [
  { id: "yt-dlp", name: "yt-dlp", role: "Video downloader" },
  { id: "ffmpeg", name: "ffmpeg", role: "Video converter" },
  { id: "ffprobe", name: "ffprobe", role: "Video inspector" },
  { id: "deno", name: "deno", role: "JavaScript runtime (YouTube support)" },
];

type Phase = "intro" | "working" | "error";

/**
 * One-time first-run setup: downloads the tools Forvid needs, with
 * per-tool progress. Plain language — no package managers mentioned.
 */
export default function SetupScreen({ onDone }: { onDone: () => void }) {
  const [phase, setPhase] = useState<Phase>("intro");
  const [progress, setProgress] = useState<Record<string, ToolsProgress>>({});
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const unlisten = onToolsProgress((event) => {
      setProgress((prev) => ({ ...prev, [event.tool]: event }));
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const start = async () => {
    setPhase("working");
    setError(null);
    try {
      await setupTools();
      onDone();
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  };

  return (
    <div className="gate">
      <div className="setup">
        <h1 className="setup-title">Set up Forvid</h1>
        <p className="setup-copy">
          Forvid needs a few small tools to do its job. They download once —
          about 120 MB — and live inside the app&apos;s own folder. No
          accounts, no package managers, no admin rights.
        </p>

        {phase === "intro" && (
          <button className="btn primary" onClick={start}>
            Download tools
          </button>
        )}

        {phase !== "intro" && (
          <div className="setup-tools">
            {TOOLS.map(({ id, name, role }) => (
              <ToolRow key={id} name={name} role={role} event={progress[id]} />
            ))}
          </div>
        )}

        {phase === "error" && (
          <>
            <p className="error-text">
              {error ?? "Something went wrong while setting up."}
            </p>
            <button className="btn primary" onClick={start}>
              Try again
            </button>
          </>
        )}
      </div>
    </div>
  );
}

function percentOf(event: ToolsProgress | undefined): number | null {
  if (!event || event.downloadedBytes == null || !event.totalBytes) return null;
  return Math.min(100, (event.downloadedBytes / event.totalBytes) * 100);
}

function ToolRow({
  name,
  role,
  event,
}: {
  name: string;
  role: string;
  event: ToolsProgress | undefined;
}) {
  const stage = event?.stage ?? "pending";
  const pct = percentOf(event);
  return (
    <div className={`tool-row ${stage}`}>
      <div className="tool-info">
        <span className="tool-name">{name}</span>
        <span className="tool-role">{role}</span>
      </div>
      <div className="tool-state">
        {stage === "pending" && <span className="muted">Waiting…</span>}
        {stage === "downloading" && (
          <div className="tool-progress">
            <div className="progress-track">
              <div
                className="progress-fill"
                style={{ width: `${pct ?? 100}%`, opacity: pct == null ? 0.4 : 1 }}
              />
            </div>
            {pct != null && (
              <span className="muted tool-pct">{Math.round(pct)}%</span>
            )}
          </div>
        )}
        {stage === "installing" && <span className="muted">Installing…</span>}
        {stage === "done" && <span className="tool-done">Done</span>}
        {stage === "error" && <span className="error-text">Failed</span>}
      </div>
    </div>
  );
}
