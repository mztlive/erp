# 域与 crate 归属

> 状态：生效
>
> 本文件登记参与改造的 crate、模块与业务域及其所有权。新增域必须先在此登记。

## 1. 后端 crate

| crate | 职责 | 允许依赖 | 禁止依赖 |
| --- | --- | --- | --- |
| `crates/bpm` | 纯流程领域模型、图规则与单令牌状态引擎 | `entity-core`、`entity-macros`、`serde`、`chrono`、`thiserror` | `entities`、`database`、`services`、`apps/web-api`、`config`、`mongodb`、`axum`、权限宏、通知客户端 |
| `entities` | ERP 业务实体、单据审批绑定、业务对象快照、WorkItem、通知 outbox | `bpm`、`entity-core`、`entity-macros` | `database`、`services`、`apps/web-api` |
| `database` | BPM 与集成模型的 MongoDB 适配层 | `bpm`、`entities`、`mongodb` | `services`、`apps/web-api` |
| `services` | ERP 应用编排、政策、授权、事务、业务副作用 | `bpm`、`entities`、`database`、`config` | `apps/web-api` |
| `apps/web-api` | HTTP 协议适配 | `services` | `bpm`、`database` 的审批 Repository |
| `apps/cli` | 运维命令行：初始化超级管理员、重置管理员密码 | `services`、`database`、`config` | `web-api`、`bpm`、`axum` |

`bpm` 对 ERP 业务层保持零反向依赖。任何 `entities`、`database`、`services`、`apps/web-api`、`mongodb` 引用均为阻断，由 `backend/scripts/check-bpm-boundaries.sh` 失败关闭地验证。

## 2. 审批专项模块

| 模块 | 归属阶段 |
| --- | --- |
| `backend/crates/bpm/src/{lib,ids,error}.rs` | P0-A |
| `backend/crates/bpm/src/{model,graph}/**` | P1 |
| `backend/crates/bpm/src/engine/**` | P3 |
| `backend/services/src/approval/process_kind.rs` | P0-A |
| `backend/entities/src/approval_integration/**` | P1 |
| `backend/entities/src/document_registry/**` 审批绑定与动作审计 | P1 |
| `backend/entities/src/work_item/work_item.rs` 审批映射 | P1 |
| `backend/database/src/repository/{bpm,approval_integration,work_item}.rs` | P2 |
| `backend/database/src/indexes/{bpm,approval_integration,work_item}.rs` | P2 |
| `backend/services/src/approval/**` | P3 |
| `backend/services/src/work_item/**` | P3；P2 仅 ownsWithin 测试辅助 `w13_delta_row()` 的 `approval_node_execution_id` 字面量 |
| `backend/services/src/document_registry/**` | P3 |
| `backend/apps/web-api/src/core/handler/{approval_process,approval_instance,work_item}/**` | P3 |
| `erp-client/features/approval-processes/**` | P4 |
| `erp-client/features/approval-workflow/**` | P4 |
| `erp-client/features/{work-items,unified-task-queue,workspace}/**` | P4 |
| `backend/scripts/reset-dev-business-data*` | P5 |
| `docs/runbooks/approval-workflow.md` | DOC |
| 旧审批模型、旧运行时、旧责任动作与旧权限的跨层删除 | P0-D；只允许在全部逐类型阶段完成后执行 |

## 3. 业务域

每个业务域在审批专项中对应一个独立的 P3 适配子阶段和一个独立的 P4 页面子阶段。

| 业务域 | Service 目录 | 涉及 `DocumentType` |
| --- | --- | --- |
| 销售 | `services/src/sales_order/**` | `SalesOrder`、`VoucherSalesOrder` |
| 销售变更 | `services/src/sales_review/**` | `SalesChangeOrder` |
| 采购 | `services/src/purchase_order/**` | `PurchaseOrder`、`PurchaseChangeOrder` |
| 库存 | `services/src/inventory/**` | `StockAdjustment` |
| 应收 | `services/src/receivable/**` | `CustomerReceipt`、`Invoice` |
| 应付 | `services/src/payable/**` | `SupplierPayment` |
| 退货退款 | `services/src/returns/**` | `CustomerRefund`、`SupplierRefund`、`ReceiptReversal`、`PaymentReversal`、`SalesReturnCase`、`PurchaseReturnOrder` |
| 履约 | `services/src/fulfillment/**` | `PurchaseReceipt`、`Delivery`、`ElectronicDelivery`、`ServiceFulfillment`、`CustomerAcceptance` |

下列域只作为 WorkItem 构造调用方参与，不拥有 `DocumentType`：`integration_ops`、`legacy_import`、`mall_sync`、`publication`、`supplier_fulfillment`、`supplier_settlement`。
