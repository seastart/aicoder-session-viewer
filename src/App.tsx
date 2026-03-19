import { useEffect } from "react";
import "./App.css";
import { Layout } from "./components/Layout";
import { useSessionStore } from "./stores/sessionStore";
import { LocaleProvider } from "./i18n";

function App() {
  const fetchSessions = useSessionStore((s) => s.fetchSessions);

  // 启动时自动加载所有 session
  useEffect(() => {
    fetchSessions();
  }, [fetchSessions]);

  return (
    <LocaleProvider>
      <Layout />
    </LocaleProvider>
  );
}

export default App;
