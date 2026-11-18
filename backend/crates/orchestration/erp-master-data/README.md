# erp-master-data

## 职责

本库是主数据写入的编排层：负责主体、客户和供应商资料、商品字典、仓库 SKU 策略、责任交接及合同文件登记。它把一次业务命令涉及的授权、领域写入和成功审计组织在一起，不接管领域实体、规则、集合和索引。

- `erp-party` 拥有主体身份、名称修订及联系、地址、税务和银行资料；`erp-customer` 拥有客户账户、负责人分配和客户资料命令回执；`erp-supplier` 拥有供应商账户、商业档案、能力、资质、评级及其命令回执。
- `erp-catalog` 拥有商品、SKU 和字典，`erp-supply` 拥有供应商针对 SKU 的供给；两者分别保存各自的维护归属和交接回执。`erp-warehouse` 拥有仓库 SKU 策略，策略不是库存余额。
- `erp-contract` 拥有合同身份、不可变修订及 PDF 关联，`erp-support` 拥有文件资产元数据，`erp-audit` 拥有审计记录；文件字节由对象存储保存。审计记录不能替代业务回执。

客户资料详情必须调用 [`erp-read-models::customer_center::profile::CustomerProfileReadService`](../../read-models/erp-read-models/src/customer_center/profile/mod.rs)，由它在授权快照中组合客户、主体、归属和账号名称，交付前复验凭证。客户资料写入仍走 `CustomerProfileProcess`；该 Process 的命令结果回读不是资料详情查询。本库不依赖 `erp-read-models`。

## 目录结构

公开入口见 [src/lib.rs](src/lib.rs)，依赖声明见 [Cargo.toml](Cargo.toml)。

| 源码 | 用途 |
| --- | --- |
| [`customer_profile/mod.rs`](src/customer_profile/mod.rs) | 客户资料建档、修订、字段授权和幂等回读的根 Process。 |
| [`supplier_profile/mod.rs`](src/supplier_profile/mod.rs) | 供应商资料建档、修订、资质附件、逐行导入，以及供应商维护人和能力负责人交接。 |
| [`party.rs`](src/party.rs)、[`customer.rs`](src/customer.rs)、[`supplier.rs`](src/supplier.rs) | 主体、客户和供应商删除命令及同事务审计。 |
| [`catalog.rs`](src/catalog.rs)、[`warehouse.rs`](src/warehouse.rs) | 创建计量单位、商品分类、SKU 属性和属性值；创建仓库 SKU 策略。 |
| [`product_handover.rs`](src/product_handover.rs)、[`offering_handover.rs`](src/offering_handover.rs)、[`handover_common.rs`](src/handover_common.rs) | 商品和供给维护人交接、候选查询、目标资格和审计事实。 |
| [`attachments/mod.rs`](src/attachments/mod.rs) | 品牌标志、商品媒体和供应商资质附件的批次接入。 |
| [`contract.rs`](src/contract.rs)、[`errors.rs`](src/errors.rs) | 合同首次归档的文件登记命令；编排错误映射与提交结果分类。 |

## 架构设计

### Process、Port 与应用装配

Process 对一次业务命令负责，决定调用顺序和事务边界；它可以是资料 Process 类型，也可以是 `upload_contract` 这样的命名函数。领域 Service 组织本域步骤，实体维护不变式，Repository 封装领域集合访问；本库通过这些公开入口协作，不复制领域规则或另建事实集合。

Port 由消费方定义，只说明需要什么事实或能力。例如合同域声明 [`ContractCreationPort` 和 `CustomerFactsPort`](../../domains/erp-contract/src/ports/customer.rs)：前者从指定客户的当前负责人和组织形成合同创建候选，后者提供客户身份和启用状态等最小事实。[应用的合同装配](../../../apps/erp-server/src/wiring/contract.rs)将客户事实适配器注入 `ContractService`，并[把创建候选 Port 接入 `ContractAccess`](../../../apps/erp-server/src/wiring/authorization/master.rs)。本库接收已经装配好的 Service，不构造具体跨域 DataScope 或外部协议适配器。

