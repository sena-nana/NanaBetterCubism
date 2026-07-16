export async function installNanaBetterCubismDiagnostics() {
  const diagnostics = await import("@lilia/ui/diagnostics");
  if (!diagnostics.isLiliaAgentDebugEnabled()) return false;
  diagnostics.installAgentDebugHarness();
  return true;
}
