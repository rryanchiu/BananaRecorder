import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  ChevronRight,
  Download,
  ExternalLink,
  FileVideo,
  Folder,
  FolderOpen,
  Merge,
  Play,
  RefreshCw,
  Search,
  Trash2,
  Video as VideoIcon,
} from 'lucide-react'
import { toast } from 'sonner'
import { api, type VideoFile, type VideoListResponse } from '@/lib/api'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { formatBytes, formatTime } from '@/lib/utils'

// extensions the browser can play natively
const PLAYABLE_EXTS = new Set(['.mp4', '.m4v', '.mov', '.webm', '.m4a', '.mp3', '.aac'])

interface PlayerState {
  file: VideoFile
  playable: boolean
}

export default function Videos() {
  const [data, setData] = useState<VideoListResponse | null>(null)
  const [path, setPath] = useState('')
  const [loading, setLoading] = useState(true)
  const [keyword, setKeyword] = useState('')
  const [player, setPlayer] = useState<PlayerState | null>(null)
  const [deleting, setDeleting] = useState<VideoFile | null>(null)
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [batchDeleting, setBatchDeleting] = useState(false)
  const [batchBusy, setBatchBusy] = useState<'' | 'merge' | 'transcode'>('')

  const load = useCallback(async (targetPath: string) => {
    setLoading(true)
    try {
      const resp = await api.listVideos(targetPath)
      setData(resp)
      setPath(targetPath)
      setSelected(new Set())
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '加载视频列表失败')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void load('')
  }, [load])

  const saveAs = async (file: VideoFile) => {
    try {
      const { save } = await import('@tauri-apps/plugin-dialog')
      const target = await save({ defaultPath: file.name })
      if (!target) return
      await api.saveVideoAs(file.path, target)
      toast.success('已保存')
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '保存失败')
    }
  }

  const breadcrumbs = path ? path.split('/') : []

  const filteredFiles = (data?.files ?? []).filter((f) =>
    f.name.toLowerCase().includes(keyword.toLowerCase())
  )
  const filteredDirs = (data?.dirs ?? []).filter((d) =>
    d.name.toLowerCase().includes(keyword.toLowerCase())
  )

  // ----- selection helpers -----
  const allSelected =
    filteredFiles.length > 0 && filteredFiles.every((f) => selected.has(f.path))
  const toggleSelect = (path: string) => {
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return next
    })
  }
  const toggleSelectAll = () => {
    if (allSelected) {
      setSelected(new Set())
    } else {
      setSelected(new Set(filteredFiles.map((f) => f.path)))
    }
  }
  const selectedFiles = useMemo(
    () => (data?.files ?? []).filter((f) => selected.has(f.path)),
    [data, selected]
  )
  const selectedExts = useMemo(
    () => new Set(selectedFiles.map((f) => f.ext.toLowerCase())),
    [selectedFiles]
  )
  const mergeDisabled =
    selected.size < 2 || (selectedExts.size > 1 ? true : false)

  // ----- batch actions -----
  const confirmBatchDelete = async () => {
    setBatchDeleting(false)
    try {
      const result = await api.deleteVideos([...selected])
      toast.success(`已删除 ${result.deleted} 个文件${result.failed ? `，${result.failed} 个失败` : ''}`)
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '批量删除失败')
    }
    setSelected(new Set())
    void load(path)
  }

  const doMerge = async () => {
    setBatchBusy('merge')
    try {
      const result = await api.mergeVideos([...selected])
      toast.success(`已合并 ${result.count} 个视频：${result.output.split(/[\\/]/).pop()}`)
      setSelected(new Set())
      void load(path)
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '合并失败')
    } finally {
      setBatchBusy('')
    }
  }

  const doTranscode = async () => {
    setBatchBusy('transcode')
    try {
      const result = await api.transcodeVideos([...selected])
      toast.success(
        `批量转码完成：转码 ${result.converted} 个${result.skipped ? `，跳过 ${result.skipped} 个已是 mp4` : ''}${
          result.failed ? `，失败 ${result.failed} 个` : ''
        }`
      )
      setSelected(new Set())
      void load(path)
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '批量转码失败')
    } finally {
      setBatchBusy('')
    }
  }

  const confirmDelete = async () => {
    if (!deleting) return
    try {
      await api.deleteVideo(deleting.path)
      toast.success('已删除')
      setDeleting(null)
      void load(path)
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '删除失败')
    }
  }

  return (
    <div className="space-y-4">
      {/* batch action bar */}
      {selected.size > 0 && (
        <div className="flex flex-wrap items-center gap-2 rounded-xl border border-primary/30 bg-primary/5 px-4 py-2.5">
          <span className="text-sm font-medium">已选 {selected.size} 个视频</span>
          <div className="ml-auto flex items-center gap-2">
            <Button
              size="sm"
              variant="outline"
              disabled={mergeDisabled || batchBusy !== ''}
              title={
                selected.size < 2
                  ? '合并至少需要选择两个视频'
                  : selectedExts.size > 1
                    ? '所选视频格式不一致，请先批量转码'
                    : '按录制时间顺序无损拼接为一个文件（原文件保留）'
              }
              onClick={() => void doMerge()}
            >
              <Merge />
              {batchBusy === 'merge' ? '合并中...' : '合并视频'}
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={batchBusy !== ''}
              title="将所选视频无损转为 mp4（已是 mp4 的自动跳过）"
              onClick={() => void doTranscode()}
            >
              <FileVideo />
              {batchBusy === 'transcode' ? '转码中...' : '批量转码'}
            </Button>
            <Button
              size="sm"
              variant="destructive"
              disabled={batchBusy !== ''}
              onClick={() => setBatchDeleting(true)}
            >
              <Trash2 />
              批量删除
            </Button>
            <Button
              size="sm"
              variant="ghost"
              disabled={batchBusy !== ''}
              onClick={() => setSelected(new Set())}
            >
              取消选择
            </Button>
          </div>
        </div>
      )}

      {/* toolbar */}
      <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
        <div className="flex items-center gap-1 text-sm">
          <Button variant="ghost" size="sm" className="h-8 px-2" onClick={() => void load('')}>
            全部
          </Button>
          {breadcrumbs.map((seg, i) => {
            const segPath = breadcrumbs.slice(0, i + 1).join('/')
            const isLast = i === breadcrumbs.length - 1
            return (
              <span key={segPath} className="flex items-center">
                <ChevronRight className="h-3.5 w-3.5 text-muted-foreground" />
                <Button
                  variant={isLast ? 'secondary' : 'ghost'}
                  size="sm"
                  className="h-8 max-w-48 truncate px-2"
                  onClick={() => void load(segPath)}
                  title={seg}
                >
                  {seg}
                </Button>
              </span>
            )
          })}
        </div>
        <div className="flex items-center gap-2">
          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              placeholder="搜索文件或文件夹"
              className="h-9 w-56 pl-8"
              value={keyword}
              onChange={(e) => setKeyword(e.target.value)}
            />
          </div>
          <Button variant="outline" size="icon" title="刷新" onClick={() => void load(path)}>
            <RefreshCw />
          </Button>
          <Button
            variant="outline"
            onClick={() =>
              void api
                .openVideoFolder(path)
                .then(() => toast.success('已在文件管理器中打开'))
                .catch((err) => toast.error(err.message))
            }
          >
            <ExternalLink />
            打开文件夹
          </Button>
        </div>
      </div>

      {/* folders and files */}
      <div className="rounded-xl border bg-card">
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead className="w-10 pl-4">
                <input
                  type="checkbox"
                  className="h-4 w-4 cursor-pointer accent-primary"
                  checked={allSelected}
                  onChange={toggleSelectAll}
                  title="全选本目录视频"
                />
              </TableHead>
              <TableHead className="pl-2">文件名</TableHead>
              <TableHead className="w-24">大小</TableHead>
              <TableHead className="w-44">录制时间</TableHead>
              <TableHead className="w-56 pr-4 text-right">操作</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {loading ? (
              <TableRow>
                <TableCell colSpan={5} className="h-32 text-center text-muted-foreground">
                  加载中...
                </TableCell>
              </TableRow>
            ) : filteredDirs.length === 0 && filteredFiles.length === 0 ? (
              <TableRow>
                <TableCell colSpan={5} className="h-32 text-center text-muted-foreground">
                  <div className="flex flex-col items-center gap-2">
                    <FolderOpen className="h-8 w-8 opacity-40" />
                    {keyword.trim() ? '没有匹配的文件或文件夹' : '当前目录为空'}
                  </div>
                </TableCell>
              </TableRow>
            ) : (
              <>
                {filteredDirs.map((dir) => (
                  <TableRow
                    key={dir.path}
                    className="cursor-pointer"
                    tabIndex={0}
                    onClick={() => void load(dir.path)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter') void load(dir.path)
                    }}
                  >
                    <TableCell className="pl-4">
                      <Folder className="h-4 w-4 text-primary" />
                    </TableCell>
                    <TableCell className="pl-2">
                      <div className="flex items-center gap-3">
                        <span className="max-w-md truncate font-medium" title={dir.name}>
                          {dir.name}
                        </span>
                        <span className="text-xs text-muted-foreground">
                          {dir.file_count} 个视频
                        </span>
                      </div>
                    </TableCell>
                    <TableCell>{formatBytes(dir.total_size)}</TableCell>
                    <TableCell className="text-muted-foreground">—</TableCell>
                    <TableCell className="pr-4 text-right">
                      <ChevronRight className="ml-auto h-4 w-4 text-muted-foreground" />
                    </TableCell>
                  </TableRow>
                ))}
                {filteredFiles.map((file) => {
                const playable = PLAYABLE_EXTS.has(file.ext)
                const checked = selected.has(file.path)
                return (
                  <TableRow key={file.path}>
                    <TableCell className="pl-4">
                      <input
                        type="checkbox"
                        className="h-4 w-4 cursor-pointer accent-primary"
                        checked={checked}
                        onChange={() => toggleSelect(file.path)}
                      />
                    </TableCell>
                    <TableCell className="pl-2">
                      <div className="flex items-center gap-3">
                        <VideoIcon className="h-4 w-4 shrink-0 text-muted-foreground" />
                        <span className="max-w-md truncate font-medium" title={file.name}>
                          {file.name}
                        </span>
                        {file.ext === '.ts' && <Badge variant="warning">待转码</Badge>}
                      </div>
                    </TableCell>
                    <TableCell>{formatBytes(file.size)}</TableCell>
                    <TableCell className="text-muted-foreground">{formatTime(file.mtime)}</TableCell>
                    <TableCell className="pr-4 text-right">
                      {playable ? (
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-8"
                          onClick={() => setPlayer({ file, playable })}
                        >
                          <Play />
                          播放
                        </Button>
                      ) : (
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-8"
                          onClick={() => setPlayer({ file, playable: false })}
                          title="浏览器不支持直接播放该格式，可下载后播放"
                        >
                          <Play />
                          查看
                        </Button>
                      )}
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-8"
                        onClick={() => void saveAs(file)}
                      >
                        <Download />
                        下载
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-8 w-8 text-destructive"
                        title="删除文件"
                        onClick={() => setDeleting(file)}
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </TableCell>
                  </TableRow>
                )
                })}
              </>
            )}
          </TableBody>
        </Table>
      </div>

      {/* player dialog */}
      <Dialog open={!!player} onOpenChange={(open) => !open && setPlayer(null)}>
        <DialogContent className="sm:max-w-3xl">
          <DialogHeader>
            <DialogTitle className="max-w-md truncate pr-6 text-base">{player?.file.name}</DialogTitle>
          </DialogHeader>
          {player && player.playable ? (
            <video
              key={player.file.path}
              src={api.videoSrc(player.file.path)}
              controls
              autoPlay
              className="max-h-[70vh] w-full rounded-lg bg-black"
            />
          ) : player && !player.playable ? (
            <div className="flex flex-col items-center gap-3 rounded-lg border border-dashed p-8 text-center text-sm text-muted-foreground">
              <VideoIcon className="h-10 w-10 opacity-40" />
              <div>
                播放器不支持直接播放 <code>{player?.file.ext}</code> 格式。
                <br />
                可下载后在本地播放器观看，或在设置中开启「录制完成后自动转为 mp4 格式」。
              </div>
              <Button variant="outline" onClick={() => void saveAs(player.file)}>
                <Download />
                另存为
              </Button>
            </div>
          ) : null}
        </DialogContent>
      </Dialog>

      {/* delete confirm */}
      <Dialog open={!!deleting} onOpenChange={(open) => !open && setDeleting(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>删除视频文件</DialogTitle>
          </DialogHeader>
          <p className="text-sm text-muted-foreground">
            确定要删除「{deleting?.name}」（{deleting ? formatBytes(deleting.size) : ''}）吗？此操作不可恢复。
          </p>
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={() => setDeleting(null)}>
              取消
            </Button>
            <Button variant="destructive" onClick={() => void confirmDelete()}>
              删除
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      {/* batch delete confirm */}
      <Dialog open={batchDeleting} onOpenChange={(open) => !open && setBatchDeleting(false)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>批量删除视频</DialogTitle>
          </DialogHeader>
          <p className="text-sm text-muted-foreground">
            确定要删除选中的 {selected.size} 个视频文件吗？此操作不可恢复。
          </p>
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={() => setBatchDeleting(false)}>
              取消
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                setBatchDeleting(false)
                void confirmBatchDelete()
              }}
            >
              全部删除
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}
