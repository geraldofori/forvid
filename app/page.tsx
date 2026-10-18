import AppGate from "./components/AppGate";
import StatusBar from "./components/StatusBar";
import ThemeToggle from "./components/ThemeToggle";

export default function Home() {
  return (
    <main className="app-shell">
      <div className="top-bar">
        <ThemeToggle />
      </div>
      <AppGate />
      <StatusBar />
    </main>
  );
}
