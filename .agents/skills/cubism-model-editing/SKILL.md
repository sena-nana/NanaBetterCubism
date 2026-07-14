---
name: cubism-model-editing
description: Plan supported Cubism Editor model operations. Use for parameters, groups, keys, Parts, selection, hierarchy, parents, masks, display properties, Glue properties, Rotation or Warp Deformers, standardization, compatibility checks, or repair workflows.
---

# Cubism Model Editing

1. Read the protocol capability matrix and inspect the active document, stable IDs, selection, hierarchy, parameters, and keys involved.
2. Express the desired state and compute a minimal idempotent diff. Reject ambiguous selections, duplicate IDs, invalid ranges, missing parents, cycles, stale documents, and unsupported object kinds.
3. Require explicit intent for deletion, duplicate-ID resolution, bulk reparenting, or other destructive normalization.
4. Create groups/parameters and Parts/Deformers before keys, parenting, properties, and final ordering. Preserve unrelated model state.
5. Execute the complete diff through `$cubism-edit-transactions`, then re-read and verify the desired state.
6. Never infer mesh geometry, UVs, topology, textures, control points, animation, physics, save/export, atlas, or PSD support from object metadata.
