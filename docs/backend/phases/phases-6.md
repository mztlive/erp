# Phase 6：履约、库存、客户验收、退货与调整

## 1. 分支与隔离

| 项目 | 约定 |
| --- | --- |
| 分支名 | `codex/backend-p1-06-fulfillment-inventory` |
| 基线 | 与全部 phase 相同的冻结 `BACKEND_PHASE_BASE_SHA` |
| 实现语言 | 读取冻结基线中的统一后端语言/版本决定；缺失时停止，不自行选栈 |
| 独占目录 | `backend/modules/fulfillment-inventory/**` |
| 编译要求 | 不要求根工程编译；可实现内部事务服务，但未决入口必须 fail-closed |
| 禁止修改 | 根构建、共享 API、正式迁移、其他 phase、前端 |

## 2. 目标与对象所有权

本 phase 统一拥有：

- `purchase_receipt`、`delivery`、`electronic_delivery`、`service_fulfillment`；
- `stock_movement`、`stock_balance`、`stock_reservation` 及分配、库存调整；
- `customer_acceptance`、验收行和验收—履约分配；
- 销售退货/拒收、采购退货及其行；
- W06、W09、W10 所需读模型和命令语义。

不拥有采购单、销售单、应付/付款、实际成本、统一任务或权限。跨域更新先表达为
不可分割的 transaction plan，Phase 10 再接入真实同事务适配器。

依据：`erp-phase-1.md` §6.3～§7.5、§9.3、§10；`erp-data-model.md` §6.7、§6.11、
§8.2；W06、W09、W10。

## 3. 目录结构

```text
backend/modules/fulfillment-inventory/
  domain/{receipt,delivery,electronic,service,inventory,reservation,adjustment,acceptance,returns}/
  application/{commands,queries,gates}/
  ports/
  contracts/
  persistence-spec/
  fixtures/
  tests/
  DECISIONS.md
```

## 4. 已确认的业务事务

### 4.1 收货与仓发

- 合格收货先校验付款门禁和采购剩余量，再追加收货事实、库存流水、余额；沿精确
  采购销售分配建立预占，并输出实际成本和采购进度意图。
- 仓发只能消费当前销售明细的有效预占；追加发货事实、库存出库流水和余额更新。
- 库存写按 `(warehouseId, skuId)` 稳定顺序锁定；流水、余额、预占建立/消费/释放
  是同一事务。

### 4.2 直发、电子和线下服务

- 供应商直发不写公司自有库存，必须引用精确采购销售分配。
- 电子交付和服务履约引用同一销售明细与有效采购分配。
- 敏感电子交付信息加密保存，日志、错误、普通缓存和查询投影只返回安全摘要。
- `PREPAY/POSTPAY` 门禁由服务端重验；付款反冲不回退已发生履约，只阻断新履约。

### 4.3 客户验收与退货

- 当前只实现从 W05 直接进入的 `DIRECT_OBJECT` 验收语义。
- 验收锁定销售行、履约事实和已有净分配；`APPLY - REVERSE` 不得超过净成功履约量。
- 冲正追加反向验收和 `REVERSE`，不覆盖原事实。
- 短少、拒收、服务不通过只记录验收结论，不直接改库存、应收或采购。
- 退公司仓只有仓储确认可再入库量才增加库存；客户直退供应商不写自有库存。
- 客户退款、红票、供应商退款和应付冲减是 Phase 7 的独立后续事实。

### 4.4 库存调整

- 流水不可改删；同一来源动作唯一，余额可由流水重建。
- `on_hand >= 0`、`reserved >= 0`、`available = on_hand - reserved >= 0`。
- 预占不自动过期，只能由审核生效的变更、作废、退货或调整释放。
- 调整通过草稿、仓储复核、财务确认后才追加流水；经办与复核岗位分离。

## 5. 正常履约任务模型硬 blocker

W09 Q1 尚未决定五类正常履约使用统一 `work_item` 还是独立领域作业投影。因此本
phase 必须实现默认拒绝的 `FulfillmentExecutionGate`：

- 所有正常履约队列、领取、续租、保存、暂挂和过账在任何领域写之前返回
  `FULFILLMENT_TASK_MODEL_UNCONFIRMED`；
