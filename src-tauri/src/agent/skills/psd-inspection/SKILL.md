---
name: psd-inspection
description: Read structure, layers, masks, blend modes, and per-layer pixels of PSD files the user explicitly attached to the conversation. Local parsing only; not Cubism Editor PSD operations.
---

# PSD Inspection

1. Only read PSD documents the user has explicitly attached to the current conversation. Never read other local files or arbitrary paths.
2. The system context includes a compact snapshot of the current conversation's PSD attachments. Use its document `id` with PSD tools; it does not contain the full layer tree.
3. Call `list_attached_psds` before claiming that no PSD is attached, when the attachment snapshot may have changed, or after resuming from user interaction.
4. Call `read_psd_structure` with the selected PSD document `id` to get the layer tree, masks, blend modes, opacity, visibility, and bounds as JSON. This returns no pixel data.
5. Use `read_psd_layer_image` to extract a single layer's pixels as an image only when you actually need to see the layer's visual content. This tool requires a vision-capable model and is hidden when image input is unsupported.
6. Layer `id` values are stable indices into the PSD layer records; pass them back unchanged as `layerId`.
7. This skill parses local PSD files for context. It does not import, edit, or export PSD through Cubism Editor, and does not claim any Editor PSD capability.
