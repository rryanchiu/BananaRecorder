import { useEffect, useRef, useState } from 'react'
import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import Hls from 'hls.js'
import { Maximize, Pause, Play, Volume2, VolumeX } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from '@/components/ui/dialog'

export function LivePreview({ roomId, anchor, onClose }: { roomId: string; anchor: string; onClose: () => void }) {
  const [video, setVideo] = useState<HTMLVideoElement | null>(null)
  const hlsRef = useRef<Hls | null>(null)
  const playerRef = useRef<HTMLDivElement>(null)
  const [paused, setPaused] = useState(true)
  const [muted, setMuted] = useState(true)
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(true)
  const [attempt, setAttempt] = useState(0)
  useEffect(() => {
    // Radix mounts Portal children after the parent's first effect. Wait for
    // the actual element, including StrictMode's mount/cleanup cycle.
    if (!video) return
    let disposed = false
    let sessionId: string | undefined
    let timer: ReturnType<typeof setInterval> | undefined
    setError('')
    setLoading(true)
    const stop = () => { if (sessionId) void invoke('stop_preview', { id: sessionId }).catch(() => {}) }
    const fail = (message: string) => {
      if (disposed) return
      setError(message)
      setLoading(false)
      hlsRef.current?.destroy()
      hlsRef.current = null
      if (timer) clearInterval(timer)
      stop()
    }
    void invoke<{ id: string; playlist: string }>('start_preview', { roomId }).then((preview) => {
      sessionId = preview.id
      if (disposed) { stop(); return }
      timer = setInterval(() => {
        void invoke('touch_preview', { id: preview.id }).catch(() => fail('直播流已断开，请重试'))
      }, 10000)
      // Preserve path separators so relative HLS segment URLs resolve in the
      // playlist directory rather than at the asset protocol's root.
      const source = convertFileSrc(preview.playlist).replace(/%2f|%5c/gi, '/')
      if (Hls.isSupported()) {
        const hls = new Hls({ liveSyncDurationCount: 2, liveMaxLatencyDurationCount: 5, maxLiveSyncPlaybackRate: 1.2 })
        hlsRef.current = hls
        hls.on(Hls.Events.ERROR, (_, data) => {
          if (data.fatal) fail('无法播放此直播流，可能是网络中断或视频编码不受支持。请重试。')
        })
        hls.on(Hls.Events.MANIFEST_PARSED, () => { void video.play().catch(() => setLoading(false)) })
        hls.loadSource(source)
        hls.attachMedia(video)
      } else if (video.canPlayType('application/vnd.apple.mpegurl')) {
        video.src = source
        void video.play().catch(() => setLoading(false))
      } else fail('当前运行环境不支持直播播放，请更新 WebView2')
    }).catch((err) => fail(String(err)))
    return () => {
      disposed = true
      if (timer) clearInterval(timer)
      hlsRef.current?.destroy()
      hlsRef.current = null
      video.pause()
      video.removeAttribute('src')
      video.load()
      stop()
    }
  }, [roomId, attempt, video])

  return <Dialog open onOpenChange={(open) => { if (!open) onClose() }}>
    <DialogContent className="max-w-4xl">
      <DialogHeader>
        <DialogTitle>{anchor || '直播'} · 实时观看</DialogTitle>
        <DialogDescription>跟随当前直播流，画面有数秒缓冲延迟。关闭观看不会停止录制。</DialogDescription>
      </DialogHeader>
      <div ref={playerRef} className="relative flex aspect-video flex-col overflow-hidden rounded-lg bg-black">
        <video ref={setVideo} autoPlay muted={muted} playsInline className="min-h-0 w-full flex-1" onPlay={() => setPaused(false)} onPause={() => setPaused(true)} onVolumeChange={(event) => setMuted(event.currentTarget.muted)} onPlaying={() => setLoading(false)} onError={() => { setError('视频解码失败，请重试或更换录制画质'); setLoading(false) }} />
        {loading && !error && <div className="pointer-events-none absolute inset-0 flex items-center justify-center text-sm text-white">正在连接直播流…</div>}
        <div className="flex shrink-0 items-center gap-2 bg-zinc-950 px-3 py-2 text-white">
          <Button variant="ghost" size="icon" aria-label={paused ? '播放直播' : '暂停直播'} onClick={() => {
            if (!video) return
            if (video.paused) void video.play().catch(() => setError('无法开始播放，请重试'))
            else video.pause()
          }}>{paused ? <Play /> : <Pause />}</Button>
          <Button variant="ghost" size="icon" aria-label={muted ? '打开声音' : '静音'} onClick={() => {
            if (video) video.muted = !video.muted
          }}>{muted ? <VolumeX /> : <Volume2 />}</Button>
          <span className="flex flex-1 items-center gap-2 text-xs"><span className="h-2 w-2 rounded-full bg-red-500" />直播</span>
          <Button variant="ghost" size="icon" aria-label="切换全屏" onClick={() => {
            const action = document.fullscreenElement ? document.exitFullscreen() : playerRef.current?.requestFullscreen()
            void action?.catch(() => setError('当前环境无法切换全屏'))
          }}><Maximize /></Button>
        </div>
      </div>
      <div className="flex items-center justify-between gap-3">
        <span className="text-sm text-muted-foreground">{error || '默认静音，可通过播放器打开声音。'}</span>
        {error && <Button variant="outline" onClick={() => setAttempt((n) => n + 1)}>重试</Button>}
      </div>
    </DialogContent>
  </Dialog>
}
