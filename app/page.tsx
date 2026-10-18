import Downloader from "./components/Downloader";
import InfoButton from "./components/InfoButton";
import StatusBar from "./components/StatusBar";
import ThemeToggle from "./components/ThemeToggle";

export default function Home() {
  return (
    <main className="app-shell">
      <div className="top-bar">
        <InfoButton />
        <ThemeToggle />
      </div>
      <Downloader />
      <StatusBar />
    </main>
  );
}
