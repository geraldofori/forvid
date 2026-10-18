import Downloader from "./components/Downloader";
import StatusBar from "./components/StatusBar";

export default function Home() {
  return (
    <main className="app-shell">
      <Downloader />
      <StatusBar />
    </main>
  );
}
