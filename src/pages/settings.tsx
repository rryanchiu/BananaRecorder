import { useCallback, useEffect, useMemo, useState } from 'react'
import { Download, FolderOpen, Package, Save } from 'lucide-react'
import { listen } from '@tauri-apps/api/event'
import { toast } from 'sonner'
import {
  api,
  type AppSettings,
  type FfmpegInstallProgress,
  type FfmpegSource,
  type FfmpegStatus,
} from '@/lib/api'
import { useStatus } from '@/lib/status'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Textarea } from '@/components/ui/textarea'

const DEFAULT_SETTINGS: AppSettings = {
  save_path: '',
  save_format: 'ts',
  quality: '原画',
  delay_seconds: 120,
  split_enabled: true,
  split_seconds: 1800,
  convert_to_mp4: true,
  delete_origin: true,
  folder_by_author: true,
  folder_by_time: false,
  douyin_cookie: '',
  bilibili_cookie: '',
  proxy_addr: '',
  proxy_enabled: false,
  ffmpeg_path: '',
}

const FFMPEG_SOURCE_LABEL: Record<FfmpegSource, string> = {
  builtin: '内置版本',
  custom: '本机路径',
  installed: '应用内安装',
  path: '系统 PATH',
  none: '未找到',
}

function Row({
  label,
  hint,
  children,
}: {
  label: string
  hint?: string
  children: React.ReactNode
}) {
  return (
    <div className="grid gap-2 py-3 md:grid-cols-[220px_1fr] md:items-center md:gap-6">
      <div>
        <Label>{label}</Label>
      </div>
      <div className="space-y-1">
        {children}
        {hint && <div className="text-xs text-muted-foreground">{hint}</div>}
      </div>
    </div>
  )
}

interface MigrationCheck {
  changed: boolean
  old_root: string
  new_root: string
  file_count: number
  total_size: number
}

