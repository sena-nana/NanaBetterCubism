export type EditorConnectionState =
  | "disconnected"
  | "connecting"
  | "awaiting_access"
  | "awaiting_edit_permission"
  | "ready"
  | "editing"
  | "cancelling"
  | "incompatible"
  | "failed";

export interface ParameterGroupSummary {
  id: string;
  name: string;
}

export interface EditorSnapshot {
  state: EditorConnectionState;
  port: number;
  apiVersion: string | null;
  modelLabel: string | null;
  groups: ParameterGroupSummary[];
  capabilities: {
    batchCreateParameters: boolean;
    findPartParameters: boolean;
  };
  message: string;
}

export interface DomainCommandError {
  code: string;
  message: string;
}
