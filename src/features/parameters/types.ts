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

export interface IdTemplateConfig {
  template: string;
  prefix: string;
  suffix: string;
  startIndex: number;
  indexWidth: number;
}

export type BatchGroupSelection =
  | { kind: "root" }
  | { kind: "existing"; id: string }
  | { kind: "new"; id: string; name: string };

export type RowGroupSelection =
  | { kind: "root" }
  | { kind: "existing"; id: string };

export interface ParameterDefaults {
  min: number;
  default: number;
  max: number;
  isBlendShape: boolean;
  isRepeat: boolean;
  group: BatchGroupSelection;
}

export interface ParameterRowOverrides {
  min?: number;
  default?: number;
  max?: number;
  isBlendShape?: boolean;
  isRepeat?: boolean;
  group?: RowGroupSelection;
}

export interface ParameterInputRow {
  clientId: string;
  name: string;
  key: string;
  side: string;
  overrides: ParameterRowOverrides;
}

export interface ParameterBatchInput {
  idTemplate: IdTemplateConfig;
  defaults: ParameterDefaults;
  rows: ParameterInputRow[];
}

export interface ValidationIssue {
  code: string;
  message: string;
  rowId: string | null;
  field: string | null;
}

export interface ParameterPreviewRow {
  clientId: string;
  name: string;
  id: string;
  groupId: string | null;
  groupLabel: string;
  min: number;
  default: number;
  max: number;
  isBlendShape: boolean;
  isRepeat: boolean;
}

export interface ParameterBatchPreview {
  previewId: string | null;
  modelLabel: string;
  rows: ParameterPreviewRow[];
  newGroup: ParameterGroupSummary | null;
  errors: ValidationIssue[];
  canExecute: boolean;
}

export type BatchPhase =
  | "validating"
  | "beginning"
  | "creating_group"
  | "creating_parameters"
  | "committing"
  | "verifying"
  | "cancelling";

export interface BatchProgress {
  operationId: string;
  phase: BatchPhase;
  completed: number;
  total: number;
  currentId: string | null;
}

export type BatchOutcome =
  | "committed"
  | "cancelled_rolled_back"
  | "failed_rolled_back"
  | "failed"
  | "unknown";

export interface BatchFinished {
  operationId: string;
  outcome: BatchOutcome;
  createdIds: string[];
  message: string;
}

export interface OperationAccepted {
  operationId: string;
}

export interface DomainCommandError {
  code: string;
  message: string;
}

export interface IdPreset extends IdTemplateConfig {
  id: string;
  name: string;
  builtIn?: boolean;
}
