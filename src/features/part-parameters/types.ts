import type { ParameterGroupSummary } from "../parameters/types";

export interface PartSelectionSummary {
  id: string;
  name: string;
}

export interface PartAssociatedObject {
  id: string;
  name: string;
  objectType: string;
  keyValues: number[];
  sourcePartIds: string[];
}

export interface PartAssociatedParameter {
  id: string;
  name: string;
  group: ParameterGroupSummary | null;
  keyValues: number[];
  objects: PartAssociatedObject[];
}

export interface PartParameterQueryResult {
  modelLabel: string;
  selectedParts: PartSelectionSummary[];
  ignoredSelectionCount: number;
  scannedObjectCount: number;
  parameters: PartAssociatedParameter[];
}
