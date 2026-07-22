---
name: model-inspection
description: Inspect parameters, keys, selections, Parts, objects, Glue, ArtMesh properties, and Deformer hierarchies without changing model structure.
---

# Model Inspection

1. Call `get_editor_snapshot` first and stop when the required read capability, model, or document is unavailable.
2. For Part-related questions, ask the user to select the relevant Parts before calling `find_selected_part_parameters` when selection is ambiguous.
3. Resolve IDs through structure reads. For `get_object`, pass only the exact `id` field returned by a structured response; never pass a display `name`, and never guess an ID from a name.
4. Object reads expose documented properties only. Do not claim access to mesh geometry, UVs, topology, Warp control points, animation, physics editing, save/export, atlas, or PSD operations.