export default function SettingsPage() {
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS)
  const [original, setOriginal] = useState<AppSettings>(DEFAULT_SETTINGS)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [migration, setMigration] = useState<MigrationCheck | null>(null)
  const [migrating, setMigrating] = useState(false)
  const [ffmpeg, setFfmpeg] = useState<FfmpegStatus | null>(null)
  const [switching, setSwitching] = useState(false)
  const [installing, setInstalling] = useState(false)
  const [installProgress, setInstallProgress] = useState<FfmpegInstallProgress | null>(null)
  const { status } = useStatus()
  const recordingCount = status?.recording_files.length ?? 0

  const load = useCallback(async () => {
    const [settingsResult, statusResult] = await Promise.allSettled([
      api.getSettings(),
      api.getFfmpegStatus(),
    ])
    if (settingsResult.status === 'fulfilled') {
      setSettings({ ...DEFAULT_SETTINGS, ...settingsResult.value })
      setOriginal({ ...DEFAULT_SETTINGS, ...settingsResult.value })
    } else {
      const err = settingsResult.reason
      toast.error(err instanceof Error ? err.message : '加载设置失败')
    }
    if (statusResult.status === 'fulfilled') {
      setFfmpeg(statusResult.value)
    }
    setLoading(false)
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  useEffect(() => {
    let unlisten: (() => void) | null = null
    void listen<FfmpegInstallProgress>('ffmpeg://install-progress', (event) => {
      setInstallProgress(event.payload)
    }).then((fn) => {
      unlisten = fn
    })
    return () => unlisten?.()
  }, [])

  const set = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setSettings((prev) => ({ ...prev, [key]: value }))
  }

  const dirtyCount = useMemo(
    () =>
      (Object.keys(settings) as (keyof AppSettings)[]).filter(
        (key) => settings[key] !== original[key]
      ).length,
    [settings, original]
  )

  const pickFolder = async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog')
      const selected = await open({ directory: true, multiple: false })
      if (typeof selected === 'string') {
        set('save_path', selected)
      }
    } catch {
      toast.error('打开目录选择器失败')
    }
  }

  const syncFfmpegPath = (status: FfmpegStatus) => {
    // explicit paths (custom / installed) are persisted; builtin/PATH means auto
    const path =
      status.source === 'custom' || status.source === 'installed' ? status.path ?? '' : ''
    setSettings((prev) => ({ ...prev, ffmpeg_path: path }))
    setOriginal((prev) => ({ ...prev, ffmpeg_path: path }))
    setFfmpeg(status)
  }

  const useBuiltinFfmpeg = async () => {
    setSwitching(true)
    try {
      const status = await api.setFfmpegPath(null)
      syncFfmpegPath(status)
      toast.success('已切换为内置版本')
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '切换失败')
    } finally {
      setSwitching(false)
    }
  }

  const pickFfmpeg = async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog')
      const selected = await open({
        directory: false,
        multiple: false,
        title: '选择 ffmpeg 可执行文件',
        filters: /Windows/.test(navigator.userAgent)
          ? [{ name: 'FFmpeg', extensions: ['exe'] }]
          : undefined,
      })
      if (typeof selected !== 'string') return
      setSwitching(true)
      const status = await api.setFfmpegPath(selected)
      syncFfmpegPath(status)
      toast.success('已切换为指定的 FFmpeg')
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '选择 FFmpeg 失败')
    } finally {
      setSwitching(false)
    }
  }

  const installFfmpeg = async () => {
    setInstalling(true)
    setInstallProgress(null)
    try {
      const status = await api.installFfmpeg()
      syncFfmpegPath(status)
      toast.success(status.version ? `FFmpeg 安装完成：${status.version}` : 'FFmpeg 安装完成')
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '安装 FFmpeg 失败')
    } finally {
      setInstalling(false)
      setInstallProgress(null)
    }
  }

  const doSave = async () => {
    setSaving(true)
    try {
      const saved = await api.updateSettings(settings)
      setSettings({ ...DEFAULT_SETTINGS, ...saved })
      setOriginal({ ...DEFAULT_SETTINGS, ...saved })
      toast.success('设置已保存')
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '保存失败')
    } finally {
      setSaving(false)
    }
  }

  const save = async () => {
    if (dirtyCount === 0) {
      toast.info('没有需要保存的修改')
      return
    }
    try {
      // changing the save folder with existing videos triggers a migration offer
      const check = await api.checkSavePathChange(settings.save_path)
      if (check.changed && check.file_count > 0) {
        setMigration(check)
        return
      }
    } catch {
      // check failure should not block saving
    }
    await doSave()
  }

  const confirmMigration = async (migrate: boolean) => {
    const check = migration
    setMigration(null)
    if (!check) return
    if (migrate) {
      setMigrating(true)
      try {
        const result = await api.migrateSavePath(settings.save_path)
        toast.success(
          `迁移完成：移动 ${result.moved} 个文件${result.skipped ? `，跳过 ${result.skipped} 个同名文件` : ''}${
            result.failed ? `，失败 ${result.failed} 个` : ''
          }`
        )
        if (result.failed > 0) {
          toast.warning('部分文件迁移失败，原文件夹中仍保留这些文件')
        }
      } catch (err) {
        toast.error(err instanceof Error ? err.message : '迁移失败，设置未保存')
        setMigrating(false)
        return
      }
      setMigrating(false)
    }
    await doSave()
  }

  if (loading) {
    return <div className="py-20 text-center text-muted-foreground">加载设置中...</div>
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="text-sm text-muted-foreground">
          {dirtyCount > 0 ? (
            <span className="text-amber-600 dark:text-amber-400">有 {dirtyCount} 项未保存的修改</span>
          ) : (
            '修改设置后保存生效，正在进行的录制不受影响。'
          )}
        </div>
        <Button onClick={() => void save()} disabled={saving || dirtyCount === 0}>
          <Save />
          {saving ? '保存中...' : '保存设置'}
        </Button>
      </div>

      <Tabs defaultValue="basic" className="space-y-4">
        <TabsList className="flex-wrap">
          <TabsTrigger value="basic">基础录制</TabsTrigger>
          <TabsTrigger value="file">文件与分段</TabsTrigger>
          <TabsTrigger value="cookie">Cookie 与代理</TabsTrigger>
          <TabsTrigger value="ffmpeg">FFmpeg</TabsTrigger>
        </TabsList>

        <TabsContent value="basic">
          <Card>
            <CardContent className="divide-y p-6 pt-2">
              <Row
                label="视频保存路径"
                hint={
                  status?.save_path
                    ? `留空时保存到 ${status.save_path}`
                    : '留空则保存到应用数据目录下的 downloads 文件夹'
                }
              >
                <div className="flex max-w-md gap-2">
                  <Input
                    value={settings.save_path}
                    onChange={(e) => set('save_path', e.target.value)}
                    placeholder={status?.save_path ?? '使用应用默认目录'}
                  />
                  <Button variant="outline" size="icon" title="选择目录" onClick={() => void pickFolder()}>
                    <FolderOpen />
                  </Button>
                </div>
              </Row>
              <Row label="视频保存格式" hint="推荐 ts（录制完成后可自动转为 mp4）">
                <Select value={settings.save_format} onValueChange={(v) => set('save_format', v)}>
                  <SelectTrigger className="max-w-xs">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {['ts', 'mkv', 'flv', 'mp4'].map((fmt) => (
                      <SelectItem key={fmt} value={fmt}>
                        {fmt.toUpperCase()}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Row>
              <Row label="默认录制画质">
                <Select value={settings.quality} onValueChange={(v) => set('quality', v)}>
                  <SelectTrigger className="max-w-xs">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {['原画', '超清', '高清', '标清', '流畅'].map((q) => (
                      <SelectItem key={q} value={q}>
                        {q}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Row>
              <Row label="直播状态检测间隔（秒）" hint="每间隔轮询各直播间是否开播">
                <Input
                  type="number"
                  className="max-w-xs"
                  value={settings.delay_seconds}
                  onChange={(e) => set('delay_seconds', Number(e.target.value) || 120)}
                />
              </Row>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="file">
          <Card>
            <CardContent className="divide-y p-6 pt-2">
              <Row label="按主播分文件夹">
                <div className="flex h-9 items-center">
                  <Switch
                    checked={settings.folder_by_author}
                    onCheckedChange={(v) => set('folder_by_author', v)}
                  />
                </div>
              </Row>
              <Row label="按日期分文件夹">
                <div className="flex h-9 items-center">
                  <Switch
                    checked={settings.folder_by_time}
                    onCheckedChange={(v) => set('folder_by_time', v)}
                  />
                </div>
              </Row>
              <Row
                label="分段录制"
                hint="按时长切分录像文件，推荐开启以降低意外损坏的损失"
              >
                <div className="flex max-w-xs items-center gap-3">
                  <Switch checked={settings.split_enabled} onCheckedChange={(v) => set('split_enabled', v)} />
                  <Input
                    type="number"
                    className="w-32"
                    value={settings.split_seconds}
                    onChange={(e) => set('split_seconds', Number(e.target.value) || 1800)}
                    disabled={!settings.split_enabled}
                  />
                  <span className="text-sm text-muted-foreground">秒</span>
                </div>
              </Row>
              <Row label="录制完成后自动转为 mp4" hint="开启后浏览器/播放器里可直接播放">
                <div className="flex h-9 items-center">
                  <Switch checked={settings.convert_to_mp4} onCheckedChange={(v) => set('convert_to_mp4', v)} />
                </div>
              </Row>
              <Row label="转码后删除原始文件">
                <div className="flex h-9 items-center">
                  <Switch checked={settings.delete_origin} onCheckedChange={(v) => set('delete_origin', v)} />
                </div>
              </Row>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="cookie">
          <Card>
            <CardHeader>
              <CardTitle className="text-base">Cookie 与代理</CardTitle>
              <CardDescription>
                登录对应平台网页版后，从浏览器开发者工具复制 Cookie 粘贴到下面（抖音高画质建议填写）
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-6">
              <div className="space-y-1.5">
                <Label>抖音 Cookie</Label>
                <Textarea
                  rows={4}
                  className="font-mono text-xs"
                  value={settings.douyin_cookie}
                  onChange={(e) => set('douyin_cookie', e.target.value)}
                />
              </div>
              <div className="space-y-1.5">
                <Label>B站 Cookie</Label>
                <Textarea
                  rows={4}
                  className="font-mono text-xs"
                  value={settings.bilibili_cookie}
                  onChange={(e) => set('bilibili_cookie', e.target.value)}
                />
              </div>
              <Row label="使用代理" hint="TikTok 等海外平台需要代理（当前版本暂未收录海外平台）">
                <div className="flex h-9 items-center">
                  <Switch checked={settings.proxy_enabled} onCheckedChange={(v) => set('proxy_enabled', v)} />
                </div>
              </Row>
              <Row label="代理地址">
                <Input
                  className="max-w-md font-mono text-xs"
                  placeholder="http://127.0.0.1:7890 或 socks5://127.0.0.1:1080"
                  value={settings.proxy_addr}
                  onChange={(e) => set('proxy_addr', e.target.value)}
                  disabled={!settings.proxy_enabled}
                />
              </Row>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="ffmpeg">
          <Card>
            <CardHeader>
              <CardTitle className="text-base">FFmpeg 运行时</CardTitle>
              <CardDescription>
                录制、合并和转码都依赖 FFmpeg。完整版安装包已内置；精简版需要在这里安装或指定本机已有的版本。
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="rounded-lg border bg-muted/30 p-4">
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant={ffmpeg?.source === 'none' ? 'destructive' : 'secondary'}>
                    {ffmpeg ? FFMPEG_SOURCE_LABEL[ffmpeg.source] : '检测中...'}
                  </Badge>
                  <span className="break-all font-mono text-xs text-muted-foreground">
                    {ffmpeg?.path ?? '未找到可用的 FFmpeg'}
                  </span>
                </div>
                {ffmpeg?.version && (
                  <div className="mt-2 text-xs text-muted-foreground">{ffmpeg.version}</div>
                )}
              </div>

              <div className="flex flex-wrap gap-2">
                <Button
                  variant="outline"
                  disabled={!ffmpeg?.builtin_available || switching || installing}
                  onClick={() => void useBuiltinFfmpeg()}
                >
                  <Package />
                  使用内置版本
                </Button>
                <Button
                  variant="outline"
                  disabled={switching || installing}
                  onClick={() => void pickFfmpeg()}
                >
                  <FolderOpen />
                  使用本机路径
                </Button>
                <Button
                  disabled={!ffmpeg?.install_supported || installing}
                  onClick={() => void installFfmpeg()}
                >
                  <Download />
                  {installing ? '安装中...' : '安装 FFmpeg'}
                </Button>
              </div>

              {installing && (
                <div className="space-y-1.5">
                  <div className="h-2 w-full overflow-hidden rounded-full bg-muted">
                    <div
                      className="h-full bg-primary transition-all"
                      style={{
                        width: `${
                          installProgress?.total
                            ? Math.min(
                                100,
                                Math.round(
                                  (installProgress.downloaded / installProgress.total) * 100
                                )
                              )
                            : installProgress?.stage === 'extracting'
                              ? 100
                              : 5
                        }%`,
                      }}
                    />
                  </div>
                  <div className="text-xs text-muted-foreground">
                    {installProgress?.stage === 'extracting'
                      ? '下载完成，正在解压...'
                      : installProgress
                        ? `正在下载 ${formatBytesPublic(installProgress.downloaded)}${
                            installProgress.total
                              ? ` / ${formatBytesPublic(installProgress.total)}`
                              : ''
                          }`
                        : '准备下载...'}
                  </div>
                </div>
              )}

              <div className="space-y-1 text-xs text-muted-foreground">
                <p>不指定路径时，按“内置版本 → 应用内安装 → 系统 PATH”的顺序自动选择。</p>
                {ffmpeg && !ffmpeg.builtin_available && (
                  <p>当前安装包未内置 FFmpeg（精简版），请安装或指定本机路径。</p>
                )}
                {ffmpeg && !ffmpeg.install_supported && (
                  <p>当前平台不支持一键安装，请手动指定 FFmpeg 路径。</p>
                )}
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      {/* save-folder migration confirmation */}
      <Dialog
        open={!!migration}
        onOpenChange={(open) => {
          if (!open && !migrating) setMigration(null)
        }}
      >
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>原文件夹中有视频文件，是否迁移至新文件夹？</DialogTitle>
            <DialogDescription asChild>
              <div className="space-y-2 text-left">
                <div>
                  原文件夹 <code className="rounded bg-muted px-1 py-0.5 text-xs">{migration?.old_root}</code> 中有{' '}
                  <span className="font-semibold">{migration?.file_count}</span> 个视频文件（共{' '}
                  {migration ? formatBytesPublic(migration.total_size) : ''}）。
                </div>
                <div>
                  新文件夹 <code className="rounded bg-muted px-1 py-0.5 text-xs">{migration?.new_root}</code>
                  。迁移保留原有目录结构，同名文件将被跳过。
                </div>
                {recordingCount > 0 && (
                  <div className="rounded-md border border-amber-500/40 bg-amber-500/10 p-2 text-xs text-amber-700 dark:text-amber-400">
                    当前有 {recordingCount} 个录制正在进行：它们会继续写完当前文件（留在原文件夹），新录制将写入新文件夹。
                  </div>
                )}
              </div>
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" disabled={migrating} onClick={() => void confirmMigration(false)}>
              不迁移，直接保存
            </Button>
            <Button disabled={migrating} onClick={() => void confirmMigration(true)}>
              {migrating ? '迁移中...' : '迁移文件并保存'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function formatBytesPublic(bytes: number): string {
  if (!bytes) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / 1024 ** i).toFixed(i === 0 ? 0 : 1)} ${units[i]}`
}
