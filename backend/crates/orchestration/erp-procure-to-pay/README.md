# erp-procure-to-pay

本 crate 负责采购、采购责任与询价费用采用的跨领域命令与事务协调。

- 必须通过拥有领域的 Service、Repository 与 Port 执行业务步骤。
- 事务以下步骤必须复用调用方的 `Executor`；外部 I/O 必须位于事务之外。
- 权限、原始错误分类、冻结身份、命令回执与未知提交恢复合同必须保持。
- 应用负责装配共享具体 adapter；本 crate 不得依赖应用入口或供应商协议实现。
- 开发验证执行 `cargo test -p erp-procure-to-pay --lib` 与 `cargo clippy -p erp-procure-to-pay --all-targets`；禁止真实服务验收例程。
