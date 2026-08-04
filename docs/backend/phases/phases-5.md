# Phase 5：采购单、采购分配、财务审核与采购变更

## 1. 分支与隔离

| 项目 | 约定 |
| --- | --- |
| 分支名 | `codex/backend-p1-05-procurement` |
| 基线 | 与全部 phase 相同的冻结 `BACKEND_PHASE_BASE_SHA` |
| 实现语言 | 读取冻结基线中的统一后端语言/版本决定；缺失时停止，不自行选栈 |
| 独占目录 | `backend/modules/procurement/**` |
| 编译要求 | 不要求根工程编译；领域代码、端口和测试向量必须完整 |
| 禁止修改 | 根构建、共享 API/路由、正式迁移、其他 phase 和前端 |

## 2. 目标与对象所有权

本 phase 是以下事实的唯一写者：

- `purchase_order`、不可变提交和正式修订；
- 采购行及采购行到销售行的精确分配；
- 采购财务审核事实；
- `purchase_change_order`、提交、审核和差额意图；
- W08 列表、对象中心、提交/版本/分配/变更时间线查询。

不拥有采购二次确认、供应商供给、销售单、应付、付款、履约、库存或统一任务。

依据：`erp-phase-1.md` §6、§7.1、§7.4、§9.2；`erp-data-model.md` §6.6、
§7.4、§8.1；W08。

## 3. 目录结构

```text
backend/modules/procurement/
  domain/{purchase_order,purchase_submission,purchase_revision,allocation,purchase_change}/
  application/{commands,queries}/
  ports/
  contracts/
  persistence-spec/
  fixtures/
  tests/
  DECISIONS.md
```

## 4. 命令与事务计划

- `CreatePurchaseDraftFromConfirmedLines`；
- `SavePurchaseDraft(expectedDraftVersion)`；
- `SubmitPurchaseForFinanceReview(expectedDraftHash)`；
- `DeferPurchaseFinanceReview`；
- `CompletePurchaseFinanceReview(APPROVE | REJECT)`；
- `CreatePurchaseChange(baseRevisionId)`、`SubmitPurchaseChange`、
  `CompletePurchaseChangeReview`；
- `VoidPurchaseDraft`。

采购草稿只能来自已通过采购二次确认的精确分行。拆单维度固定为供应商、采购类型、
付款条件和履约责任；任一维度不同必须拆单，同一销售单/提交且四维完全一致时才可按
固定规则合并。

审核通过输出 `PurchaseActivationPlan`：冻结修订、应付、确认成本、审核动作、任务完成和
outbox 意图。Phase 10 负责同事务写入；本 phase 不直接写 Phase 7 或 Phase 1 的存储。

## 5. 领域不变量

- 正式采购单、提交和修订不可覆盖；财务审核只读不可变提交和 `subjectHash`。
- 采购行必须逐行追溯到已确认分行；采购到销售分配不超采购量和销售承诺量。
- 物流费用行独立计税且与商品/服务采购属于同一供应商，不能伪装为商品数量。
- 一张采购单支持多次付款；采购审核通过不等于已付款。
- 入仓采购在合格入库后沿精确采购销售分配建立预占，不能只按 SKU 猜销售归属。
- 已生效采购变更只追加新版本和差额；不得回写已付款、已入库、已开票或履约事实。
- 所有保存、提交、审核、变更和作废分别使用稳定幂等键；结果未知按同键查最终结果。
- 审核人与采购经办岗位分离，真实策略由权限端口重验。

## 6. 独立端口

- `ProcurementConfirmationReadPort`、`SalesCommitmentReadPort`；
- `SupplierOfferingSnapshotPort`；
- `PayablePostingPort`、`ConfirmedCostPostingPort`；
- `WorkItemTransactionPort`、`AuthorizationPort`、`AuditPort`、`OutboxPort`；
- `FulfillmentProgressReadPort`、`PaymentProgressReadPort`。

这些端口由本 phase 拥有接口和 recording fixtures；不 import 其他 phase 类型。跨域参数
必须包含稳定 ID、精确 revision、业务日期、数量、金额和指纹。

## 7. 测试要求

1. 四维拆单和合并规则，禁止跨销售提交/版本拼单。
2. 同草稿并发保存、双提交、审核/驳回竞争、旧提交/旧指纹拒绝。
3. 采购确认行、供给修订、供应商/SKU/业务日不一致时零写入。
4. 分配行数量守恒、超分配和不同销售行错误引用。
5. 审核通过事务计划恰含一个正式版本、应付、确认成本、任务完成和审计意图。
6. 重复幂等审核不重复应付或任务完成；结果未知可查询同一结果。
7. 同基准版本双变更只有一个成功；变更不覆盖历史付款/入库/票据。
8. 物流费税率、供应商和分配边界。

## 8. 未决项与 fail-closed

- W08 Q1：采购正式编号分配时点未确认。缺少编号策略时，草稿只用内部稳定 ID，
  不生成看似正式的可复用编号。
- W08 Q2：财务驳回原因代码集合未固化；Phase 10 注册前只接受明确受控集合，不接受
  任意客户端枚举冒充正式代码。
- W08 Q3：采购变更额外审批阈值未确认；由服务端版本化规则返回额外任务，缺规则时
  命中潜在高风险变更应阻断，不由 UI 猜阈值。

## 9. 完成标准

- 采购事实只有本 phase 一个写者，和 Phase 4 的二次确认边界清楚。
- 拆单、分配、审核、版本和变更不变量有领域/并发/幂等测试向量。
- 没有共享路由、物理 DDL、跨 phase FK 或直接写他域存储。
- 向 Phase 10 交付事务计划、端口、逻辑 schema、错误码、状态机和未决项。
