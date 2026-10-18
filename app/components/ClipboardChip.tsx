"use client";

import { useEffect, useState } from "react";
import { readText } from "@tauri-apps/plugin-clipboard-manager";

const URL_PATTERN = /^https?:\/\/\S+$/;

/**
 * Surfaces the last-copied link when the window regains focus, so the user
 * can fill the input with one click. Dismissal is remembered per-link: the
 * same URL won't be offered again, a newly copied one will.
 */
export default function ClipboardChip({ onUse }: { onUse: (text: string) => void }) {
  const [candidate, setCandidate] = useState<string | null>(null);
  const [dismissed, setDismissed] = useState<string | null>(null);

  useEffect(() => {
    const check = async () => {
      try {
        const text = (await readText())?.trim();
        if (text && URL_PATTERN.test(text) && text !== dismissed) {
          setCandidate(text);
        }
      } catch {
        // Clipboard empty, non-text, or permission denied — stay quiet.
      }
    };
    check();
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, [dismissed]);

  if (!candidate) return null;

  const display =
    candidate.length > 48 ? `${candidate.slice(0, 48)}…` : candidate;

  return (
    <div className="clipboard-chip">
      <span className="chip-text" title={candidate}>
        Link copied: {display}
      </span>
      <button
        className="chip-action"
        onClick={() => {
          onUse(candidate);
          setCandidate(null);
        }}
      >
        Use link
      </button>
      <button
        className="chip-dismiss"
        aria-label="Dismiss"
        onClick={() => {
          setDismissed(candidate);
          setCandidate(null);
        }}
      >
        ×
      </button>
    </div>
  );
}
