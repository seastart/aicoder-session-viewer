import { useEffect } from "react";
import "./App.css";
import { Layout } from "./components/Layout";
import { useSessionStore } from "./stores/sessionStore";

function App() {
  const fetchSessions = useSessionStore((s) => s.fetchSessions);

  // 启动时自动加载所有 session
  useEffect(() => {
    fetchSessions();
  }, [fetchSessions]);

  return <Layout />;
}

export default App;
