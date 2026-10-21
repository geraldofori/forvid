import AppGate from "./components/AppGate";
import StatusBar from "./components/StatusBar";
import ThemeToggle from "./components/ThemeToggle";
import UpdateButton from "./components/UpdateButton";

export default function Home() {
  return (
    <main className="app-shell">
      <div className="top-bar-left">
        <UpdateButton />
      </div>
      <div className="top-bar">
        <ThemeToggle />
      </div>
      <AppGate />
      <StatusBar />
    </main>
  );
}
