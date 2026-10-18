"use client";

import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { checkEnvironment, type EnvironmentStatus } from "@/lib/tauri";

function versionToken(full: string | null): string {
  if (!full) return "missing";
  const match = full.match(/\d+\.\d+[\.\d]*/);
  return match ? match[0] : full;
}

/** Info button with a modal summarising what Forvid is and how it works. */
export default function InfoButton() {
  const [open, setOpen] = useState(false);
  const [env, setEnv] = useState<EnvironmentStatus | null>(null);

  // Load versions lazily the first time the popup opens.
  useEffect(() => {
    if (open && !env) {
      checkEnvironment()
        .then(setEnv)
        .catch(() => {});
    }
  }, [open, env]);

  // Escape closes the modal.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  return (
    <>
      <button
        className="info-button"
        aria-label="About Forvid"
        title="About Forvid"
        onClick={() => setOpen(true)}
      >
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden
        >
          <circle cx="12" cy="12" r="10" />
          <line x1="12" y1="16" x2="12" y2="12" />
          <line x1="12" y1="8" x2="12.01" y2="8" />
        </svg>
      </button>

      {open && (
        <div className="modal-overlay" onClick={() => setOpen(false)}>
          <div
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-label="About Forvid"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="modal-head">
              <h2>Forvid</h2>
              <button
                className="modal-close"
                aria-label="Close"
                onClick={() => setOpen(false)}
              >
                ×
              </button>
            </div>

            <p className="modal-lede">
              Paste a link, pick a quality, get the video. A small native app
              powered by yt-dlp — no ads, no downloader websites.
            </p>

            <ul className="modal-list">
              <li>Single videos and whole playlists (saved to their own folder)</li>
              <li>Real quality options per video, plus audio-only MP3</li>
              <li>Live progress with speed and ETA — cancel anytime</li>
              <li>Notices links on your clipboard and offers to use them</li>
            </ul>

            <p className="muted">
              Works through the yt-dlp and ffmpeg installed on your system.
            </p>
            {env && (
              <p className="modal-versions">
                yt-dlp {versionToken(env.ytdlpVersion)} · ffmpeg{" "}
                {versionToken(env.ffmpegVersion)} · deno{" "}
                {versionToken(env.denoVersion)}
              </p>
            )}

            <button
              className="btn ghost modal-github"
              onClick={() =>
                openUrl("https://github.com/geraldofori/forvid").catch(() => {})
              }
            >
              View on GitHub
            </button>
          </div>
        </div>
      )}
    </>
  );
}
