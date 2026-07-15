import { openUrl } from "@tauri-apps/plugin-opener";

export function normalizeExternalUrl(value: string): string | null {
  const trimmed = value.trim();
  if (!/^(https?:\/\/|mailto:)/i.test(trimmed)) return null;
  try {
    const url = new URL(trimmed);
    return ["http:", "https:", "mailto:"].includes(url.protocol) ? url.toString() : null;
  } catch {
    return null;
  }
}

export async function openExternalUrl(value: string): Promise<boolean> {
  const url = normalizeExternalUrl(value);
  if (!url) return false;
  await openUrl(url);
  return true;
}
