---
name: cubism-model-editing
description: Plan supported Cubism Editor model operations. Use for parameters, groups, keys, Parts, selection, hierarchy, parents, masks, display properties, Glue, Rotation or Warp Deformers, standardization, compatibility, or repair.
---

# Cubism Model Editing

## MUST

- MUST read the protocol capability matrix and inspect the active document, stable IDs, selection, hierarchy, parameters, and keys involved before editing.
  - Reason: 盲改会破坏层级与参数依赖。
- MUST express the desired state and compute a minimal idempotent diff. Reject ambiguous selections, duplicate IDs, invalid ranges, missing parents, cycles, stale documents, and unsupported object kinds.
  - Reason: 非最小 diff 会误改无关对象；非法结构会让 Editor 拒绝。
- MUST require explicit intent for deletion, duplicate-ID resolution, bulk reparenting, or other destructive normalization.
  - Reason: 破坏性归一化不可逆，需用户确认。
- MUST create groups/parameters and Parts/Deformers before keys, parenting, properties, and final ordering. Preserve unrelated model state.
  - Reason: 依赖对象必须先存在，否则子操作引用失败。
- MUST execute the complete diff through `$cubism-edit-transactions`, then re-read and verify the desired state.
  - Reason: 事务保证可取消可回滚，重读验证真实结果。

## MUST NOT

- MUST NOT infer mesh geometry, UVs, topology, textures, control points, animation, physics, save/export, atlas, or PSD support from object metadata.
  - Reason: 元数据不含这些能力，臆造会写入无法运行的代码。

## See also

- `$cubism-edit-transactions` for execution, `$cubism-editor-protocol` for capability gating.
