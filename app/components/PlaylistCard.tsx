import type { PlaylistInfo } from "@/lib/tauri";

export default function PlaylistCard({
  playlist,
  saveTo,
}: {
  playlist: PlaylistInfo;
  saveTo: string;
}) {
  return (
    <div className="meta-card playlist-card">
      <div className="playlist-badge" aria-hidden>
        ▤
      </div>
      <div className="meta-body">
        <p className="meta-title">{playlist.title}</p>
        <p className="meta-sub">
          {[
            "Playlist",
            `${playlist.videoCount} videos`,
            playlist.uploader,
          ]
            .filter(Boolean)
            .join(" · ")}
        </p>
        <p className="meta-sub playlist-dest">
          Saves into a new folder: {saveTo}/{playlist.title}/
        </p>
      </div>
    </div>
  );
}
