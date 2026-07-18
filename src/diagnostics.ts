export async function installNanaBetterCubismDiagnostics() {
  const { installAgentDebugHarness, isLiliaAgentDebugEnabled } = await import("./ui");
  if (!isLiliaAgentDebugEnabled()) return false;
  installAgentDebugHarness();
  return true;
}
