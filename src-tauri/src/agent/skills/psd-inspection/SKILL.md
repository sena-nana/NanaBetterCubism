---
name: psd-inspection
description: Read structure, layers, masks, blend modes, and per-layer pixels of PSD files the user explicitly attached to the conversation. Local parsing only; not Cubism Editor PSD operations.
---

# PSD Inspection

1. Only read PSD documents the user has explicitly attached to the current conversation. Never read other local files or arbitrary paths.
2. Call `read_psd_structure` first to get the layer tree, masks, blend modes, opacity, visibility, and bounds as JSON. This returns no pixel data.
3. Use `read_psd_layer_image` to extract a single layer's pixels as an image only when you actually need to see the layer's visual content. This tool requires a vision-capable model and is hidden when image input is unsupported.
4. Layer `id` values are stable indices into the PSD layer records; pass them back unchanged as `layerId`.
5. This skill parses local PSD files for context. It does not import, edit, or export PSD through Cubism Editor, and does not claim any Editor PSD capability.
