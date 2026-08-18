# 审批流程改造：阶段矩阵与切换状态

> 状态：DOC-A 已合并，P0-A 已解锁；DOC-B、DOC-C 合同就绪，必须按独立 PR 顺序合并；DOC-D 等待 P3-HTTP。
>
> 权威业务合同：[`docs/approval-workflow-contract.md`](../approval-workflow-contract.md)
>
> 技术实施计划：[`docs/approval-workflow-implementation-plan/`](../approval-workflow-implementation-plan/README.md)
>
> 通用规则：[`conventions.md`](./conventions.md)、[`domains.md`](./domains.md)、[`_meta.json`](./_meta.json)

本文件不重新定义业务语义，也不重新定义通用所有权规则。它只登记审批专项的阶段映射、已签署输入的引用、逐类型批次归属和切换状态。

## 1. 已签署输入引用

P0 的全部前置输入已在权威业务合同中生效：

| 输入 | 权威位置 | 状态 |
| --- | --- | --- |
| 20 行政策矩阵（`NO_APPROVAL` / `PROCESS_REQUIRED`） | 合同 §4.3 | 已签署 |
| 12 行生命周期矩阵（状态、版本、动作、副作用、撤回合同） | 合同 §4.4 | 已签署 |
| `subject_version` 权威来源 | 合同 §4.4.1 | 已签署 |
| 状态机收敛规则 | 合同 §4.4.2 | 已签署 |
| 团队业务任务禁止规则（采购确认作为必备用途节点，低毛利环节删除） | 合同 §4.4.3、§5.2 | 已签署 |
| `subject_snapshot` 有界字段 | 合同 §4.4.5 | 已签署 |
| 唯一试点 `StockAdjustment` | 合同 §4.5 | 已签署 |
| 权限双门禁与管理员类型级权限命名 | 合同 §4.6、§15.2 | 已签署 |
| WorkItem 适配映射 | 合同 §13.3 | 已签署 |
| BLOCKED 分类与三条恢复路径 | 合同 §12.2—§12.5 | 已签署 |
| 通知收件人、去重、重试与死信 | 合同 §16.5 | 已签署 |
| BPM / ERP 单向依赖 | 合同 §2.4 | 已签署 |
| 开发环境硬切换与无迁移边界 | 合同 §17 | 已签署 |
| 两类销售单提交直接启动审批 | 合同 §4.4.1 | 已签署 |
| `ReviewStatus` 三值并删除审批导致的业务 `REJECTED` | 合同 §4.4.2 第 1、5 条 | 已签署 |
| 卡券运营环节为普通审批节点 | 合同 §4.4.3 | 已签署 |
| 全系统唯一责任模型（无责任池、无领取） | 合同 §1 第 7 条、§13.2 | 已签署 |
| W01/W02 合并为唯一 `/workspace` 工作台 | 合同 §16.4 | 已签署 |

上述任一项被发现仍有空值、二选一或待定说明时，必须停止对应阶段并退回 DOC 阶段修订合同。实施人员不得代替业务合同设置默认值。

### 1.1 DOC 合并门禁

| 输入 | 归属阶段 | 卡住的阶段 | 状态 |
| --- | --- | --- | --- |
| 权威合同、phase、实施计划与 `docs/dev-plan` | DOC-A | P0-A | 已合并 |
| `erp-data-model.md` 目标集合、索引、版本和删除范围 | DOC-B | P2 | 合同就绪，待 DOC-A 后合并 |
| 全部受影响 W 文档、W24、流程与术语 | DOC-C | P4-DEFINITION、P4-WORKFLOW | 合同就绪，待 DOC-A 后合并 |
| `approval-workflow-openapi.yaml`、错误目录、`runbooks/approval-workflow.md`、`openapi:lint` 脚本与 `@redocly/cli` 锁定 | DOC-D | P6-PILOT | 阻塞于 P3-HTTP |

DOC-D 依赖 P3-HTTP 的端点稳定，因此排在 P3 之后、P6-PILOT 之前，不阻断 P0—P3。

## 2. 阶段映射

