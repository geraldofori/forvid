"use client";

import { useEffect, useState } from "react";
import { checkTools } from "@/lib/tauri";
import Downloader from "./Downloader";
import SetupScreen from "./SetupScreen";

/**
 * First-run gate: verifies the tools Forvid needs are available (managed
 * or system). Shows a friendly one-time setup screen when they are not.
 */
export default function AppGate() {
  const [ready, setReady] = useState<boolean | null>(null);

  useEffect(() => {
    checkTools()
      .then((status) => setReady(!status.needsSetup))
      // If the check itself fails, don't lock the user out.
      .catch(() => setReady(true));
  }, []);

  if (ready === null) {
    return (
      <div className="gate">
        <p className="hint">Checking tools…</p>
      </div>
    );
  }
  if (!ready) {
    return <SetupScreen onDone={() => setReady(true)} />;
  }
  return <Downloader />;
}
