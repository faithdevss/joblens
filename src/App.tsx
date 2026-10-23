import { useEffect, useState } from "react";
import type { AppInfo } from "./bindings/AppInfo";
import { Sidebar } from "./components/Sidebar";
import { useToast } from "./components/Toast";
import { ipc } from "./lib/ipc";
import type { ViewId } from "./lib/nav";
import { useAppData } from "./lib/useAppData";
import { ApplicationsView } from "./views/ApplicationsView";
import { FeedView } from "./views/FeedView";
import { ResumeView } from "./views/ResumeView";
import { SettingsView } from "./views/SettingsView";
import { SetupWizard } from "./views/SetupWizard";

export default function App() {
  const [view, setView] = useState<ViewId>("feed");
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [setupOpen, setSetupOpen] = useState(false);
  const toast = useToast();
  const data = useAppData();

  useEffect(() => {
    ipc
      .appInfo()
      .then(setInfo)
      .catch((err) => toast(`Couldn't open your data: ${err}`, "error"));
  }, [toast]);

  // First run: open setup until it's finished or skipped once.
  useEffect(() => {
    if (data.setupStatus === "pending") setSetupOpen(true);
  }, [data.setupStatus]);

  if (setupOpen && data.loaded) {
    return <SetupWizard data={data} onClose={() => setSetupOpen(false)} />;
  }

  return (
    <div className="shell">
      <Sidebar
        view={view}
        version={info?.version ?? "0.1.0"}
        onNavigate={setView}
        onQuickSetup={() => setSetupOpen(true)}
      />
      <main className="main">
        {view === "feed" && <FeedView />}
        {view === "applications" && <ApplicationsView />}
        {view === "resume" && <ResumeView data={data} />}
        {view === "settings" && (
          <SettingsView
            info={info}
            data={data}
            onEditProfile={() => setView("resume")}
            onRunSetup={() => setSetupOpen(true)}
          />
        )}
      </main>
    </div>
  );
}
