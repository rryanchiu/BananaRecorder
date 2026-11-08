import { createContext, useContext, useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { api, type StatusResponse } from '@/lib/api'

interface StatusContextValue {
  status: StatusResponse | null
  connected: boolean
  refresh: () => Promise<void>
}

const StatusContext = createContext<StatusContextValue>({
  status: null,
  connected: false,
  refresh: async () => {},
})

export function useStatus() {
  return useContext(StatusContext)
}

export function StatusProvider({ children }: { children: React.ReactNode }) {
  const [status, setStatus] = useState<StatusResponse | null>(null)
  const [connected, setConnected] = useState(false)

  const refresh = async () => {
    try {
      setStatus(await api.getStatus())
      setConnected(true)
    } catch {
      setConnected(false)
    }
  }

  useEffect(() => {
    void refresh()
    // the Rust side pushes aggregated status every few seconds
    const unlisten = listen<StatusResponse>('recorder://status', (event) => {
      setStatus(event.payload)
      setConnected(true)
    })
    return () => {
      void unlisten.then((fn) => fn())
    }
  }, [])

  return (
    <StatusContext.Provider value={{ status, connected, refresh }}>
      {children}
    </StatusContext.Provider>
  )
}
