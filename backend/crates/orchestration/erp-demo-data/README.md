# erp-demo-data

本库只供已存在的受控演示数据入口与 CLI 索引装配调用。普通业务库及其测试不得依赖本库。

- 演示 Process 必须通过 `DemoRuntimePorts<A>` 接收完成装配的领域服务、资料命令、供给命令、采购责任命令、工作项合同及工作流授权和审计 Port。
- `A` 必须实现 `WorkflowAuthorizationPort`；具体 `WorkflowAuth` 与外域 adapter 只允许在应用组合根装配。
- `enabled` 必须沿用当前配置的演示开关。关闭时禁止生成或清理数据，不得以测试或迁移为由绕过。
- 生成、恢复、责任规则、发布审批及清理必须沿用现有命令身份、资格校验、逐步顺序与失败停止语义。
- 主数据 JSON、供给 JSON、岗位与审批 JSON、内嵌商品图片属于本库；修改数据必须同步执行对应库单元测试。
- `ensure_indexes` 只登记本库拥有的演示记录索引。其他集合与索引继续由其领域公开索引入口唯一拥有。

公开入口见 [src/lib.rs](src/lib.rs)，应用注入合同见 [src/runtime_ports.rs](src/runtime_ports.rs)。验证执行 [backend/AGENTS.md](../../../AGENTS.md)，禁止集成测试、ignored 测试、真实数据库及外部服务验收命令。
