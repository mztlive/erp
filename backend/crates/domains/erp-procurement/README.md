# erp-procurement：采购订单与采购责任

## 职责

本 crate 管理公司向供应商采购的订单：买什么、买多少、采用哪份供给条款、哪一版已生效，以及由谁负责。采购单表达采购承诺，不是供应商主档、实际收货凭证或应付账簿。

### 负责的数据与能力

- **采购单据**：采购主表、可编辑草稿、冻结的审批提交及明细、生效版本及明细、采购变更单及其目标提交。主状态与付款、收票、履约进度分别记录。
- **数量与来源**：精确创建依据、拆单及供给分配计划、采购对销售需求的覆盖、采购版本行与销售行的正式分配，以及销售来源变化后的采购依据更新记录。
- **采购责任**：责任规则、负责人池及轮转状态、按需求行解析候选人，以及采购单上明确保存的当前责任人与业务组织。本域也保存采购命令回执、发件记录和费用确认发生记录。

### 使用场景

采购草稿、冻结提交、正式变更、数量覆盖或采购责任规则变化时，从本域入口找规则；要改审批结束后如何一起写入采购、应付和履约，或要改实际收货、供应商接口下单，则从对应 Process 或拥有该事实的领域入手。

## 目录结构

### 代码入口

公开模块和重导出见 [src/lib.rs](src/lib.rs)，直接依赖见 [Cargo.toml](Cargo.toml)。

| 目录 | 内容与入口 |
| --- | --- |
| [`src/entity/`](src/entity/mod.rs) | [`purchase_order`](src/entity/purchase_order/mod.rs) 定义单据、版本、变更、覆盖和分配；[`procurement_responsibility`](src/entity/procurement_responsibility/mod.rs) 定义规则与解析；[`facts.rs`](src/entity/facts.rs) 定义本域消费的外域事实。 |
| [`src/service/`](src/service/mod.rs) | [`PurchaseOrderService`](src/service/purchase_order/mod.rs) 提供本域构造、校验和持久化步骤；[`ProcurementResponsibilityService`](src/service/procurement_responsibility/mod.rs) 维护规则、解析候选和认领轮次。 |
| [`src/ports/`](src/ports/mod.rs) | 供应商资质和付款条件、覆盖事实、当前销售行、采购数据范围及责任解析事实的合同；变更所需的销售行事实也在此声明。 |
| [`src/authorization/`](src/authorization/mod.rs) | 提供采购对象事实与查询字段映射；[`PurchaseAccess`](src/service/purchase_order/access.rs) 消费统一授权，而不自行解释角色政策。 |
| [`src/repository/`](src/repository/mod.rs)、[`src/indexes/`](src/indexes/mod.rs) | 本域 MongoDB 集合访问器、查询和写入，以及组合根复用的 `indexes::ensure`。 |
| [`src/dto/`](src/dto/mod.rs)、[`src/error.rs`](src/error.rs) | 采购单、变更和责任的请求、响应与视图，以及领域错误和已知唯一索引冲突的映射。 |

## 架构设计

### 协作示例

已生效销售单出现供给缺口时，采购人员在自己的开放供给分配任务中选择来源。[依据建单](../../orchestration/erp-procure-to-pay/src/procure_to_pay/creation_basis/create.rs)在事务内核验任务、推进销售单的采购并发守卫，再按当前销售版本重算缺口和可供数量；本域根据[精确创建依据](src/entity/purchase_order/creation_basis.rs)校验数量与拆单范围。依据指纹绑定销售当前版本、守卫、任务、逐行供给版本和数量；专项报价版本参与分组和指纹，选定目标仓时也纳入依据。一个精确依据只能生成一张采购单，由[唯一索引](src/indexes/purchase_order.rs)兜底。[批量选源](../../orchestration/erp-procure-to-pay/src/procure_to_pay/sourcing_create.rs)还可在同一事务内落地现有库存预占；两种采购建单路径都在建单事务内冻结提交并启动审批，不把中间草稿作为成功结果返回。

### 订单：草稿、提交与生效版本