| 阶段 | 实施计划文件 | 交付 |
| --- | --- | --- |
| DOC-A | [10](../approval-workflow-implementation-plan/10-contract-document-synchronization.md) | 权威合同、两份 phase 文档和本目录 |
| DOC-B | [10](../approval-workflow-implementation-plan/10-contract-document-synchronization.md) | `erp-data-model.md` 目标集合、索引与字段 |
| DOC-C | [10](../approval-workflow-implementation-plan/10-contract-document-synchronization.md) | W01 唯一工作台、W02 废止声明、W05/W19/W24 与 `ui-glossary.md` |
| DOC-D | [10](../approval-workflow-implementation-plan/10-contract-document-synchronization.md) | OpenAPI、错误目录、runbook 与 `openapi:lint` |
| P0-A | [00](../approval-workflow-implementation-plan/00-foundation-and-shared-contracts.md) | `bpm` workspace、依赖边界、冻结 ID 与边界类型、`DocumentType <-> ProcessKind` 映射、共享声明与目标模块占位、边界检查脚本 |
| P1 | [01](../approval-workflow-implementation-plan/01-policy-and-domain-model.md) | BPM 流程模型与图规则；ERP 集成实体 |
| P2 | [02](../approval-workflow-implementation-plan/02-persistence-and-indexes.md) | Repository、索引、CAS |
| P3-DEFINITION | [03](../approval-workflow-implementation-plan/03-definition-management-service.md) | 定义草稿、发布、退役 Service |
| P3-RUNTIME | [05](../approval-workflow-implementation-plan/05-runtime-rejection-and-reassignment.md) | BPM 纯引擎与 ERP 事务编排 |
| P3-ADAPTER-BASE / PILOT / `P3-ADAPTER-*` / `P3-NO-APPROVAL-*` | [04](../approval-workflow-implementation-plan/04-document-binding-and-business-adapters.md) | 绑定端口、审批适配器、无审批证明与逐类型接入 |
| P3-HTTP | [06](../approval-workflow-implementation-plan/06-http-permissions-and-errors.md) | HTTP、权限、错误 |
| P0-B | [00](../approval-workflow-implementation-plan/00-foundation-and-shared-contracts.md) | `AppState`、路由与 handler 合并、outbox worker 启停、权限生成；未接入类型失败关闭 |
| P4-DEFINITION | [07](../approval-workflow-implementation-plan/07-frontend-definition-management.md) | W24 审批流程配置工作面 |
| P4-WORKFLOW / PILOT / 逐类型 | [08](../approval-workflow-implementation-plan/08-frontend-document-and-workbench.md) | 通用审批区、工作台、逐类型页面接入 |
| P0-C | [00](../approval-workflow-implementation-plan/00-foundation-and-shared-contracts.md) | W24 workspace 注册与前端权限生成 |
| P5 | [09](../approval-workflow-implementation-plan/09-development-reset-and-cutover.md) | 开发业务数据重置脚本与 runbook |
| P0-D | [11](../approval-workflow-implementation-plan/11-integration-and-acceptance.md) | 全类型接入后的旧模型、旧运行时、旧责任动作与旧权限硬删除 |
| P6-PILOT / P6-FINAL | [11](../approval-workflow-implementation-plan/11-integration-and-acceptance.md) | 试点门禁与最终验收 |

## 3. 逐类型批次

试点：**`StockAdjustment`**。`P6-PILOT` 通过前，下表其余 `PROCESS_REQUIRED` 类型不得开始接入。

| `DocumentType` | P3 适配子阶段 | P4 页面子阶段 | 状态 |
| --- | --- | --- | --- |
| `StockAdjustment`（试点） | `P3-ADAPTER-PILOT` | `P4-PILOT` | 未开始 |
| `SalesOrder` | `P3-ADAPTER-SALES-ORDER` | `P4-SALES-ORDER` | 阻塞于 P6-PILOT |
| `VoucherSalesOrder` | `P3-ADAPTER-VOUCHER-SALES-ORDER` | `P4-VOUCHER-SALES-ORDER` | 依赖上一类型 |
| `SalesChangeOrder` | `P3-ADAPTER-SALES-CHANGE-ORDER` | `P4-SALES-CHANGE-ORDER` | 依赖上一类型 |
| `PurchaseOrder` | `P3-ADAPTER-PURCHASE-ORDER` | `P4-PURCHASE-ORDER` | 依赖上一类型 |
| `PurchaseChangeOrder` | `P3-ADAPTER-PURCHASE-CHANGE-ORDER` | `P4-PURCHASE-CHANGE-ORDER` | 依赖上一类型 |
| `CustomerReceipt` | `P3-ADAPTER-CUSTOMER-RECEIPT` | `P4-CUSTOMER-RECEIPT` | 依赖上一类型 |
| `Invoice` | `P3-NO-APPROVAL-INVOICE` | `P4-INVOICE` | `NO_APPROVAL` 证明批次 |
| `SupplierPayment` | `P3-ADAPTER-SUPPLIER-PAYMENT` | `P4-SUPPLIER-PAYMENT` | 依赖上一类型 |
| `CustomerRefund` | `P3-ADAPTER-CUSTOMER-REFUND` | `P4-CUSTOMER-REFUND` | 依赖上一类型 |
| `SupplierRefund` | `P3-ADAPTER-SUPPLIER-REFUND` | `P4-SUPPLIER-REFUND` | 依赖上一类型 |
| `ReceiptReversal` | `P3-ADAPTER-RECEIPT-REVERSAL` | `P4-RECEIPT-REVERSAL` | 依赖上一类型 |
| `PaymentReversal` | `P3-ADAPTER-PAYMENT-REVERSAL` | `P4-PAYMENT-REVERSAL` | 依赖上一类型 |
| `SalesReturnCase` | `P3-NO-APPROVAL-SALES-RETURN-CASE` | `P4-SALES-RETURN-CASE` | `NO_APPROVAL` 证明批次 |
| `PurchaseReturnOrder` | `P3-NO-APPROVAL-PURCHASE-RETURN-ORDER` | `P4-PURCHASE-RETURN-ORDER` | `NO_APPROVAL` 证明批次 |
| `PurchaseReceipt` | `P3-NO-APPROVAL-PURCHASE-RECEIPT` | `P4-PURCHASE-RECEIPT` | `NO_APPROVAL` 证明批次 |
| `Delivery` | `P3-NO-APPROVAL-DELIVERY` | `P4-DELIVERY` | `NO_APPROVAL` 证明批次 |
| `ElectronicDelivery` | `P3-NO-APPROVAL-ELECTRONIC-DELIVERY` | `P4-ELECTRONIC-DELIVERY` | `NO_APPROVAL` 证明批次 |
| `ServiceFulfillment` | `P3-NO-APPROVAL-SERVICE-FULFILLMENT` | `P4-SERVICE-FULFILLMENT` | `NO_APPROVAL` 证明批次 |
| `CustomerAcceptance` | `P3-NO-APPROVAL-CUSTOMER-ACCEPTANCE` | `P4-CUSTOMER-ACCEPTANCE` | `NO_APPROVAL` 证明批次 |

