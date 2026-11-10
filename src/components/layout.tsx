import { useEffect, useRef, useState } from 'react'
import { NavLink, useLocation } from 'react-router-dom'
import { getVersion } from '@tauri-apps/api/app'
import { openUrl } from '@tauri-apps/plugin-opener'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  CirclePlay,
  CircleStop,
  Copy,
  FolderOpen,
  LayoutDashboard,
  Minus,
  MonitorPlay,
  RefreshCw,
  Settings,
  Square,
  X,
} from 'lucide-react'
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

const appWindow = getCurrentWindow()
const REPO_URL = 'https://github.com/rryanchiu/BananaRecorder'

function GitHubMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true" className={className}>
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27s1.36.09 2 .27c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z" />
    </svg>
  )
}

function GitHubFooter() {
  const [version, setVersion] = useState('')

  useEffect(() => {
    void getVersion()
      .then(setVersion)
      .catch(() => {})
  }, [])

  return (
    <a
      href={REPO_URL}
      title="在 GitHub 上查看 BananaRecorder"
      onClick={(event) => {
        event.preventDefault()
        void openUrl(REPO_URL).catch(() => {})
      }}
      className="flex items-center gap-2 px-5 py-4 text-xs text-muted-foreground transition-colors hover:text-foreground"
    >
      <GitHubMark className="h-4 w-4" />
      <span>{version ? `v${version}` : 'GitHub'}</span>
    </a>
  )
}

/// Frameless-window controls (minimize / maximize / close) drawn in-app.
/// Close still routes through the Rust handler → hides to tray.
function WindowControls() {
  const [maximized, setMaximized] = useState(false)

  useEffect(() => {
    let unlisten: (() => void) | null = null
    const update = async () => setMaximized(await appWindow.isMaximized())
    void update()
    void appWindow.onResized(() => void update()).then((fn) => {
      unlisten = fn
    })
    return () => unlisten?.()
  }, [])

  const btn =
    'flex h-10 w-11 items-center justify-center text-muted-foreground transition-colors hover:bg-accent hover:text-foreground'

  return (
    <div className="flex items-center">
      <button className={btn} title="最小化" onClick={() => void appWindow.minimize()}>
        <Minus className="h-4 w-4" />
      </button>
      <button
        className={btn}
        title={maximized ? '还原' : '最大化'}
        onClick={() => void appWindow.toggleMaximize()}
      >
        {maximized ? <Copy className="h-3.5 w-3.5" /> : <Square className="h-3.5 w-3.5" />}
      </button>
      <button
        className="flex h-10 w-11 items-center justify-center text-muted-foreground transition-colors hover:bg-red-500 hover:text-white"
        title="关闭（隐藏到托盘，后台继续监控）"
        onClick={() => void appWindow.close()}
      >
        <X className="h-4 w-4" />
      </button>
    </div>
  )
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
    <div data-tauri-drag-region className="flex items-center gap-2">
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
            停止监控
          </Button>
        </>
      ) : (
        <Button size="sm" onClick={() => void control('start')}>
          <CirclePlay />
          开始监控
        </Button>
      )}
    </div>
  )
}

function LayoutInner({ children }: { children: React.ReactNode }) {
  const location = useLocation()
  const title = PAGE_TITLES[location.pathname] ?? '控制台'

  return (
    <div className="flex h-full overflow-hidden pt-10">
      {/* window top strip: own blank row — drag region + window controls */}
      <div
        data-tauri-drag-region
        className="fixed top-0 right-0 left-0 z-40 flex h-10 items-center justify-end bg-background md:left-56"
      >
        <WindowControls />
      </div>

      {/* Sidebar */}
      <aside data-tauri-drag-region className="fixed inset-y-0 left-0 z-30 hidden w-56 select-none flex-col border-r bg-card md:flex">
        <div data-tauri-drag-region className="flex items-center gap-2.5 px-5 py-4">
          <div data-tauri-drag-region className="flex h-9 w-9 items-center justify-center rounded-lg bg-amber-100 dark:bg-amber-900/40">
            <img data-tauri-drag-region src="banana.png" alt="banana" className="h-6 w-6 object-contain" draggable={false} />
          </div>
          <div data-tauri-drag-region>
            <div data-tauri-drag-region className="text-sm font-semibold leading-tight">香蕉录制</div>
            <div data-tauri-drag-region className="text-xs text-muted-foreground">BananaRecorder</div>
          </div>
        </div>
        <nav data-tauri-drag-region className="flex-1 space-y-1 px-3">
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
        <GitHubFooter />
      </aside>

      {/* Main area */}
      <div className="flex min-h-0 min-w-0 flex-1 flex-col md:pl-56">
        <header
          data-tauri-drag-region
          className="relative z-20 flex shrink-0 flex-col gap-3 border-b bg-background/95 px-4 py-3 backdrop-blur md:flex-row md:items-center md:justify-between md:px-6"
        >
          <h1 data-tauri-drag-region className="text-lg font-semibold select-none">
            {title}
          </h1>
          <RecorderControls />
        </header>
        {/* mobile nav */}
        <nav className="flex shrink-0 items-center gap-1 overflow-x-auto border-b px-2 py-1.5 md:hidden">
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
        <main className="min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-contain p-4 md:p-6">{children}</main>
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
