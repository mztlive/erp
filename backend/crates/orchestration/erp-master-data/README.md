# erp-master-data

本库负责主体、客户与供应商资料、商品字典、仓库策略、合同文件登记及责任交接的跨域命令。领域实体、集合与索引继续由各拥有领域管理。

- 应用必须在调用前装配领域 Service、Access 与当前身份资格服务；本库不得构造具体跨域 DataScope 或外部协议 adapter。
- 客户资料写入必须通过 `CustomerProfileProcess::with_access` 注入授权源。供应商资料写入必须通过 `SupplierProfileProcess::with_authorization` 注入相同请求上下文的身份服务与供应商 Access。
- 商品、供给责任交接必须通过 `CatalogHandoverPorts`、`OfferingHandoverPorts` 注入范围与写入服务。两者必须采用同一授权来源。
- 多集合命令只在公开命令入口开启事务；授权重验、资料写入、独立命令回执与成功审计共用同一 Executor。
- 文件资产批次复用 `erp-file-assets`；未知提交保留原错误与来源，禁止自动重放。

公开入口见 [src/lib.rs](src/lib.rs)。边界与验证规则按 [backend/AGENTS.md](../../../AGENTS.md) 执行；测试仅执行库单元测试。
