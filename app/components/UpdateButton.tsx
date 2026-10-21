"use client";

import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  checkAppUpdate,
  downloadAppUpdate,
  onAppUpdateProgress,
  type AppUpdate,
} from "@/lib/tauri";

type Phase = "idle" | "downloading" | "ready" | "failed";

/**
 * Top-left update pill. Appears only when a newer release exists. Clicking
 * downloads the asset matching this device (dmg / setup.exe / AppImage)
 * straight into ~/Downloads — no browser detour. Falls back to opening
 * the release page when no asset matches.
 */
export default function UpdateButton() {
  const [update, setUpdate] = useState<AppUpdate | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [pct, setPct] = useState<number | null>(null);
  const [path, setPath] = useState<string | null>(null);

  useEffect(() => {
    checkAppUpdate()
      .then(setUpdate)
      .catch(() => {});
    const unlisten = onAppUpdateProgress((e) => {
      if (e.downloadedBytes != null && e.totalBytes) {
        setPct(Math.round((e.downloadedBytes / e.totalBytes) * 100));
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  if (!update) return null;

  const startDownload = async () => {
    // No matching asset for this device: open the release page instead.
    if (!update.assetUrl || !update.assetName) {
      openUrl(update.url).catch(() => {});
      return;
    }
    setPhase("downloading");
    setPct(null);
    try {
      const dest = await downloadAppUpdate(update.assetUrl, update.assetName);
      setPath(dest);
      setPhase("ready");
      revealItemInDir(dest).catch(() => {});
    } catch {
      setPhase("failed");
    }
  };

  const label =
    phase === "downloading"
      ? pct != null
        ? `${pct}%`
        : "…"
      : phase === "ready"
        ? "Open installer"
        : phase === "failed"
          ? "Retry update"
          : `v${update.version}`;

  return (
    <button
      className={`update-pill${phase === "ready" ? " ready" : ""}`}
      onClick={phase === "ready" ? () => path && revealItemInDir(path).catch(() => {}) : startDownload}
      disabled={phase === "downloading"}
      title={
        phase === "ready"
          ? "Installer downloaded to ~/Downloads — click to reveal"
          : `Forvid v${update.version} is available — click to download`
      }
    >
      {phase === "downloading" ? (
        <span className="update-spinner" aria-hidden />
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
