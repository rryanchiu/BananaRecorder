import { StrictMode } from 'react'
import { cleanup, render, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { LivePreview } from './live-preview'

const { invoke, attachMedia, destroy } = vi.hoisted(() => ({
  invoke: vi.fn(), attachMedia: vi.fn(), destroy: vi.fn(),
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke, convertFileSrc: (p: string) => `http://asset.localhost/${p}` }))
vi.mock('hls.js', () => ({ default: class {
  static isSupported() { return true }
  static Events = { ERROR: 'error', MANIFEST_PARSED: 'parsed' }
  on() {}
  loadSource() {}
  attachMedia = attachMedia
  destroy = destroy
} }))
beforeEach(() => {
  vi.clearAllMocks()
  vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {})
  vi.spyOn(HTMLMediaElement.prototype, 'load').mockImplementation(() => {})
})
afterEach(() => { cleanup(); vi.restoreAllMocks() })

it('opens the Portal in StrictMode and attaches only to a mounted video', async () => {
  invoke.mockImplementation((command: string) => Promise.resolve(command === 'start_preview' ? { id: 'session', playlist: 'preview/live.m3u8' } : undefined))
  const view = render(<StrictMode><LivePreview roomId="room" anchor="主播" onClose={() => {}} /></StrictMode>)
  await waitFor(() => expect(attachMedia).toHaveBeenCalled())
  expect(attachMedia.mock.calls[0][0]).toBeInstanceOf(HTMLVideoElement)
  expect(view.getByRole('dialog')).toBeTruthy()
  view.unmount()
  expect(destroy).toHaveBeenCalled()
  expect(invoke).toHaveBeenCalledWith('stop_preview', { id: 'session' })
})

it('releases a late preview session when closed before startup finishes', async () => {
  let resolveStart!: (value: unknown) => void
  invoke.mockImplementation((command: string) => command === 'start_preview' ? new Promise((resolve) => { resolveStart = resolve }) : Promise.resolve())
  const view = render(<LivePreview roomId="room" anchor="主播" onClose={() => {}} />)
  await waitFor(() => expect(resolveStart).toBeTypeOf('function'))
  view.unmount()
  resolveStart({ id: 'late', playlist: 'preview/live.m3u8' })
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('stop_preview', { id: 'late' }))
  expect(attachMedia).not.toHaveBeenCalled()
})
