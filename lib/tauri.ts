import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Mirrors src-tauri/src/models.rs — keep in sync. */

export interface EnvironmentStatus {
  ytdlpPath: string | null;
  ytdlpVersion: string | null;
  ffmpegPath: string | null;
  ffmpegVersion: string | null;
  denoPath: string | null;
  denoVersion: string | null;
}

export interface VideoInfo {
  id: string;
  title: string;
  thumbnail: string | null;
  durationSeconds: number | null;
  durationString: string | null;
  uploader: string | null;
  webpageUrl: string;
  availableHeights: number[];
}

export interface PlaylistEntry {
  id: string;
  title: string;
  durationSeconds: number | null;
  url: string | null;
}

export interface PlaylistInfo {
  id: string;
  title: string;
  uploader: string | null;
  thumbnail: string | null;
  videoCount: number;
  entries: PlaylistEntry[];
}

/** What a URL points at: a single video or a whole playlist. */
export type MediaInfo =
  | ({ type: "video" } & VideoInfo)
  | ({ type: "playlist" } & PlaylistInfo);

export type Resolution =
  | { kind: "best" }
  | { kind: "height"; height: number }
  | { kind: "audioMp3" };

export type DownloadStage =
  | "downloading"
  | "stream-finished"
  | "completed"
  | "cancelled"
  | "error";

export interface ProgressEvent {
  jobId: string;
  stage: DownloadStage;
  percent: string | null;
  speed: string | null;
  eta: string | null;
  downloadedBytes: number | null;
  totalBytes: number | null;
  filepath: string | null;
  error: string | null;
  playlistIndex: number | null;
  playlistCount: number | null;
}

export function checkEnvironment() {
  return invoke<EnvironmentStatus>("check_environment");
}

export function fetchMetadata(url: string) {
  return invoke<MediaInfo>("fetch_metadata", { url });
}

export function startDownload(
  url: string,
  resolution: Resolution,
  outputDir: string | null,
  playlist: boolean,
) {
  return invoke<string>("start_download", {
    request: { url, resolution, outputDir, playlist },
  });
}

export function cancelDownload(jobId: string) {
  return invoke<void>("cancel_download", { jobId });
}

export function onDownloadProgress(
  callback: (event: ProgressEvent) => void,
): Promise<UnlistenFn> {
  return listen<ProgressEvent>("download-progress", (e) => callback(e.payload));
}
