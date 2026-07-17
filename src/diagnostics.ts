export async function installNanaBetterCubismDiagnostics() {
  const { installAgentDebugHarness, isLiliaAgentDebugEnabled } = await import("@lilia/ui");
  if (!isLiliaAgentDebugEnabled()) return false;
  installAgentDebugHarness();
  return true;
}
