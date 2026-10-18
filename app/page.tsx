import Downloader from "./components/Downloader";
import StatusBar from "./components/StatusBar";
import ThemeToggle from "./components/ThemeToggle";

export default function Home() {
  return (
    <main className="app-shell">
      <div className="top-bar">
        <ThemeToggle />
      </div>
      <Downloader />
      <StatusBar />
    </main>
  );
}
