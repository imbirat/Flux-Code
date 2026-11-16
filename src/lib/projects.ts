import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useStore } from "./store";
import type { ProjectInfo } from "./types";

/**
 * Open the native folder picker and register the chosen directory as a
 * project. Returns an error message on failure, or null on success (the
 * project becomes the active one). Failures are also surfaced via the global
 * toast.
 */
export async function pickAndAddProject(): Promise<string | null> {
  const res = await openDialog({
    directory: true,
    multiple: false,
    title: "Add Project",
  }).catch(() => null);
  if (typeof res !== "string" || !res) return null; // user cancelled
  try {
    await useStore.getState().addProject(res);
    return null;
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    useStore.getState().setToast(msg);
    return msg;
  }
}

/** Stable, human identity for a project row: folder icon + basename. */
export function projectNameOrFallback(p: Pick<ProjectInfo, "name" | "path">): string {
  return p.name || p.path.split(/[\\/]/).filter(Boolean).pop() || p.path;
}
