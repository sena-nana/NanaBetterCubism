const MAX_MERMAID_SOURCE_LENGTH = 20_000;
export const MERMAID_MANUAL_ACTIVATION_LENGTH = 1_200;

let loader: Promise<typeof import("mermaid").default> | null = null;
let initialized = false;
let sequence = 0;

export function validateMermaidSource(source: string) {
  if (!source.trim()) throw new Error("图表内容为空。");
  if (source.length > MAX_MERMAID_SOURCE_LENGTH) {
    throw new Error("图表内容过长，已保留原始图源。");
  }
}

async function loadMermaid() {
  loader ??= import("mermaid").then((module) => module.default);
  const mermaid = await loader;
  if (!initialized) {
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      theme: "base",
      themeVariables: {
        background: "transparent",
        fontFamily: "var(--font-sans)",
        primaryColor: "transparent",
        primaryTextColor: "currentColor",
        lineColor: "currentColor",
        textColor: "currentColor",
      },
    });
    initialized = true;
  }
  return mermaid;
}

export async function renderMermaid(source: string) {
  validateMermaidSource(source);
  const mermaid = await loadMermaid();
  const id = `nana-mermaid-${Date.now()}-${sequence++}`;
  return (await mermaid.render(id, source)).svg;
}
