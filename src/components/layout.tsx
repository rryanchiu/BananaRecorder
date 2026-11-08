import { useRef } from 'react'
import { NavLink, useLocation } from 'react-router-dom'
import { FolderOpen, LayoutDashboard, MonitorPlay, Radio, Settings, CirclePlay, CircleStop, RefreshCw } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { useStatus } from '@/lib/status'
import { StatusProvider } from '@/lib/status'
import { api } from '@/lib/api'

const NAV_ITEMS = [
  { to: '/', label: '仪表盘', icon: LayoutDashboard },
  { to: '/rooms', label: '直播间管理', icon: MonitorPlay },
  { to: '/videos', label: '视频库', icon: FolderOpen },
  { to: '/settings', label: '录制设置', icon: Settings },
]

const PAGE_TITLES: Record<string, string> = {
  '/': '仪表盘',
  '/rooms': '直播间管理',
  '/videos': '视频库',
  '/settings': '录制设置',
}

function RecorderControls() {
  const { status, refresh } = useStatus()
  const running = status?.recorder.running ?? false
  const busy = useRef(false)

  const control = async (action: 'start' | 'stop' | 'restart') => {
    if (busy.current) return
    busy.current = true
    try {
      if (action === 'restart') {
        await api.controlRecorder('stop')
        await api.controlRecorder('start')
      } else {
        await api.controlRecorder(action)
      }
      const msg = action === 'start' ? '录制引擎已启动' : action === 'stop' ? '录制引擎已停止' : '录制引擎已重启'
      toast.success(msg)
      await refresh()
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '操作失败')
    } finally {
      busy.current = false
    }
  }

  return (
    <div className="flex items-center gap-2">
      <Badge variant={running ? 'success' : 'secondary'} className="gap-1.5">
        <span className={cn('inline-block h-1.5 w-1.5 rounded-full', running ? 'bg-emerald-500 animate-pulse' : 'bg-muted-foreground')} />
        {running ? '录制引擎运行中' : '录制引擎已停止'}
      </Badge>
      {running ? (
        <>
          <Button variant="outline" size="sm" onClick={() => void control('restart')} title="重启录制引擎">
            <RefreshCw />
            重启
          </Button>
          <Button variant="destructive" size="sm" onClick={() => void control('stop')}>
            <CircleStop />
            停止录制
          </Button>
        </>
      ) : (
        <Button size="sm" onClick={() => void control('start')}>
          <CirclePlay />
          开始录制
        </Button>
      )}
    </div>
  )
}

function LayoutInner({ children }: { children: React.ReactNode }) {
  const location = useLocation()
  const title = PAGE_TITLES[location.pathname] ?? '控制台'

  return (
    <div className="flex min-h-screen">
      {/* Sidebar */}
      <aside className="fixed inset-y-0 left-0 z-30 hidden w-56 flex-col border-r bg-card md:flex">
        <div className="flex items-center gap-2.5 px-5 py-5">
          <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-primary text-primary-foreground">
            <Radio className="h-5 w-5" />
          </div>
          <div>
            <div className="text-sm font-semibold leading-tight">直播录制</div>
            <div className="text-xs text-muted-foreground">LiveRecorder</div>
          </div>
        </div>
        <nav className="flex-1 space-y-1 px-3">
          {NAV_ITEMS.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.to === '/'}
              className={({ isActive }) =>
                cn(
                  'flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors',
                  isActive
                    ? 'bg-primary/10 font-medium text-primary dark:bg-primary/20'
                    : 'text-muted-foreground hover:bg-accent hover:text-foreground'
                )
              }
            >
              <item.icon className="h-4 w-4" />
              {item.label}
            </NavLink>
          ))}
        </nav>
        <div className="border-t px-5 py-4 text-xs text-muted-foreground">
          基于 FFmpeg 多平台直播录制
        </div>
      </aside>

      {/* Main area */}
      <div className="flex flex-1 flex-col md:pl-56">
        <header className="sticky top-0 z-20 flex flex-col gap-3 border-b bg-background/95 px-4 py-3 backdrop-blur md:flex-row md:items-center md:justify-between md:px-6">
          <h1 className="text-lg font-semibold">{title}</h1>
          <RecorderControls />
        </header>
        {/* mobile nav */}
        <nav className="flex items-center gap-1 overflow-x-auto border-b px-2 py-1.5 md:hidden">
          {NAV_ITEMS.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.to === '/'}
              className={({ isActive }) =>
                cn(
                  'flex items-center gap-1.5 whitespace-nowrap rounded-md px-3 py-1.5 text-sm',
                  isActive ? 'bg-primary/10 font-medium text-primary' : 'text-muted-foreground'
                )
              }
            >
              <item.icon className="h-4 w-4" />
              {item.label}
            </NavLink>
          ))}
        </nav>
        <main className="flex-1 p-4 md:p-6">{children}</main>
      </div>
    </div>
  )
}

export function Layout({ children }: { children: React.ReactNode }) {
  return (
    <StatusProvider>
      <LayoutInner>{children}</LayoutInner>
    </StatusProvider>
  )
}