- [采购主表](src/entity/purchase_order/order/mod.rs)保存稳定身份与当前指针。`current_submission_id` 指向当前编辑稿或审批提交；`current_revision_id` 指向当前生效版本。仓储乐观锁版本用于并发更新，`approval_subject_version` 标识审批所冻结的一次提交，正式版本号记录生效历史，三者不能互换。
- [保存草稿](src/service/purchase_order/draft_edit.rs)从当前草稿及其完整明细构造替换稿；[冻结提交](src/service/purchase_order/submission.rs)从服务端已有内容继承选源并生成独立提交，首次提交才分配正式采购号。[审批启动写入](../../orchestration/erp-procure-to-pay/src/procure_to_pay/start_approval/start_persist.rs)把冻结提交、审批运行事实和入口任务放进同一事务。撤回及[采购单驳回](../../orchestration/erp-procure-to-pay/src/procure_to_pay/rejection.rs)都从原冻结提交派生独立编辑稿，不改原提交内容；旧 `PENDING_FINANCE_REVIEW` 仅兼容历史数据。作废只适用于符合条件的未生效编辑稿，由[作废流程](../../orchestration/erp-procure-to-pay/src/procure_to_pay/void_order.rs)释放相关专项占用并同步任务。
- 最终审批通过后，[正式化步骤](src/service/purchase_order/formalization.rs)从当前冻结提交生成生效版本及明细，写入正式销售分配，再将提交记为通过并切换采购单的生效指针。[分配维护](src/service/purchase_order/allocation_maintenance.rs)按稳定销售行 ID 和精确 SKU 绑定销售当前版本行；新采购版本写自己的分配，不覆盖历史分配。[覆盖聚合](src/entity/purchase_order/coverage.rs)只统计采购当前指针：未生效单据读取当前提交，正式采购读取当前版本及其分配；历史采购版本和作废采购不计入，现有库存预占按已预占与已消耗数量合计一次。销售减量造成的超覆盖单独列示，剩余量不为负。

### 变更：冻结目标，不回写历史

[采购变更流程](../../orchestration/erp-procure-to-pay/src/procure_to_pay/change/submit.rs)只允许对已生效或部分执行的采购单发起，已有进行中的变更（含可重提的已驳回变更）时不重复开单。变更单固定基准版本；保存编辑副本、冻结目标提交和最终生效是不同步骤。审批期间原采购生效版本继续有效。[数量规则](src/entity/purchase_order/change_order/quantity_bounds.rs)以已发生的入库、发货或交付量为下限，以销售当前目标和其他供给覆盖等事实计算上限，不允许通过改单抹去已发生的履约。

最终通过时，[本域变更步骤](src/service/purchase_order/change/effect.rs)只用当前待处理的冻结目标构造新版本，拒绝已经偏离当前采购版本的基准；客户端不能直接使变更生效。[变更过账](../../orchestration/erp-procure-to-pay/src/procure_to_pay/change/posting.rs)在原事务内复核数量、交付和采购来源，推进销售守卫，写新版本及其分配后才切换采购指针，同时同步应付依据或差额、任务及审计。原采购版本、历史分配和实际履约记录不回写。

### 责任：解析候选与确认分派

[责任规则](src/entity/procurement_responsibility/resolution.rs)按精确 SKU、当前分类到父分类、商品类型、默认调度池的顺序匹配；询价场景先尝试专用询价池。同层多条命中、无兜底或账号无效时拒绝，不猜负责人。[领域解析](src/service/procurement_responsibility/resolver.rs)通过 Port 取目录和人员事实，校验候选是可登录的后台账号；[责任编排](../../orchestration/erp-procure-to-pay/src/procure_to_pay/responsibility/resolver.rs)再校验实际办理资格。

预览不占轮次。销售正式化[创建采购任务前](../../orchestration/erp-order-to-cash/src/order_to_cash/formalization_posting.rs)在原事务复验责任计划；[销售询价分派](../../orchestration/erp-order-to-cash/src/sales_inquiry/assignment.rs)复用同一规则，但核验询价任务所需资格。实际分派时，同一批任务命中同一规则池只[认领一次轮次](src/service/procurement_responsibility/rotation.rs)，池成员、规则或轮次有冲突就拒绝，不能单独消耗轮次而不落任务。任务责任、采购单当前责任人与制单人是不同事实：建单重验任务和冻结行范围，并写入明确的采购责任；旧单缺责任人不能回退到创建人，改派也不自动改变单据业务组织。

### 依赖与协作边界

