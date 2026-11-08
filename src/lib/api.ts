// Typed API layer over Tauri IPC commands.

import { invoke, convertFileSrc } from "@tauri-apps/api/core";

export interface Room {
  id: string
  enabled: boolean
  quality: string
  url: string
  name: string
}

export interface RoomListResponse {
  rooms: Room[]
  recorder_running: boolean
}

export interface RecordingFile {
  room_id: string
  anchor: string
  folder: string
  file: string
  size: number
}

export interface RecorderInfo {
  running: boolean
  uptime: number
}

export interface StatusResponse {
  recorder: RecorderInfo
  rooms: { total: number; enabled: number }
  recording_files: RecordingFile[]
  save_path: string
  disk_free_gb: number | null
}

export interface AppSettings {
  save_path: string
  save_format: string // ts | mkv | flv | mp4
  quality: string // 原画/超清/高清/标清/流畅
  delay_seconds: number
  split_enabled: boolean
  split_seconds: number
  convert_to_mp4: boolean
  delete_origin: boolean
  folder_by_author: boolean
  folder_by_time: boolean
  douyin_cookie: string
  bilibili_cookie: string
  proxy_addr: string
  proxy_enabled: boolean
}

export interface VideoDir {
  name: string
  path: string
  file_count: number
  total_size: number
}

export interface VideoFile {
  name: string
  path: string
  size: number
  mtime: number
  ext: string
}

export interface VideoListResponse {
  root: string
  path: string
  parent: string
  dirs: VideoDir[]
  files: VideoFile[]
  total_size: number
}

export interface LogsResponse {
  lines: string[]
  source: string
}

export class ApiError extends Error {
  status: number
  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

async function request<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args)
  } catch (err) {
    const message = typeof err === "string" ? err : err instanceof Error ? err.message : String(err)
    throw new ApiError(0, message)
  }
}

export const api = {
  // status
  getStatus: () => request<StatusResponse>("get_status"),
  getLogs: (source = "app", lines = 200) => request<LogsResponse>("get_logs", { source, lines }),

  // recorder control
  controlRecorder: (action: "start" | "stop") =>
    request<{ ok: boolean; running: boolean }>("control_recorder", { action }),

  // rooms
  listRooms: () => request<RoomListResponse>("list_rooms"),
  createRoom: (room: { url: string; name?: string; quality?: string }) =>
    request<{ ok: boolean }>("create_room", { room }),
  updateRoom: (id: string, patch: Partial<Pick<Room, "url" | "name" | "quality" | "enabled">>) =>
    request<{ ok: boolean }>("update_room", { id, patch }),
  deleteRoom: (id: string) => request<{ ok: boolean }>("delete_room", { id }),
  stopRecording: (roomId: string) =>
    request<{ ok: boolean }>("stop_recording", { roomId }),

  // settings
  getSettings: () => request<AppSettings>("get_settings"),
  updateSettings: (settings: AppSettings) => request<AppSettings>("update_settings", { settings }),
  checkSavePathChange: (newPath: string) =>
    request<{
      changed: boolean
      old_root: string
      new_root: string
      file_count: number
      total_size: number
    }>("check_save_path_change", { newPath }),
  migrateSavePath: (newPath: string) =>
    request<{ moved: number; skipped: number; failed: number }>("migrate_save_path", { newPath }),

  // videos
  listVideos: (path = "") => request<VideoListResponse>("list_videos", { path }),
  videoSrc: (absolutePath: string) => convertFileSrc(absolutePath),
  deleteVideo: (path: string) => request<{ ok: boolean }>("delete_video", { path }),
  deleteVideos: (paths: string[]) =>
    request<{ deleted: number; failed: number }>("delete_videos", { paths }),
  mergeVideos: (paths: string[]) =>
    request<{ ok: boolean; output: string; count: number }>("merge_videos", { paths }),
  transcodeVideos: (paths: string[]) =>
    request<{ converted: number; skipped: number; failed: number }>("transcode_videos", { paths }),
  openVideoFolder: (path: string) => request<{ ok: boolean }>("open_video_folder", { path }),
  saveVideoAs: (path: string, destPath: string) =>
    request<{ ok: boolean }>("save_video_as", { path, destPath }),
}

// ---------------------------------------------------------------------------
// Platform detection from room url
// ---------------------------------------------------------------------------

const PLATFORM_MAP: [string, string][] = [
  ["live.douyin.com", "抖音"],
  ["v.douyin.com", "抖音"],
  ["www.douyin.com", "抖音"],
  ["live.kuaishou.com", "快手"],
  ["www.huya.com", "虎牙"],
  ["live.bilibili.com", "B站"],
]

export function detectPlatform(url: string): string {
  for (const [host, name] of PLATFORM_MAP) {
    if (url.includes(host)) return name
  }
  return "未知平台"
}

export const SUPPORTED_PLATFORMS = ["抖音", "B站", "虎牙", "快手"]
