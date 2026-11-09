import { useCallback, useEffect, useMemo, useState } from 'react'
import { Check, CircleStop, Copy, Pencil, Plus, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { api, detectPlatform, SUPPORTED_PLATFORMS, type Room } from '@/lib/api'
import { useStatus } from '@/lib/status'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
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
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'

const QUALITIES = ['原画', '蓝光', '超清', '高清', '标清', '流畅']

interface RoomFormDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  room?: Room | null
  onSaved: () => void
}

function RoomFormDialog({ open, onOpenChange, room, onSaved }: RoomFormDialogProps) {
  const isEdit = !!room
  const [url, setUrl] = useState('')
  const [name, setName] = useState('')
  const [quality, setQuality] = useState('原画')
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    if (open) {
      setUrl(room?.url ?? '')
      setName(room?.name ?? '')
      setQuality(room?.quality ?? '原画')
    }
  }, [open, room])

  const submit = async () => {
    if (!url.trim()) {
      toast.error('请填写直播间链接')
      return
    }
    setSaving(true)
    try {
      if (isEdit && room) {
        await api.updateRoom(room.id, { url, name, quality })
        toast.success('直播间已更新')
      } else {
        await api.createRoom({ url, name, quality })
        toast.success('直播间已添加')
      }
      onOpenChange(false)
      onSaved()
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '保存失败')
    } finally {
      setSaving(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{isEdit ? '编辑直播间' : '添加直播间'}</DialogTitle>
          <DialogDescription>
            当前支持{SUPPORTED_PLATFORMS.join('、')}。抖音支持直播间链接、
            关注页链接（douyin.com/follow/live/...）、主播主页链接及 App 分享的
            v.douyin.com 短链（主页需主播设置了抖音号，否则建议粘贴直播间分享短链）。
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <div className="space-y-1.5">
            <Label htmlFor="room-url">直播间链接 *</Label>
            <Input
              id="room-url"
              placeholder="例如 https://live.douyin.com/123456789 或抖音主播主页链接"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="room-name">主播名称（可选，便于识别和归档）</Label>
            <Input
              id="room-name"
              placeholder="留空则自动使用主播昵称"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>
          <div className="space-y-1.5">
            <Label>录制画质</Label>
            <Select value={quality} onValueChange={setQuality}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {QUALITIES.map((q) => (
                  <SelectItem key={q} value={q}>
                    {q}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            取消
          </Button>
          <Button onClick={() => void submit()} disabled={saving}>
            {saving ? '保存中...' : '保存'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function DeleteRoomDialog({
  room,
  onOpenChange,
  onDeleted,
}: {
  room: Room | null
  onOpenChange: (open: boolean) => void
  onDeleted: () => void
}) {
  const confirmDelete = async () => {
    if (!room) return
    try {
      await api.deleteRoom(room.id)
      toast.success('已删除，对应的监控与录制任务已停止')
      onOpenChange(false)
      onDeleted()
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '删除失败')
    }
  }

  return (
    <Dialog open={!!room} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>删除直播间</DialogTitle>
          <DialogDescription>确定要删除「{room?.name || room?.url}」吗？已生成的录像文件不受影响。</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            取消
          </Button>
          <Button variant="destructive" onClick={() => void confirmDelete()}>
            删除
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    } catch {
      toast.error('复制失败')
    }
  }
  return (
    <Button variant="ghost" size="icon" className="h-7 w-7" onClick={() => void copy()} title="复制链接">
      {copied ? <Check className="h-3.5 w-3.5 text-emerald-500" /> : <Copy className="h-3.5 w-3.5" />}
    </Button>
  )
}

export default function Rooms() {
  const { status, refresh } = useStatus()
  const [rooms, setRooms] = useState<Room[]>([])
  const [loading, setLoading] = useState(true)
  const [formOpen, setFormOpen] = useState(false)
  const [editingRoom, setEditingRoom] = useState<Room | null>(null)
  const [deletingRoom, setDeletingRoom] = useState<Room | null>(null)

  const recorderRunning = status?.recorder.running ?? false
  // exact in-flight recording detection: the backend tags each recording with
  // its room id, no folder-name guessing
  const recordingRoomIds = useMemo(
    () => new Set((status?.recording_files ?? []).map((f) => f.room_id)),
    [status]
  )

  const load = useCallback(async () => {
    try {
      const data = await api.listRooms()
      setRooms(data.rooms)
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '加载直播间列表失败')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void load()
    const t = setInterval(() => void load(), 10000)
    return () => clearInterval(t)
  }, [load])

  const toggleRoom = async (room: Room, enabled: boolean) => {
    try {
      await api.updateRoom(room.id, { enabled })
      toast.success(enabled ? '已启用，开始监控' : '已停用，对应的监控与录制任务已停止')
      await load()
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '操作失败')
    }
  }

  const changeQuality = async (room: Room, quality: string) => {
    try {
      await api.updateRoom(room.id, { quality })
      toast.success('画质已保存，下次开播录制时生效')
      await load()
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '操作失败')
    }
  }

  const isRecording = (room: Room) => recordingRoomIds.has(room.id)

  const roomState = (
    room: Room
  ): { label: string; variant: 'success' | 'info' | 'secondary' | 'destructive' } => {
    if (isRecording(room)) return { label: '录制中', variant: 'success' }
    if (!room.enabled) return { label: '已停用', variant: 'secondary' }
    if (recorderRunning) return { label: '监控中', variant: 'info' }
    return { label: '待启动', variant: 'secondary' }
  }

  const stopRecording = async (room: Room) => {
    try {
      await api.stopRecording(room.id)
      toast.success('已停止录制并停用该直播间（重新打开开关即可恢复监控）')
      await load()
    } catch (err) {
      toast.error(err instanceof Error ? err.message : '停止失败')
    }
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="text-sm text-muted-foreground">
          共 {rooms.length} 个直播间，已启用 {rooms.filter((r) => r.enabled).length} 个。
          增删直播间立即生效；当前支持{SUPPORTED_PLATFORMS.join('、')}。
        </div>
        <Button
          onClick={() => {
            setEditingRoom(null)
            setFormOpen(true)
          }}
        >
          <Plus />
          添加直播间
        </Button>
      </div>

      <div className="rounded-xl border bg-card">
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead className="w-20 pl-4">启用</TableHead>
              <TableHead className="w-24">平台</TableHead>
              <TableHead>主播 / 链接</TableHead>
              <TableHead className="w-28">画质</TableHead>
              <TableHead className="w-24">状态</TableHead>
              <TableHead className="w-28 pr-4 text-right">操作</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {loading ? (
              <TableRow>
                <TableCell colSpan={6} className="h-32 text-center text-muted-foreground">
                  加载中...
                </TableCell>
              </TableRow>
            ) : rooms.length === 0 ? (
              <TableRow>
                <TableCell colSpan={6} className="h-32 text-center text-muted-foreground">
                  还没有添加直播间，点击右上角「添加直播间」开始使用
                </TableCell>
              </TableRow>
            ) : (
              rooms.map((room) => {
                const state = roomState(room)
                return (
                  <TableRow key={room.id}>
                    <TableCell className="pl-4">
                      <Switch
                        checked={room.enabled}
                        onCheckedChange={(v) => void toggleRoom(room, v)}
                      />
                    </TableCell>
                    <TableCell>
                      <Badge variant="outline">{detectPlatform(room.url)}</Badge>
                    </TableCell>
                    <TableCell>
                      <div className="min-w-0 truncate font-medium" title={room.name}>
                        {room.name || '（自动获取主播昵称）'}
                      </div>
                      <div className="flex items-center gap-1 text-xs text-muted-foreground">
                        <span className="max-w-xs truncate">{room.url}</span>
                        <CopyButton text={room.url} />
                      </div>
                    </TableCell>
                    <TableCell>
                      <Select value={room.quality} onValueChange={(q) => void changeQuality(room, q)}>
                        <SelectTrigger className="h-8 w-24 text-xs">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          {QUALITIES.map((q) => (
                            <SelectItem key={q} value={q}>
                              {q}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    </TableCell>
                    <TableCell>
                      <Badge variant={state.variant}>{state.label}</Badge>
                    </TableCell>
                    <TableCell className="pr-4 text-right">
                      {isRecording(room) && (
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-8 text-destructive"
                          title="停止该直播间的录制（同时停用监控）"
                          onClick={() => void stopRecording(room)}
                        >
                          <CircleStop />
                          停止录制
                        </Button>
                      )}
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-8 w-8"
                        title="编辑"
                        onClick={() => {
                          setEditingRoom(room)
                          setFormOpen(true)
                        }}
                      >
                        <Pencil className="h-4 w-4" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-8 w-8 text-destructive"
                        title="删除"
                        onClick={() => setDeletingRoom(room)}
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </TableCell>
                  </TableRow>
                )
              })
            )}
          </TableBody>
        </Table>
      </div>

      <RoomFormDialog
        open={formOpen}
        onOpenChange={setFormOpen}
        room={editingRoom}
        onSaved={() => {
          void load()
          void refresh()
        }}
      />
      <DeleteRoomDialog
        room={deletingRoom}
        onOpenChange={(open) => !open && setDeletingRoom(null)}
        onDeleted={() => {
          void load()
          void refresh()
        }}
      />
    </div>
  )
}
