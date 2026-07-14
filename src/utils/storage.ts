export function readJsonStorage<T>(key: string, fallback: T): T {
  try {
    const stored = localStorage.getItem(key);
    return stored === null ? fallback : (JSON.parse(stored) as T);
  } catch {
    return fallback;
  }
}

export function writeJsonStorage(key: string, value: unknown): void {
  localStorage.setItem(key, JSON.stringify(value));
}

export function readIntegerStorage(key: string, fallback: number, min: number, max: number): number {
  const value = Number(localStorage.getItem(key) ?? fallback);
  return Number.isInteger(value) && value >= min && value <= max ? value : fallback;
}
