"use client";

import { useEffect, useState } from "react";

type Theme = "system" | "light" | "dark";
const STORAGE_KEY = "forvid-theme";
const OPTIONS: { value: Theme; label: string }[] = [
  { value: "system", label: "Auto" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

/**
 * Appearance switcher. "system" removes the data-theme attribute so the
 * prefers-color-scheme media query governs; light/dark set it explicitly.
 * Persisted in localStorage (Tauri's webview storage survives restarts).
 */
export default function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>("system");

  useEffect(() => {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "light" || saved === "dark" || saved === "system") {
      setTheme(saved);
      applyTheme(saved);
    }
  }, []);

  const applyTheme = (t: Theme) => {
    if (t === "system") {
      delete document.documentElement.dataset.theme;
    } else {
      document.documentElement.dataset.theme = t;
    }
  };

  const select = (t: Theme) => {
    setTheme(t);
    localStorage.setItem(STORAGE_KEY, t);
    applyTheme(t);
  };

  return (
    <div className="theme-toggle" role="group" aria-label="Appearance">
      {OPTIONS.map(({ value, label }) => (
        <button
          key={value}
          className={`theme-option${theme === value ? " active" : ""}`}
          onClick={() => select(value)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
