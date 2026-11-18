# erp-supplier：供应商账户与资质

`erp-supplier` 管理往来主体的供应商角色，回答「谁是供应商、具备哪些能力、在指定业务日是否合格、按什么商务条件合作」。供应商账户不是登录账号，也不代表该供应商已经为某个 SKU 建立了供给。

## 职责

### 负责的数据与能力

| 业务对象 | 本域负责的事实与规则 |
| --- | --- |
| 供应商账户 | 用 `party_id` 引用共享主体；维护稳定的供应商编号、启停状态、整体维护人、业务组织、默认付款条件引用及当前商务修订指针。一个主体至多有一个供应商账户。 |
| 供应商能力 | 同一供应商可登记实物、虚拟、线下服务、API、印刷等能力；同一能力代码在该供应商下唯一。维护能力负责人、服务区域、履约说明、启停状态、有效期和修订。 |
| 资质与能力关联 | 维护资质类型、证书编号、发证机构、有效期、附件引用、适用能力及资质修订。列表按最需处理的一份资质展示健康状态；筛选按是否存在符合条件的资质命中，两者口径不同。 |
| 评级与商务档案 | 记录评分和评级修订；商务修订保存结算方式、对账周期、付款条件快照、经营类目、开票资料及签约、付款主体引用。付款条件使用可计算计划付款日的受控代码。 |
| 资料命令与门户申请 | 定义建档和资料修订的确定性规则，持有资料命令与责任交接的幂等回执。门户合作条款申请与正式商务档案分开保存，保留冻结提交和处理历史；本域还定义独立的合作条款成功回执合同。 |

### 使用场景

调整供应商启停、维护人或能力负责人，登记证照、合同或适用能力，判断某类供给或采购所需的资格，维护评级、付款条件和商务修订，或处理门户合作条款申请时，从本域的规则和公开入口定位。需要同时保存主体、供应商与附件时，从主数据编排入口进入。

### 协作示例

为一家供应商登记实物供货能力、相关合同和月结条件：法定名称、联系人及银行账户由 [erp-party](../erp-party/README.md) 维护，能力、合同资质及商务档案由本域维护，整套资料由 [erp-master-data](../../orchestration/erp-master-data/README.md) 的 `SupplierProfileProcess` 统一保存。为该供应商登记某个 SKU 的报价和可供状态时，由 [erp-supply](../erp-supply/README.md) 维护供给，编排层调用本域校验所需资格。

## 目录结构

### 代码入口

| 入口 | 用途 |
| --- | --- |
| [Cargo.toml](Cargo.toml)、[src/lib.rs](src/lib.rs) | crate 依赖及公开入口，导出实体、DTO、Port、仓储和 `SupplierService` 等。 |
| [src/entity/](src/entity/mod.rs) | `supplier/` 下维护账户、能力、资质及其能力关联、评级、商务修订与付款条件；[建档计划](src/entity/supplier/creation_plan.rs)、[资料变更计划](src/entity/supplier/profile_change/mod.rs)和[资格判定](src/entity/supplier/eligibility.rs)不执行 I/O。[交接回执](src/entity/handover_receipt.rs)独立于审计记录。 |
| [src/service/supplier/](src/service/supplier/mod.rs) | `SupplierService` 提供列表、详情及事实装配；[资格服务](src/service/supplier/eligibility.rs)用调用方执行器加载事实，再调用实体的资格规则。 |
| [src/dto/](src/dto/mod.rs)、[src/error.rs](src/error.rs) | 列表、详情、资料保存、敏感字段查看、交接及导入的 DTO，以及本域错误类型。 |
| [src/ports/](src/ports/mod.rs)、[src/authorization.rs](src/authorization.rs) | 主体事实、附件事实及敏感令牌 Port；授权所需的供应商对象事实和固定查询映射。 |
| [src/repository/](src/repository/mod.rs)、[src/indexes/](src/indexes/mod.rs) | MongoDB 集合访问、查询筛选与分页、资料写入步骤和索引注册；公开的 `indexes::ensure` 依次注册主档、交接回执和门户索引。 |
| [src/portal/](src/portal/mod.rs) | 合作条款申请、冻结提交与决定、独立回执、正式商务修订计划及使用调用方事务的确认写入步骤。 |

## 架构设计

### 依赖与协作边界

