# erp-sales：销售订单、销售变更、询价与选品

`erp-sales` 是销售业务领域 crate。它维护销售单、询价和选品各自的状态与来源，区分可编辑内容、已冻结的协作记录和正式生效的销售版本。审批实例、采购单、履约单据和财务账并不归本域所有。

<a name="使用场景"></a>
<a name="负责的数据与能力"></a>

## 职责

- **销售订单与变更**：维护实物与服务、卡券销售单的稳定主单和明细身份、草稿工作副本、冻结提交、正式修订及生命周期。生效后通过销售变更单修改，保留原版本。
- **销售询价**：维护需求草稿、冻结需求、逐行供给反馈、销售接受、多轮重询与关闭；记录跨版本的转单数量、费用归属及办理额度。另维护询价命令回执、导入预览数据，以及销售和任务采购的独立视图。
- **销售选品**：维护选品册、准备任务、商品和图片快照、单品或套餐陈列、册内售价、客户访问与会话，以及客户提交后冻结的销售方案。选品方案不是销售单，也不替代合同或凭证。

<a name="代码入口"></a>

## 目录结构

| 入口 | 用途 |
| --- | --- |
| [Cargo.toml](Cargo.toml)、[src/lib.rs](src/lib.rs) | crate 依赖与公开模块；统一导出 `Error`、`Result`。 |
| [entity/](src/entity/mod.rs) | 订单、变更、询价和选品的实体、值对象、状态迁移及确定性规则。询价的[商品池模板合同](src/entity/sales_inquiry_pool.rs)定义版本、列布局及需求导入白名单，由 [erp-data-import](../../orchestration/erp-data-import/README.md) 的导入和导出共用。 |
| [service/](src/service/mod.rs) | 本域用例与写入步骤：[SalesOrderService](src/service/sales_order/mod.rs)、[SalesReviewService](src/service/sales_review/mod.rs)（销售变更）、[SalesInquiryService](src/service/sales_inquiry/mod.rs) 和 [SalesSelectionService](src/service/sales_selection/mod.rs)。 |
| [dto/](src/dto/mod.rs) | 命令输入、查询和响应结构，包括按访问场景分开的询价和选品投影。 |
| [ports/](src/ports/mod.rs) | 销售规则所需的外域事实接口，以及供组合层显式映射的销售变更事实。 |
| [repository/](src/repository/mod.rs)、[indexes/](src/indexes/mod.rs) | 本域 MongoDB 仓储、集合访问器、筛选与分页；组合根通过 `indexes::ensure` 注册索引。 |

## 架构设计

### 销售订单：草稿、提交和正式修订是三类记录

- **草稿可改**。稳定主单保存商业主状态和当前正式修订指针，工作副本及其明细承载编辑内容。`base.version` 用于乐观并发校验；工作副本另有 `draft_version`，随内容保存递增并用于计算草稿内容指纹。明细继续引用稳定行身份，不因重提而重建业务身份。见[工作副本](src/entity/sales_order/working_copy.rs)和[订单生命周期](src/service/sales_order/lifecycle.rs)。
- **提交冻结**。每次提交锁定工作副本并新增提交头和明细。`submission_no` 标识本轮提交，提交记录同时保留来源草稿的 `draft_version`；审批引用该轮冻结内容，不读后来的草稿。实物与服务、卡券单的新提交均进入商业主状态 `PENDING_REVIEW`、审核轨 `IN_APPROVAL`；撤回和驳回都回到 `DRAFT`，审核轨分别记为 `NOT_SUBMITTED` 和 `REJECTED`。草稿可以作废；履约、回款、开票和关闭状态与商业主状态分开。见[提交实体](src/entity/sales_order/submission.rs)和[状态定义](src/entity/sales_order/entity/status.rs)。
- **正式版本只追加**。首次审批通过时从冻结提交生成正式修订 `revision_no = 1`，主单转为 `EFFECTIVE` 并挂接该修订。已生效订单不直接编辑；变更单以 `base_revision_id` 为基线，提交时另行冻结。变更可撤回或被驳回，草稿和已驳回变更可作废。生效前确认基线仍是当前正式版本，再由冻结的变更提交生成下一版、更新主单指针，不覆盖旧版本。见[订单生效](src/service/sales_order/formalize.rs)、[变更状态](src/entity/sales_review/sales_change_order.rs)和[变更生效](src/service/sales_review/effective.rs)。

### 销售询价：需求版本、反馈轮次和办理进度独立