术语与统一分工见[总索引](../../README.md#代码中常见名称的含义)。本域可依赖 `erp-identity` 的公开接口；常规、构建和测试依赖均不得指向其他业务领域或组合层。采购消费的外域数据通过 Port 或显式事实传入，具体提供方由 Process 或应用装配；[采购应用接线](../../../apps/erp-server/src/wiring/purchase.rs)注入共享授权、审批运行时与财务任务责任。本域仓储只写本域集合。

| 边界 | 谁负责什么 |
| --- | --- |
| [erp-procure-to-pay](../../orchestration/erp-procure-to-pay/src/procure_to_pay/mod.rs) | 负责建单、审批、变更与采购到付款的跨域事务。[首次正式化过账](../../orchestration/erp-procure-to-pay/src/procure_to_pay/formalization_posting.rs)调用本域写采购版本，再调用 [erp-finance](../erp-finance/README.md) 写应付和确认成本，并登记履约草稿及任务；形成应付不等于已付款。跨域查询投影，如[采购覆盖取数](../../read-models/erp-read-models/src/purchase_center/repository/coverage.rs)，由 ReadModel 组合。 |
| [erp-fulfillment](../erp-fulfillment/README.md) 与 [erp-fulfillment-execution](../../orchestration/erp-fulfillment-execution/README.md) | 采购提供生效版本、分配和目标仓等约定；实际收货、发货和交付事实由履约领域持有。[入库过账](../../orchestration/erp-fulfillment-execution/src/fulfillment_execution/purchase_receipt_posting.rs)读取当前采购版本，将跨版本累计合格收货归并到当前行，并协调履约与库存写入。建履约草稿不等于实际收货或交付。 |
| [erp-supplier](../erp-supplier/README.md) 与 [erp-supply](../erp-supply/README.md) | 供应商账户、资质及商务档案归前者；SKU 供给、商业修订、可供状态、[询价专项报价及采用余额](../erp-supply/src/inquiry/service.rs)和供应商侧执行事实归后者。采购只记录选中来源及其冻结条款；[API 执行登记](../../orchestration/erp-supplier-orders/src/supplier_api_orders/enqueue.rs)在正式化事务中写执行意图，不在该事务内向供应商下单。 |

**询价专项采用**：商品采购行的 `supplier_offering_source.inquiry_quote` 非空时，实际含税单位成本和进项税率必须等于专项条款原值，不能改用通用供给价或销售反馈价。[候选组装](../../read-models/erp-read-models/src/purchase_center/repository/inquiry_basis.rs)同时保留通用和专项选择；源自询价的需求可直接改选或拆分使用通用供给，无须填改选原因。需求来源与实际报价采用分别追溯，未采用专项不能标记为已采用。草稿修改、提交和正式变更沿[同一专项合同](../../orchestration/erp-procure-to-pay/src/procure_to_pay/inquiry_supply.rs)重验数量、地址、有效期和供应资格；[待审变更](../../orchestration/erp-procure-to-pay/src/procure_to_pay/inquiry_supply/change.rs)只占用增量，不提前释放原生效数量和地址。

### 事务与并发

跨域采购命令由 `procure_to_pay` 开启事务；最终审批动作复用[审批运行时传入的执行器](../../../apps/erp-server/src/wiring/approval_dispatch/action_registry.rs)。本域 Service、Repository 和参与写入判断的 Port 复用调用方的 `persistence_core::Executor`，不另开写事务。决定写入的授权和事实在写入边界内校验；正式编号之外，提交序号或正式版本号可以预读计算，但不能替代事务内的状态复核、乐观锁（CAS）和唯一索引。外部 HTTP、发信等 I/O 不进入数据库事务。

对具有幂等合同的命令，[采购强类型回执](src/entity/purchase_order/command_receipt.rs)与业务写入同事务保存。以[保存草稿](../../orchestration/erp-procure-to-pay/src/procure_to_pay/draft_edit.rs)为例，同键异载荷冲突；提交结果未知时用原命令身份及指纹查证或重试，不能仅凭单据当前状态宣称重放成功。

## 修改执行要求

1. 无 I/O 的输入校验、状态迁移和不变式放入本域实体或值对象；Service 组织本域用例，跨集合原子写入由调用方用例或 Process 持有事务。
2. 新增或调整集合查询时同步评估索引；组合根复用 `indexes::ensure`，保持逐集合注册顺序。
3. HTTP 请求与响应优先复用用例所在层的 DTO；扩展公开合同须同步检查 Process、ReadModel 和应用调用方。
4. 业务改动补充本域库单元测试，覆盖成功、失败、边界及相关幂等或版本冲突路径。

## 验证要求

仅改文档时，只对改动文件运行 `git diff --check`。业务代码变更按[后端开发规则](../../../AGENTS.md#验证)执行定向检查；采购域库单测命令在 `backend/` 执行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-procurement --lib --locked
```

全量检查见[统一质量门禁](../../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
