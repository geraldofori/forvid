"use client";

import { useEffect, useState } from "react";
import { checkEnvironment, type EnvironmentStatus } from "@/lib/tauri";

/** "ffmpeg version 9.0.2 Copyright ..." -> "9.0.2"; "deno 2.9.7 (...)" -> "2.9.7" */
function versionToken(full: string | null): string {
  if (!full) return "missing";
  const match = full.match(/\d+\.\d+[\.\d]*/);
  return match ? match[0] : full;
}

export default function StatusBar() {
  const [env, setEnv] = useState<EnvironmentStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    checkEnvironment()
      .then(setEnv)
      .catch((e) => setError(String(e)));
  }, []);

  if (error) {
    return <footer className="status-bar error">{error}</footer>;
  }
  if (!env) {
    return <footer className="status-bar">Checking environment…</footer>;
  }
  return (
    <footer className="status-bar">
      <span>yt-dlp {versionToken(env.ytdlpVersion)}</span>
      <span>ffmpeg {versionToken(env.ffmpegVersion)}</span>
      <span>deno {versionToken(env.denoVersion)}</span>
    </footer>
  );
}
