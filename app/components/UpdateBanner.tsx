"use client";

import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { checkAppUpdate, type AppUpdate } from "@/lib/tauri";

/**
 * On every app start, checks GitHub for a newer Forvid release and shows
 * a dismissible banner when one exists. Silent when up to date, offline,
 * or the check fails.
 */
export default function UpdateBanner() {
  const [update, setUpdate] = useState<AppUpdate | null>(null);
  const [dismissedFor, setDismissedFor] = useState<string | null>(null);

  useEffect(() => {
    checkAppUpdate()
      .then(setUpdate)
      .catch(() => {});
  }, []);

  if (!update || update.version === dismissedFor) return null;

  return (
    <div className="update-banner">
      <span className="update-text">
        Forvid <strong>v{update.version}</strong> is available
      </span>
      <button
        className="chip-action"
        onClick={() => openUrl(update.url).catch(() => {})}
      >
        Download
      </button>
      <button
        className="chip-dismiss"
        aria-label="Dismiss"
        onClick={() => setDismissedFor(update.version)}
      >
        ×
      </button>
    </div>
  );
}
