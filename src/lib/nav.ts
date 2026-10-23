export type ViewId = "feed" | "applications" | "resume" | "settings";

export const NAV_ITEMS: ReadonlyArray<{ id: ViewId; label: string; icon: string }> = [
  { id: "feed", label: "Job feed", icon: "work" },
  { id: "applications", label: "Applications", icon: "checklist" },
  { id: "resume", label: "Resume", icon: "description" },
  { id: "settings", label: "Settings", icon: "tune" },
];
