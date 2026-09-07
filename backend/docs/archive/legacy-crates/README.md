# 旧三层历史档案合同

本目录保存领域 crate 迁移前的历史测试与旧文档。归档文件只用于追溯原业务断言和历史设计，不属于当前运行说明、活动源码或 Cargo target。

- `services/tests/`：13个历史测试源码。
- `database/tests/`：13个历史测试源码和1份历史测试说明。
- `entities/README.md`、`database/README.md`、`database/TRANSACTIONS.md`：3份旧层文档。

所有归档文件必须保持迁入时的原始字节；原路径、现路径、长度及SHA-256见 [manifest.json](manifest.json)。旧文档中的模块路径、示例和相对链接仅表示历史快照，不得据此恢复旧crate。

当前业务实现归属以 [backend/AGENTS.md](../../../AGENTS.md) 及 [领域迁移执行计划](../../superpowers/plans/domain-crate-migration/README.md) 为准。不得将本目录配置为workspace成员、显式测试目标或活动模块路径，也不得运行其中依赖真实数据库的测试。

需要复用历史断言时，应在拥有领域的现有内联测试中按当前公开合同移植必要断言，保留来源，执行相应单元测试。归档内容不能作为当前测试已通过的证据。
