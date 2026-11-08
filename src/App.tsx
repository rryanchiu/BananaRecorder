import { useEffect } from "react";
import { HashRouter, Navigate, Route, Routes, useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { Layout } from "@/components/layout";
import Dashboard from "@/pages/dashboard";
import Rooms from "@/pages/rooms";
import Videos from "@/pages/videos";
import SettingsPage from "@/pages/settings";

/// Tray menu clicks emit `app://navigate` with a route — follow it here.
function TrayNavigator() {
  const navigate = useNavigate();
  useEffect(() => {
    const unlisten = listen<string>("app://navigate", (event) => {
      navigate(event.payload);
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [navigate]);
  return null;
}

export default function App() {
  return (
    <HashRouter>
      <TrayNavigator />
      <Layout>
        <Routes>
          <Route path="/" element={<Dashboard />} />
          <Route path="/rooms" element={<Rooms />} />
          <Route path="/videos" element={<Videos />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </Layout>
    </HashRouter>
  );
}