- `base.version` 防止并发覆盖；`requirement_version` 标识冻结需求，首次提交或需求确有变化时才新增快照；`feedback_round` 在采购发布反馈时才递增。销售提交后直接进入采购处理中；采购可退回待销售补充，也可汇总责任行结果并发布，进入待销售处理。销售接受时绑定确切的需求版本和反馈轮次，进入已确认，但不自动转单。销售重询或采购主动重报回到采购处理中，不覆盖旧记录；关闭仅结束询价，不替代已派生单据的生命周期。见[询价聚合](src/entity/sales_inquiry/mod.rs)与[状态迁移](src/entity/sales_inquiry/lifecycle.rs)。
- [数量和费用台账](src/entity/sales_inquiry/ledger.rs)按稳定的需求行、配送地址及费用身份跨轮累计，重询不能抹掉已占用或已办理的量额。询价转单时，Process 在建单事务中复验已接受的来源并占用数量；销售草稿变动可调整占用，作废时释放，首次生效时记为已办理。费用可按销售单分配，由采购或财务单据及行引用办理额度；`SalesInquiryService` 支持按来源单据反查，并在调用方的同一 `Executor` 内保存聚合与独立命令回执。见[询价转销售](../../orchestration/erp-order-to-cash/src/order_to_cash/inquiry.rs)和[销售单的询价来源](src/service/sales_order/inquiry.rs)。
- 销售视图不暴露未发布的采购工作与内部办理单据身份，任务采购使用的独立视图不包含销售参考价。投影不代替授权，调用方仍须校验对象可见性和任务责任范围。见[询价 DTO](src/dto/sales_inquiry/mod.rs)与[询价读取入口](../../read-models/erp-read-models/src/sales_center/inquiry/mod.rs)。

### 销售选品：准备批次、链接授权和客户会话分别控制

- [选品册状态](src/entity/sales_selection/status.rs)主路径为草稿 → 准备中 → 待发布 → 已发布。创建时会排入首次准备；待发布可再次准备，准备失败恢复到任务开始前的草稿或待发布。准备结果按批次保存商品、图片与陈列快照；写入时校验册版本、活动任务和任务运行版本，防止迟到结果覆盖当前批次。待发布时可改当前批次的册内价格，不改目录价格；发布必须确认册版本和当前批次，并复核冻结 SKU 修订仍可售。见[准备队列](src/service/sales_selection/prepare_queue.rs)、[册内改价](src/service/sales_selection/pricing.rs)和[发布](src/service/sales_selection/publish.rs)。
- `base.version`、`current_batch_id`、`link_token_version` 是册的写入版本、当前批次和链接令牌版本；客户会话另用 `session_version` 防止并发覆盖。发布建立受链接、密码和有效期约束的客户会话；提交在同一事务中冻结会话并写入方案、陈列行、SKU 行和幂等结果，一册一方案，提交后为 `SUBMITTED`。草稿或待发布可作废，已发布可关闭，链接到期或撤销后不能继续公开操作。见[选品册实体](src/entity/sales_selection/booklet.rs)、[客户会话](src/service/sales_selection/session.rs)和[选品唯一索引](src/indexes/sales_selection.rs)。
- 按份采购方案可为同一客户预填实物与服务销售单。销售单保留方案 ID 和编号作追溯，预填明细来自冻结 SKU 行；商业条款、交付信息和开单凭据仍按销售单规则办理。见[开单依据](src/dto/sales_order/selection_basis.rs)与[方案来源规则](src/entity/sales_order/selection_source.rs)。

<a name="协作示例"></a>

### 依赖与协作边界

- **领域依赖**：`erp-sales` 使用共享基础库及 [erp-identity](../erp-identity/README.md#业务授权-api) 的公开授权服务；常规、构建和测试依赖不得指向其他业务领域或组合层。订单可售性、参考价及应收余额，以及选品所需的客户、目录和图片能力，通过[消费方 Port](src/ports/mod.rs)或明确传入的事实取得，由组合层或应用接入实际提供方。可派生销售进度，但不在本域维护采购、履约或财务原始事实。
- **写入与审批**：[erp-order-to-cash](../../orchestration/erp-order-to-cash/README.md) 的 `order_to_cash`、`sales_change`、`sales_inquiry` 和 `sales_selection` 协调跨域命令。例如，销售提交时本域冻结提交，Process 通过 [erp-approval-runtime](../../orchestration/erp-approval-runtime/README.md) 接入 [erp-workflow](../erp-workflow/README.md) 的审批运行；生效时由 Process 协调正式修订、应收或采购任务的联动。询价责任任务、审计与回执由 Process 协调。原子写入由 Service 或 Process 持有事务，Repository、Port 和写入步骤复用其 `Executor`；外部 I/O（如准备选品图片快照）放在写事务外。
- **读取与交付**：[erp-read-models](../../read-models/erp-read-models/README.md) 组装订单列表中的应收、采购和审批信息，并承接询价、选品册及方案的管理端授权查询；本域提供单域查询、业务事实和按访问场景区分的 DTO。应用负责 HTTP、适配器装配与后台任务运行。术语与统一分工见[总索引](../../README.md#代码中常见名称的含义)。

## 修改执行要求

1. 将无 I/O 的校验、状态迁移和不变式放入实体或值对象；Service 组织本域用例，Process 协调跨域写入。
2. Repository 使用调用方传入的 `persistence_core::Executor`。新增或调整查询时同步评估索引；组合根复用本域公开索引入口，保持既有逐集合注册顺序。
3. HTTP 请求和响应优先复用本域 DTO；扩展公开合同时检查 Process、ReadModel 和应用调用方。
4. 业务改动补充本域库单元测试，覆盖成功、失败、边界及相关幂等或版本冲突路径。

## 验证要求

仅改文档时，按[后端开发规则](../../../AGENTS.md#验证)只对改动文件运行 `git diff --check`。业务代码变更按[统一质量门禁](../../README.md#质量门禁)执行定向检查；本 crate 的库单元测试命令在 `backend/` 下执行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-sales --lib --locked
```

不执行集成测试或依赖真实外部服务的测试。
