# NanaBetterCubism

NanaBetterCubism 接入 Cubism Editor 稳定 External API 与 5.4 Alpha1 API 1.1.0 的完整官方函数表。Agent 可查询 Editor、控制临时参数缓冲、读取通知，并通过事务编辑参数、关键点、Part/Object 与 Deformer。

结构编辑必须先生成预览并获得确认；Rust 后端独占 WebSocket、会话 UID 与事务，在提交后回读验证。版本、模式、模型或编辑权限不满足要求时，工具返回真实不可用原因。

参见[开发与验证](./guide/development.md)。
