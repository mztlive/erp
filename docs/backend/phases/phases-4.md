# Phase 4：统一销售单与采购二次确认

## 1. 分支与隔离

| 项目 | 约定 |
| --- | --- |
| 分支名 | `codex/backend-p1-04-sales-confirmation` |
| 基线 | 与全部 phase 相同的冻结 `BACKEND_PHASE_BASE_SHA` |
| 实现语言 | 读取冻结基线中的统一后端语言/版本决定；缺失时停止，不自行选栈 |
| 独占目录 | `backend/modules/sales-confirmation/**` |
| 编译要求 | 不要求根工程编译；业务聚合、命令、端口、状态机和测试向量必须完成 |
| 禁止修改 | 根工程、全局路由/OpenAPI、真实迁移、其他 phase 和前端 |

## 2. 目标、范围与唯一写者

本 phase 是统一 `sales_order`、稳定销售行、草稿、不可变提交、正式版本、销售变更和
采购二次确认事实的唯一写者。

第一期能力边界：

- ERP 可创建、提交、生效和变更 `GOODS_SERVICE` 销售单；
- `VOUCHER` 销售单只接受满足本 phase 自有 `MallVoucherSnapshotContract` 的完整快照，
  追加只读商业版本；
- ERP 卡券审批、ERP 卡券变更和向商城发送执行投影属于第二期，不在本 phase 实现；
- 采购二次确认是销售生效闸门和正式行为，不是独立业务单据；
- 客户验收、采购单、应收、任务和 outbox 由其他 phase 拥有，本 phase 只声明事务意图。

依据：`erp-phase-1.md` §4.1、§4.3、§4.4、§6、§7.1、§7.4、§8、§10；
`erp-data-model.md` §6.4、§6.5；W05、W07。

## 3. 模块结构

```text
backend/modules/sales-confirmation/
  domain/{sales_order,sales_submission,sales_revision,procurement_confirmation,sales_change}/
  application/{commands,queries}/
  ports/
  contracts/
  persistence-spec/
  fixtures/
  tests/
  DECISIONS.md
```

## 4. 聚合与命令

### 4.1 实物与服务销售单

非卡券销售草稿、提交和正式版本的每一行均固定引用 `sku_revision_id`，并保留品名、
规格、成交单价、税率等成交快照。公司商品池只是业务日期有效公司 SKU 的查询视图，
不是独立实体；`sales_visible_price` 归 `sku_revision` 所有，销售行唯一商品关联为
`sku_revision_id`。

- `CreateGoodsServiceSalesDraft`；
- `SaveSalesDraft(expectedDraftVersion)`；
- `SubmitSalesOrder(expectedDraftHash)`：冻结提交和 `subjectHash`；
- `RequestSubmissionWithdrawal`：W05 Q2 的版本化撤回策略未配置时返回
  `WITHDRAW_POLICY_UNCONFIRMED`，不得注册真实撤回写操作；策略确认后仍须重验未领取、
  无正式决定和无后继事实；
- `CreateSalesChange(baseRevisionId)`、`SubmitSalesChange`、`ApplyApprovedSalesChange`；
- `VoidPreEffectiveSalesOrder`；
- `EvaluateSalesCloseEligibility`：只输出资格，真实关闭事务由 Phase 10 组装。

### 4.2 采购二次确认

- `OpenProcurementConfirmation(submissionId, subjectHash)`；
- `SaveProcurementConfirmationEvidence`；
- `DeferProcurementConfirmation`：保持任务 `PENDING/IN_PROGRESS`；
- `CompleteProcurementConfirmation(APPROVE | REJECT)`。

通过必须基于不可变提交和完整指纹，逐行重验供应商、能力、资质、数量覆盖、精确
`supplierOfferingRevisionId`、成本、交期和履约方式。通过结果输出一个不可分割的
`SalesActivationPlan`，包含销售正式版本、应收意图、采购创建依据、审计和任务完成意图；
Phase 10 必须把这些写入同一真实事务。前置 phase 不可用多次端口调用伪装原子性。

驳回只形成确认事实和当前任务完成，不创建采购单或后继任务。销售选择改品/改价后，
必须形成新提交号、新指纹和全新采购确认任务；旧任务、旧确认行不能复用。

### 4.3 商城卡券快照

