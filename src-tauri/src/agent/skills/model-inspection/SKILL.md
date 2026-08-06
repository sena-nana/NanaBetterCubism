---
name: model-inspection
description: Inspect parameters, keys, selections, Parts, objects, Glue, ArtMesh properties, and Deformer hierarchies without changing model structure.
---

# Model Inspection

1. Call `get_editor_snapshot` first and stop when the required read capability, model, or document is unavailable.
2. For Part-related questions, ask the user to select the relevant Parts before calling `find_selected_part_parameters` when selection is ambiguous.
3. Resolve IDs through structure reads (`get_part_structure` / `get_deformer_structure`). Pass only exact `id` fields from those responses; never pass a display `name`, and never guess an ID from a name.
4. Read object properties with `get_objects` (known ID list) or `get_all_objects` (whole model). Do not call `get_object` one ID at a time for draw order, opacity, or other display properties.
5. Object reads expose documented properties only. Do not claim access to mesh geometry, UVs, topology, Warp control points, animation, physics editing, save/export, atlas, or PSD operations.