表中 19 组 P3/P4 对象均已在 `_meta.json.perDocumentTypeStages` 登记完整分支、依赖、所有权、删除项和验收命令。`NO_APPROVAL` 类型也必须有独立证明批次，禁止把它们推迟到 `P6-FINAL` 临时补测。

### 3.1 `BusinessDocument` 注册前置

`P3-ADAPTER-BASE` 必须先完成 `BusinessDocument` 注册清点并补齐缺失注册，否则统一绑定端口无处落地。当前基线只有下列域接触 `BusinessDocument`：

```text
services/src/sales_order/**
services/src/sales_review/**
services/src/receivable/**
services/src/legacy_import/**
services/src/file_asset/**
```

因此 `purchase_order`、`inventory`、`payable`、`returns`、`fulfillment` 全域的创建事务都需要新增 `BusinessDocument` 注册。清点结果和逐域补齐必须在 `P3-ADAPTER-BASE` 的 PR 中列出，不得散落到各类型批次里临时补。

这五个域的 Service 目录同时是各 `DocumentType` 子阶段的 `owns`，因此 `_meta.json` 用 `ownsWithin` 分段登记：`P3-ADAPTER-BASE` 在这些目录内**只允许**补 `BusinessDocument` 注册，状态机、审批适配和提交命令一律留给各类型子阶段。两者存在依赖顺序，不会并行。

## 4. P0 amendment 记录

| 编号 | 主题 | 分支 | 状态 |
| --- | --- | --- | --- |
| P0-A | `bpm` 地基与依赖边界 | `chore/erp-p0-amend-approval-workflow-foundation` | 未开始 |
| P0-B | 共享接线与权限生成 | `chore/erp-p0-amend-approval-workflow-wiring` | 未开始 |
| P0-C | 前端 W24 与权限生成 | `chore/erp-p0-amend-approval-workflow-frontend-registry` | 未开始 |
| P0-D | 全类型硬切换清理 | `chore/erp-p0-amend-approval-workflow-hard-cutover-cleanup` | 阻塞于全部逐类型阶段 |

新增冻结修改必须追加一行单主题 amendment，合并后所有在途分支 rebase。

## 5. 开发环境切换状态

| 步骤 | 状态 | 证据 |
| --- | --- | --- |
| 停止写入进程 | 未开始 | — |
| reset preview 与脱敏报告 | 未开始 | — |
| 显式范围确认 | 未开始 | — |
| reset execute | 未开始 | — |
| reset verify | 未开始 | — |
| 部署新代码 | 未开始 | — |
| 创建并验证新索引 | 未开始 | — |
| readiness 与旧符号清零 | 未开始 | — |
| 发布 12 个 `PROCESS_REQUIRED` 定义 | 未开始 | — |
| 试点冒烟 | 未开始 | — |
| 全量类型冒烟 | 未开始 | — |

本专项不设置可回退旧运行时的全局开关。`P6-PILOT` 后只有已完成目标 rollout 的类型可进入新运行时；未接入的 `PROCESS_REQUIRED` 类型必须返回 `APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER`，不得回退旧路径。全部类型完成后执行 `P0-D` 清除旧代码。

## 6. 完成定义

只有 `P6-FINAL` 全部门禁通过，才允许把 `docs/approval-workflow-contract.md` 的状态改为「已实施」。任何单一阶段完成、局部编译通过、页面可见、`P6-PILOT` 通过或重置脚本成功均不构成整体交付。