`ApplyMallVoucherSnapshot` 只接受本 phase 自有 `MallVoucherSnapshotContract`：来源身份、
唯一卡券明细和映射校验均为必需值；本 phase 用 fixture 独立覆盖正反例，不依赖 Phase 8
完成或 import 其代码。命令按来源商城 + 来源销售单号定位同一 ERP 销售单，追加观察版本；
不得由金额、名称或卡券类目猜测身份，也不得开放 ERP 编辑命令。Phase 10 才把 Phase 8
的写入意图适配到此契约。

## 5. 领域不变量

- `business_type` 创建后终身不变；卡券与实物服务不得混单。
- 每个卡券销售版本恰好一条卡券明细；实物服务销售单至少一条有效明细。
- 合同与销售单一对多；提交冻结客户、合同、结算主体、明细、价格、税率和履约承诺。
- 销售只能从业务日期有效的公司商品池（公司 SKU 集合）选品；系统以 `sku_revision_id` 重验 SKU 可售资格和 `sales_visible_price`，销售提交/正式版本保留成交快照。
- 采购确认只读取不可变提交，不读取可变草稿。
- 一个有效提交和指纹最多一个有效采购确认任务；幂等重放不重复生成版本或任务。
- 生效后商业变化只能追加销售变更和新版本；已发生履约、票款和旧版本不可回退。
- 非卡券销售单全部明细履约完成且应收结清后关闭；开票未完成不阻塞关闭。
- 商城卡券单第一期仍由商城主责；ERP 不是另建副本，也不创建第二个销售单号。

## 6. 独立端口

本 phase 自行声明并使用 recording/deny-by-default fixtures：

- `CustomerContractReadPort`、`SalesCatalogReadPort`、`SupplierOfferingReadPort`；
- `WorkItemTransactionPort`、`ReceivablePostingPort`、`PurchaseBasisPort`；
- `AuthorizationPort`、`AuditPort`、`OutboxPort`；
- `FulfillmentProgressReadPort`、`ReceivableBalanceReadPort`；
- `MallSnapshotSourcePort` 和本模块自有 `MallVoucherSnapshotContract` fixture。

端口参数只含稳定 ID、精确修订、业务日期、版本、指纹和不可变值对象，不 import
Phase 2、3、5、7、8 的实现或 ORM 类型。

## 7. 测试要求

1. 业务性质不可变、卡券恰一行、实物服务至少一行和跨类混单拒绝。
2. 同草稿并发保存、同提交双提交、旧 hash 确认和审批/驳回竞争。
3. SKU 修订、供给或资质在提交后失效时，确认必须拒绝且零半提交；不得以已失效的公司商品池查询结果或旧价格快照绕过重验。
4. 采购通过计划含正式版本、应收、采购依据和任务完成；任一适配失败整体回滚语义。
5. 驳回不建后继；新提交产生新 hash、新确认和新任务唯一身份。
6. 生效后销售变更只追加差额，不覆盖旧版本或已发生事实。
7. 商城快照重复、迟到、A→B→A、映射未完成和多明细拒绝。
8. 关闭条件只使用正式履约与应收事实，开票不成为第三条件。

## 8. 未决项与 fail-closed

- W05 Q1～Q4 的影响首屏、撤回条件、异常排序和移动端动作不能成为后端默认规则。
- W07 Q1：暂挂租约释放策略未确认，缺策略时返回 blocker，不静默保留/释放。
- W07 Q2：自动下一项偏好未确认时只接受当前会话显式覆盖，不写持久偏好。
- W07 Q3：成本偏离阈值由版本化规则提供；缺失时不猜百分比。
- W07 Q4：政策确认前，W07 handler 必须拒绝把角色池任务直接转给个人；不能因 Phase 1
  提供通用转交协议就默认放行。确认后仍完整使用正式后继任务链，不能覆盖责任人。
- 当前统一模型的 `procurement_confirmation_line` 缺精确供给修订引用；在 Phase 10 补齐
  `supplier_offering_revision_id` 及同供应商/SKU/业务日约束前，正式通过入口不得上线。
- W05 含第二期卡券审批设计；第一期后端必须 capability-gate 并完全不注册这些命令。

## 9. 完成标准

- `sales_order` 在全部 phase 中只有本 phase 一个写者；Phase 8 与本 phase 开发时互不依赖，
  只由 Phase 10 适配写入意图和快照契约。
- 实物服务销售、提交、确认、变更、关闭资格和卡券只读版本均有测试向量。
- 跨域原子性以一个强类型事务计划表达，未伪装成已完成真实事务。
- 仅修改独占目录，并向 Phase 10 交付逻辑约束、端口、错误码和 blocker。