- [erp-party](../erp-party/README.md) 拥有主体身份、名称修订、联系人、地址、银行及税务资料，也负责敏感数据编解码和揭示令牌。本域通过 `PartyFactsPort` 读取主体事实，通过 `SensitiveTokenPort` 请求短时令牌，不直接查主体集合。资质只保存附件引用，通过 `FileAssetFactsPort` 取得存在性与敏感级别；文件资产归 [erp-support](../erp-support/README.md)。这些 Port 由[服务端组合层](../../../apps/erp-server/src/wiring/supplier.rs)接到实际提供方。
- [erp-supply](../erp-supply/README.md) 拥有供应商与公司 SKU 的供给关系、SKU 供给条款和可供状态、供应商 API 连接、供应商侧履约及结算单；采购订单归 [erp-procurement](../erp-procurement/README.md)。本域的商务档案是供应商层面的合作条件，不替代供给条款或结算单。资格服务按商品类型定位所需能力，核对账户、能力及当前修订的启用状态、归属和业务日有效期；有适用的关联合同时，至少一份须在该业务日有效，起止日期未核实的合同不算有效，无关联合同则不额外要求合同。
- [erp-master-data 的 `SupplierProfileProcess`](../../orchestration/erp-master-data/src/supplier_profile/mod.rs) 持有资料创建、修订和责任交接的跨域事务。建档、修订在事务内做授权与引用校验，协调主体、供应商资料、附件登记、命令回执及成功审计；本域提供纯规则和仓储步骤，不接管跨域事务。同一幂等键重试须核对原请求并复用已记录结果。
- 门户合作条款由 [erp-supplier-catalog 的门户流程](../../orchestration/erp-supplier-catalog/src/supplier_portal/commercial.rs) 编排。提交冻结原稿，撤回、退回和确认追加决定；内部确认读取冻结提交，用调用方事务追加正式商务修订、推进账户指针，不回写既有采购快照。申请结果、工作项和审计在同一事务提交；当前流程通过[统一门户命令入口](../../orchestration/erp-supplier-catalog/src/supplier_portal/command.rs)使用 `erp-supply` 的门户回执，不走本域的合作回执仓储。
- 业务领域间的直接依赖仅允许使用 [erp-identity](../erp-identity/README.md) 的公开身份与授权接口；常规、构建及测试依赖均不得指向其他业务领域或组合层。`erp-permissions` 提供公共权限定义，其余外域事实通过 Port 或显式输入传递。术语与统一分工见[后端 crate 总索引](../../README.md#代码中常见名称的含义)。

### 修改执行要求

1. 将无 I/O 的输入规范化、不变式和状态迁移放在实体或值对象；创建和修订计划接收调用方准备的事实、ID、业务日和操作人。由 Service 组织本域用例，跨域原子写入由 Process 编排。
2. 受保护用例使用 [`AuthorizationService`](../erp-identity/README.md#业务授权-api) 的 `require` 或 `query`。本域只提供实际维护人、业务组织等对象事实和固定 Mapping，不解释角色条款、不组合权限范围、不构造授权结果。列表在同一快照内使用本阶段凭证，取数前核对资源、集合、输出粒度及读取边界，交付前用原执行器复验账号、权限、事实版本和有效期；凭证不得跨阶段、请求或事务复用，后续页须回传范围版本。
3. Repository 和 Port 沿用调用方的 [`persistence_core::Executor`](../../foundation/persistence-core/README.md#读取边界)。稳定主档更新使用版本检查，商务修订以追加新版本保留历史；查询或集合变更须同步评估索引，组合根复用公开注册入口并保持逐集合注册顺序。
4. HTTP 请求和响应优先复用本域 DTO；变更公开合同须检查 Process、[ReadModel](../../read-models/erp-read-models/README.md) 和应用调用方。业务代码改动补充库单元测试，覆盖成功、失败、边界及相关幂等和版本冲突路径。

## 验证要求

仅修改本文档时，在仓库根目录对本文件运行 `git diff --check -- backend/crates/domains/erp-supplier/README.md`。修改业务代码时，按[后端开发规则](../../../AGENTS.md#验证)和[统一质量门禁](../../README.md#质量门禁)执行；本 crate 的库单元测试命令在 `backend/` 运行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-supplier --lib --locked
```

不运行集成测试或依赖真实 MongoDB、外部服务的测试。
