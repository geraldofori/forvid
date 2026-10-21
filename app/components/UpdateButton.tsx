"use client";

import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

type Phase = "idle" | "downloading" | "ready" | "failed";

/**
 * Top-left update pill, driven by Tauri's official updater plugin.
 * On app start it checks the latest.json manifest on the newest GitHub
 * release; when a newer version exists the pill appears. One click
 * downloads the update bundle, verifies its minisign signature against
 * the public key baked into the app, installs it in place, and offers a
 * restart — no browser, no installer UI.
 */
export default function UpdateButton() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [pct, setPct] = useState<number | null>(null);

  useEffect(() => {
    check()
      .then(setUpdate)
      .catch(() => {
        // No manifest yet, offline, or check failed — stay silent.
      });
  }, []);

  if (!update) return null;

  const install = async () => {
    setPhase("downloading");
    setPct(null);
    let total: number | undefined;
    let downloaded = 0;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          if (total) {
            setPct(Math.min(100, Math.round((downloaded / total) * 100)));
          }
        }
      });
      setPhase("ready");
    } catch {
      setPhase("failed");
    }
  };

  const label =
    phase === "downloading"
      ? pct != null
        ? `${pct}%`
        : "Updating…"
      : phase === "ready"
        ? "Restart to update"
        : phase === "failed"
          ? "Retry update"
          : `Update v${update.version}`;

  return (
    <button
      className={`update-pill${phase === "ready" ? " ready" : ""}`}
      onClick={phase === "ready" ? () => relaunch() : install}
      disabled={phase === "downloading"}
      title={
        phase === "ready"
          ? "Update installed — restart Forvid to use it"
          : `Forvid v${update.version} is available — click to update automatically`
      }
    >
      {phase === "downloading" ? (
        <span className="update-spinner" aria-hidden />
      ) : phase === "ready" ? (
        <svg
          width="13"
          height="13"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2.5"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden
        >
          <polyline points="23 4 23 10 17 10" />
          <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
        </svg>
      ) : (
        <svg
          width="13"
          height="13"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2.5"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden
        >
          <path d="M12 3v12" />
          <polyline points="7 10 12 15 17 10" />
          <line x1="4" y1="21" x2="20" y2="21" />
        </svg>
      )}
      <span>{label}</span>
    </button>
  );
}
