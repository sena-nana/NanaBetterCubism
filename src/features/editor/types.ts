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
    officialApi: boolean;
    officialEditApi: boolean;
  };
  message: string;
  /** 最近一次参数结构握手失败时为 true（groups 仅作陈旧诊断）。 */
  structureStale: boolean;
  /** 成功校验的参数结构代数。 */
  structureGeneration: number;
}

export interface DomainCommandError {
  code: string;
  message: string;
}
