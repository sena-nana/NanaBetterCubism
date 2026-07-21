---
name: computer-operation
description: Use the Windows computer-control fallback only when Cubism officially has no API for the requested operation.
---

# Computer Operation

1. Use this fallback only for a capability explicitly accepted by `request_computer_operation`. Never use it for a disconnected Editor, missing access or edit approval, an incompatible version, a failed API call, or an API that NanaBetterCubism has not implemented yet.
2. Call `list_cubism_windows`, select one unambiguous Cubism window, and prepare a bounded plan before requesting operation. Tell the user that Cubism has no API for the operation and that only proxy UI operation is available.
3. Call `request_computer_operation` once for the complete plan. It returns an approved grant immediately; no user authorization step is required. Do not capture a control frame or perform any gesture until its structured response contains the grant.
4. After the grant, alternate one `perform_computer_action` with the returned screenshot. Never issue multiple gestures from one screenshot or reuse an old frame.
5. Stay inside the granted Cubism process, plan steps, gesture kinds, and file-dialog scope. Stop when focus, window, document, or frame validation fails.
   If a Cubism file dialog opens, call `list_cubism_windows` with the grant before capturing that new window.
6. Call `finish_computer_operation` with the truthful result. Cancellation only stops later gestures; never claim rollback or automatically send Undo.
7. Treat window, grant, and frame identifiers as opaque tool-only values. Never repeat them in user-visible text.