应用在调用前装配所需的领域 Service、Access 和当前身份资格服务。[`wiring::authorization::build`](../../../apps/erp-server/src/wiring/authorization/mod.rs) 装配 Facts、Mapping 和共享 `AuthorizationService`；权限定义归 `erp-permissions`，授权判定归 `erp-identity`。[应用工厂](../../../apps/erp-server/src/wiring/application.rs)在 `CustomerProfileProcess::new` 注入共享授权源及同源 `CustomerAccess`，在 `SupplierProfileProcess::new` 注入同次装配的身份服务与 `SupplierAccess`。`CatalogHandoverPorts` 和 `OfferingHandoverPorts` 是应用传入的 Access 与写入 Service 组合，不是新授权实现；各自的范围与写入服务、以及命令使用的身份资格服务必须来自同一授权源。

### 真实用例与事务边界

- **资料建档和修订。** [客户建档](src/customer_profile/create.rs)先检查创建和字段资格，再确定新建还是复用主体；首条 `OWNER` 归属以认证创建人为准，而不是请求中的 `owner_user_id`。落库前重验真实创建候选；[客户修订](src/customer_profile/update.rs)检查当前客户范围、字段资格及客户和主体版本。[供应商建档](src/supplier_profile/create.rs)按维护人及其当前有效主属组织授权，联动主体、供应商商业档案、能力和资质。若带资质文件，[上传预检](src/supplier_profile/attachment_upload.rs)只检查功能资格，不签发对象凭证；正式事务另行授权并校验附件引用。品牌、商品和供应商的文件资产批次复用 [erp-file-assets](../erp-file-assets/README.md)；对象字节先在事务外写入，资产元数据和业务变更在事务内提交。
- **显式责任交接。** `handover_product` 和 `handover_offering` 先授权操作者对来源对象的更新，再检查原幂等回执。首次执行核验接任方是具备相应资格的有效内部账号（商品 `product:update`，供给 `supplier_offering:update`），并检查指定组织已启用。领域 Service 用同一个 `Executor` 重验当前对象和拟写入的归属、校验期望版本，再写维护人和业务组织。独立回执绑定操作人、目标和请求载荷，与变更及成功审计一起提交；匹配回执时重验当前访问权限并返回当前责任视图，不再写入或追加成功审计。同键异参拒绝。候选列表只供选择，不能代替写入授权。
- **合同文件首次登记。** `upload_contract` 接收已写入对象存储的 PDF 元数据，先用新文件资产 ID 规划合同与首修订，再开启根事务。事务内先由 [`ContractService`](../../domains/erp-contract/src/service/contract/mod.rs)按真实客户的归属重验创建范围，再登记文件资产；领域服务核验客户启用状态与签约主体资格后写入合同与首个不可变修订。文件登记、合同创建两笔成功审计按此顺序写入，所有步骤共用同一 `Executor`。该入口不上传字节，也不做识别；识别任务的人工确认归档走 [`erp-data-import` 的 `ContractImportProcess`](../erp-data-import/src/contract_import/archive.rs)。

多集合写入只在公开用例边界开启根事务：由 Process 持有或委托 `erp-audit` 的事务入口；品牌和商品附件包装交由 `CatalogService` 持有事务，不再套一层。事务内的授权重验、数据校验、业务写入、独立回执（有回执的命令）及成功审计复用同一 `Executor`，Repository、Port 和内部写入步骤不另开事务。HTTP 认证和事前检查不替代业务授权，外部 HTTP、对象存储和通知不放进数据库事务。

提交结果未知时不得自动重放写入。客户和供应商资料命令可用原请求、原幂等键和当前授权在新快照中只读查证；无法确认时保留原错误和来源。调用方重试有幂等键的命令时沿用原键，不另造命令。边界与验证规则按 [backend/AGENTS.md](../../../AGENTS.md) 执行；授权细节见[业务授权 API](../../domains/erp-identity/README.md#业务授权-api)，执行器选择见[读取边界](../../foundation/persistence-core/README.md#读取边界)。仅文档修改只做目标文件的 `git diff --check`；代码测试范围限库单元测试。
