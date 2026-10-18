import type { VideoInfo } from "@/lib/tauri";

function formatDuration(meta: VideoInfo): string | null {
  if (meta.durationSeconds == null) return meta.durationString;
  const s = Math.round(meta.durationSeconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  const mm = h > 0 ? String(m).padStart(2, "0") : String(m);
  return `${h > 0 ? `${h}:` : ""}${mm}:${String(r).padStart(2, "0")}`;
}

export default function MetadataCard({ meta }: { meta: VideoInfo }) {
  const duration = formatDuration(meta);
  return (
    <div className="meta-card">
      {meta.thumbnail && (
        // Plain <img>: remote thumbnail, no next/image loader in a static export.
        // eslint-disable-next-line @next/next/no-img-element
        <img className="meta-thumb" src={meta.thumbnail} alt="" />
      )}
      <div className="meta-body">
        <p className="meta-title">{meta.title}</p>
        <p className="meta-sub">
          {[meta.uploader, duration].filter(Boolean).join(" · ")}
        </p>
      </div>
    </div>
  );
}