- 不持久化 `workItemId`、`operationTaskId`、`paused` 或客户端 `mode`；
- 不借用 `BUSINESS_EXCEPTION` 冒充正常履约任务；
- 可实现只读历史查询、领域算法和被 gate 包住的事务服务，但不注册正式运行时入口；
- 两个候选只能保留在 `DECISIONS.md`，不得同时生成运行时代码。

确认后必须二选一并删除另一候选：

- `WORK_ITEM`：注册固定类型，使用 Phase 1 完整领取/租约/完成协议，事实与任务完成同事务；
- `DOMAIN_OPERATION`：使用独立领域租约，不进入 W01/W02，不读写或复制 `work_item` 状态机。

## 6. 命令、查询与端口

领域命令包括：

- `PostQualifiedReceipt`、`PostWarehouseShipment`、`PostDirectShipment`；
- `PostElectronicDelivery`、`PostServiceFulfillment`；
- `SaveAcceptanceDraft`、`PostDirectAcceptance`、`ReverseAcceptance`；
- `CreateSalesReturnCase`、`ConfirmWarehouseReturn`、`CreatePurchaseReturn`；
- `CreateStockAdjustment`、`CompleteStockAdjustmentReview`；
- W10 四视图查询、来源追溯和后台导出意图。

端口包括 `PurchaseSnapshotPort`、`SalesAllocationPort`、`PaymentGatePort`、
`InventoryPostingPort`、`ActualCostPort`、`SalesProgressPort`、`PayableCorrectionPort`、
`WorkItemTransactionPort`、`AuthorizationPort`、`AuditPort`。

库存调整固定复用已登记的 `INVENTORY_ADJUSTMENT_REVIEW`，不得临时创造第二种任务代码：

1. 草稿提交后状态为待仓储复核，创建仓储责任池任务；只允许针对该阶段的通过/驳回决定。
2. 仓储通过与当前任务完成同事务，把调整推进为待财务确认，并以新 subject
   version/hash 创建财务责任池的同类型后继任务。
3. 财务通过与后继任务完成同事务追加库存流水、更新余额/预占并过账；任一失败整体回滚。
4. 经办、仓储复核和财务确认必须岗位分离；驳回保留决定，不复用已完成任务。
5. Phase 10 若未注册 stage-aware handler、各阶段唯一 completion action 和上述顺序，
   库存调整提交/复核/过账全部 fail-closed。

## 7. 测试要求

1. 默认 gate 下所有正常履约命令零写入。
2. 两次收货累计超采购量、重复来源动作和付款门禁竞争。
3. 两次仓发竞争同一预占、库存/调整并发、负库存和余额重建。
4. 直发/电子/服务零自有库存流水，且精确分配引用错误时拒绝。
5. 敏感交付信息的加密、查询裁剪和日志脱敏。
6. 多次验收分配守恒、跨销售行拒绝、验收/冲正并发。
7. 退公司仓、客户直退供应商和采购退货的库存差异。
8. 调整岗位分离、双审核竞争、重复幂等过账和不可改流水。
9. 事务半失败时收货/余额/预占/成本意图整体回滚语义。

## 8. 其他未决项与 fail-closed

- W06 Q1：证据白名单缺失时，命中需附件的验收拒绝过账。
- W06 Q2：普通验收任务类型/粒度未登记；`WORK_ITEM` 入口拒绝，但不阻塞明确的
  `DIRECT_OBJECT` 验收。
- W06 Q3～Q5：拒收后继、冲正复核和移动端批量验收不自行默认。
- W09 Q2～Q5：单次批量边界、凭证规则、超收和冲正岗位分离由版本化策略提供；
  缺失时对应动作阻断。
- W10 Q1：安全库存策略未确认前只展示零可用事实，不生成“低库存”业务结论。
- W10 Q2：仓储默认只看销售单/行稳定引用，不返回客户字段。
- W10 Q4：当前导出全部走冻结后台任务，不开放前端自定同步阈值。

## 9. 完成标准

- 履约、库存、验收和退货对象只有本 phase 写入，边界有测试证明。
- W09 未决时编写的业务代码不能绕过默认 gate；不存在双轨兼容层。
- 所有跨域写入由单一 transaction plan 描述，没有直接写其他模块。
- 仅修改独占目录，向 Phase 10 交付逻辑 schema、端口、状态机、测试向量和 blockers。
