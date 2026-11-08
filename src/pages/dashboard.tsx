import { useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { Clock, FolderOpen, HardDrive, MonitorPlay, Video } from 'lucide-react'
import { api } from '@/lib/api'
import { useStatus } from '@/lib/status'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { formatBytes, formatDuration } from '@/lib/utils'
import { cn } from '@/lib/utils'

function StatCard({
  title,
  icon: Icon,
  value,
  sub,
}: {
  title: string
  icon: React.ComponentType<{ className?: string }>
  value: string
  sub?: string
}) {
  return (
    <Card>
      <CardContent className="flex items-center gap-4 p-5">
        <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-primary/10 text-primary dark:bg-primary/20">
          <Icon className="h-5 w-5" />
        </div>
        <div className="min-w-0">
          <div className="text-sm text-muted-foreground">{title}</div>
          <div className="truncate text-xl font-semibold">{value}</div>
          {sub && <div className="truncate text-xs text-muted-foreground">{sub}</div>}
        </div>
      </CardContent>
    </Card>
  )
}

const MAX_LOG_LINES = 300

function LogPanel() {
  const [lines, setLines] = useState<string[]>([])
  const boxRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    // initial fill from the in-memory ring buffer
    void api.getLogs('app', 150).then((data) => setLines(data.lines)).catch(() => {})
    // then follow live log events pushed by the recorder
    const unlisten = listen<{ level: string; message: string }>('recorder://log', (event) => {
      const prefix = event.payload.level === 'ERROR' ? '[E] ' : event.payload.level === 'WARN' ? '[W] ' : ''
      setLines((prev) => {
        const next = [...prev, prefix + event.payload.message]
        return next.length > MAX_LOG_LINES ? next.slice(-MAX_LOG_LINES) : next
      })
    })
    return () => {
      void unlisten.then((fn) => fn())
    }
  }, [])

  useEffect(() => {
    const box = boxRef.current
    if (box) box.scrollTop = box.scrollHeight
  }, [lines])

  return (
    <Card className="flex h-full flex-col">
      <CardHeader className="flex-row items-center justify-between space-y-0 pb-3">
        <CardTitle className="text-base">运行日志</CardTitle>
        <span className="text-xs text-muted-foreground">实时</span>
      </CardHeader>
      <CardContent className="flex-1 pt-0">
        <div
          ref={boxRef}
          className={cn('h-72 overflow-y-auto rounded-lg bg-zinc-950 p-3 font-mono text-xs leading-5 text-zinc-200')}
        >
          {lines.length === 0 ? (
            <div className="text-zinc-500">暂无日志</div>
          ) : (
            lines.map((line, i) => (
              <div
                key={i}
                className={cn(
                  'whitespace-pre-wrap break-all',
                  /ERROR|错误|失败/i.test(line) && 'text-red-400',
                  /WARN|警告/i.test(line) && 'text-amber-300'
                )}
              >
                {line || '\u00a0'}
              </div>
            ))
          )}
        </div>
      </CardContent>
    </Card>
  )
}

export default function Dashboard() {
  const { status } = useStatus()
  const running = status?.recorder.running ?? false

  return (
    <div className="space-y-6">
      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <StatCard
          title="监测直播间"
          icon={MonitorPlay}
          value={`${status?.rooms.total ?? 0} 个`}
          sub={`已启用 ${status?.rooms.enabled ?? 0} 个`}
        />
        <StatCard
          title="正在录制"
          icon={Video}
          value={`${status?.recording_files.length ?? 0} 个`}
          sub={running ? '录制引擎运行中' : '录制引擎已暂停'}
        />
        <StatCard
          title="引擎运行时长"
          icon={Clock}
          value={running ? formatDuration(status?.recorder.uptime ?? 0) : '未运行'}
        />
        <StatCard
          title="磁盘剩余空间"
          icon={HardDrive}
          value={status?.disk_free_gb != null ? `${status.disk_free_gb} GB` : '--'}
          sub={status?.save_path}
        />
      </div>

      <div className="grid gap-4 xl:grid-cols-5">
        <Card className="xl:col-span-2">
          <CardHeader className="flex-row items-center justify-between space-y-0 pb-3">
            <CardTitle className="text-base">正在录制的文件</CardTitle>
            <Badge variant={running ? 'success' : 'secondary'}>{running ? '运行中' : '已暂停'}</Badge>
          </CardHeader>
          <CardContent className="pt-0">
            {(status?.recording_files.length ?? 0) === 0 ? (
              <div className="flex h-40 flex-col items-center justify-center gap-2 rounded-lg border border-dashed text-sm text-muted-foreground">
                <FolderOpen className="h-8 w-8 opacity-40" />
                {running ? '当前没有直播间正在推流，等待开播...' : '启动录制引擎后，直播间开播将自动录制'}
              </div>
            ) : (
              <div className="max-h-72 space-y-2 overflow-y-auto">
                {status!.recording_files.map((f) => (
                  <div key={`${f.folder}/${f.name}`} className="flex items-center gap-3 rounded-lg border p-3">
                    <span className="relative flex h-2.5 w-2.5">
                      <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-red-500 opacity-60" />
                      <span className="relative inline-flex h-2.5 w-2.5 rounded-full bg-red-500" />
                    </span>
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm font-medium">{f.name}</div>
                      <div className="truncate text-xs text-muted-foreground">{f.folder || '根目录'}</div>
                    </div>
                    <Badge variant="secondary">{formatBytes(f.size)}</Badge>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>

        <div className="xl:col-span-3">
          <LogPanel />
        </div>
      </div>
    </div>
  )
}
