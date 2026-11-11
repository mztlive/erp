# 供应目录与供给编排

本库执行供应目录采集、来源资料同步、明确选品、供给治理、供应商门户、商品回调和相关集成治理命令。

- 领域实体、集合、索引和资格规则必须由 `domains/` 中的拥有领域管理。
- 组合根必须注入 `wiring::CatalogServices` 中实际装配的商品、供给和供应商服务及对象授权。业务库不得创建共享身份范围 adapter。
- 正式任务操作通过 `wiring::CatalogWorkItems` 执行。应用可注入已经装配授权、对象事实和审计 Port 的 `WorkItemService`。
- `supplier_portal_material:read/review` 仅用于内部材料的完整来源资格检查，不登记 HTTP 路由或角色权限。调用方必须先证明任务、申请或素材入口资格；材料政策不得替代该入口权限。供应商、供给的功能资格和范围分别通过 identity 独立检查。
- 外部商品协议必须通过 `erp-supply` 定义的网关、协议接口和运行时注册表调用；本库不得依赖具体供应商协议 crate。
- 外部商品和媒体读取必须位于数据库事务之外。来源观察、正式供给、公司资料与检查点必须沿用同一事务执行器提交。
- 消息消费者必须通过应用注入的消息目录 Port 调用 `supplier_connection_execution::CatalogMessageWriter`；本库不得负责消息领取、原消息确认或订单状态写入。
- 来源媒体的文件资产批次通过 `erp-file-assets` 转换为商品附件 Port，资产持久化与审计必须使用原业务事务。
- `background/` 只实现本库业务任务 adapter；轮询、退出、预算和 drain 机制必须使用 `background-runtime`。

修改后在 `backend/` 执行受影响库单元测试及 Clippy；依赖变化必须同步 `scripts/crate-layers.json` 并执行领域边界门禁。禁止执行真实数据库、真实供应商服务或集成验收例程。
