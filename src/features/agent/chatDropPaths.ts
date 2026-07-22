export async function addDroppedChatPaths(
  paths: string[],
  handlers: {
    addImages: (paths: string[]) => Promise<string[]>;
    addPsds: (paths: string[]) => Promise<string[]>;
    setError: (message: string | null) => void;
  },
): Promise<void> {
  if (paths.length === 0) return;
  const images: string[] = [];
  const psds: string[] = [];
  for (const path of paths) {
    (path.toLowerCase().endsWith(".psd") ? psds : images).push(path);
  }
  const [imageErrors, psdErrors] = await Promise.all([
    images.length ? handlers.addImages(images) : Promise.resolve([]),
    psds.length ? handlers.addPsds(psds) : Promise.resolve([]),
  ]);
  const errors = [...imageErrors, ...psdErrors];
  handlers.setError(errors.length ? errors.join("\n") : null);
}
