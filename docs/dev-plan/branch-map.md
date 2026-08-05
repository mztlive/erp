# 分支 ↔ owns 路径对照表

开 worktree 时：只改本表「owns」列路径；共享汇合文件仅 **S11** 修改。

## 快速开树

```bash
# 示例
git worktree add ../erp-s00 -b feat/erp-s00-document-infra <base>
git worktree add ../erp-s01 -b feat/erp-s01-master-data <after-S00>
# …
git worktree add ../erp-s11 -b feat/erp-s11-integration-merge <after-S10>
```

## 对照表

| 阶段 | 分支 | owns 路径（实现唯一可写代码/清单） |
| --- | --- | --- |
| S00 | `feat/erp-s00-document-infra` | `backend/entities/src/{source_system,external_identity,business_document,document_relation,document_participant,workflow_action,work_item,bulk_selection,background_job,audit_event,file_asset,document_attachment,data_scope}`；`backend/database/src/repository/` 同上域名；`backend/services/src/{document_infra,work_item,background_job,file_asset}`；`backend/apps/web-api/src/core/handler/admin/{document_infra,work_item,background_job,file_asset}`；`docs/dev-plan/S00-PATCHNOTES.md` |
| S01 | `feat/erp-s01-master-data` | `backend/entities/src/{party,customer_account,supplier_account,customer_assignment,supplier_commercial,supplier_capability,supplier_qualification,supplier_rating,product,sku,product_category,product_brand,unit_of_measure,sku_attribute,voucher_category,warehouse}`；对应 `repository/*`；`backend/services/src/{master_data,party,supplier_profile,product,warehouse}`；`handler/admin/{master_data,party,product,warehouse,supplier_profile}`；`docs/dev-plan/S01-PATCHNOTES.md` |
| S02 | `feat/erp-s02-customer-contract` | `backend/entities/src/contract`（及客户服务所需 party/customer/files/jobs 路径以阶段文档 owned_paths 为准）；`repository/contract`；`services/{customer,contract}`；`handler/admin/{customer,contract}`；`docs/dev-plan/S02-PATCHNOTES.md`；前端 `erp-client/features/{customers,contracts}` |
| S03 | `feat/erp-s03-sales` | `backend/entities/src/{sales_order,sales_change,customer_acceptance}`；对应 repository；`services/sales`；`handler/admin/sales`；`docs/dev-plan/S03-PATCHNOTES.md`；`erp-client/features/sales-orders` |
| S04 | `feat/erp-s04-procurement` | `backend/entities/src/{procurement_confirmation,purchase_order,purchase_change,supplier_catalog,supplier_offering}`；对应 repository；`services/{procurement,purchase,supplier_catalog}`；`handler/admin/{procurement,purchase,supplier_catalog}`；`docs/dev-plan/S04-PATCHNOTES.md`；`erp-client/features/{procurement-confirmation,purchase-orders,supplier-catalog}` |
| S05 | `feat/erp-s05-fulfillment-inventory` | `backend/entities/src/{purchase_receipt,delivery,electronic_delivery,service_fulfillment,stock}`；对应 repository；`services/{fulfillment,inventory}`；`handler/admin/{fulfillment,inventory}`；PATCHNOTES；`erp-client/features/{fulfillment-operations,inventory}` |
| S06 | `feat/erp-s06-finance` | `backend/entities/src/{receivable,payable,customer_receipt,supplier_payment,invoice,cost,sales_return,purchase_return,customer_refund,supplier_refund,receipt_reversal,payment_reversal}`；对应 repository；`services/{finance,customer_quality,profit_loss}`；`handler/admin/{finance,customer_quality,profit_loss}`；`docs/dev-plan/S06-PATCHNOTES.md`；`erp-client/features/{customer-receivables,supplier-payables,card-funds-review,customer-quality,actual-profit-loss}` |
| S07 | `feat/erp-s07-workbench` | `backend/services/src/workspace`；`handler/admin/workspace`；`docs/dev-plan/S07-PATCHNOTES.md`；`erp-client/features/{workspace,unified-task-queue}`（复用 S00 `work_item` 仓储，不平行实现） |
| S08 | `feat/erp-s08-mall-sync-p1` | `backend/entities/src/{mall_sales_sync,master_mapping,legacy_import}`；对应 repository；`services/{mall_sync,legacy_import}`；`handler/admin/{mall_sync,legacy_import}`；`docs/dev-plan/S08-PATCHNOTES.md` |
| S09 | `feat/erp-s09-integration-p2` | `backend/entities/src/{supplier_api,product_publication,sales_order_projection,mall_consumption,mall_after_sales,supplier_fulfillment,supplier_settlement,integration_governance}`；对应 repository；`services/{supplier_api,product_publication,sales_order_projection,mall_consumption,supplier_fulfillment,supplier_settlement,integration_governance,card_business}`；`handler/admin/` 同上；`docs/dev-plan/S09-PATCHNOTES.md` |
| S10 | `feat/erp-s10-frontend-api` | `erp-client/features/{workspace,unified-task-queue,customers,contracts,sales-orders,procurement-confirmation,purchase-orders,fulfillment-operations,inventory,customer-receivables,supplier-payables,card-funds-review,master-data,customer-quality,actual-profit-loss,mall-sync,import-opening,access-audit,supplier-api-connections,supplier-catalog,product-publications,execution-projections,mall-consumption-orders,supplier-orders,supplier-settlements,card-business-analytics,integration-errors,history-backfill}`；`erp-client/lib`；`erp-client/PATCHNOTES.md` |
| S11 | `feat/erp-s11-integration-merge` | **仅汇合文件**：`backend/entities/src/lib.rs`；`backend/services/src/lib.rs`；`backend/database/src/repository/mod.rs`；`extensions.rs`；`indexes.rs`；`handler/mod.rs`；`handler/admin/mod.rs`；`routes/mod.rs`；`routes/admin.rs`；`app_state.rs`；`main.rs`；`build.rs`；`backend/fronts/admin/src/constants/permissions.generated.ts` |

## 共享汇合（禁止 S00–S10 改）

| 路径 | 所有者 |
| --- | --- |
| `backend/entities/src/lib.rs` | S11 |
| `backend/services/src/lib.rs` | S11 |
| `backend/database/src/repository/mod.rs` | S11 |
| `backend/database/src/repository/extensions.rs` | S11 |
| `backend/database/src/indexes.rs` | S11 |
| `backend/apps/web-api/src/core/handler/mod.rs` | S11 |
| `backend/apps/web-api/src/core/handler/admin/mod.rs` | S11 |
| `backend/apps/web-api/src/core/routes/**` | S11 |
| `backend/apps/web-api/src/app_state.rs` | S11 |
| `backend/apps/web-api/src/main.rs` | S11 |
| `backend/apps/web-api/build.rs` | S11 |
| `backend/fronts/admin/src/constants/permissions.generated.ts` | S11 |

## 依赖 DAG（合并前须满足）

```text
S00
├── S01 ── S02 ── S03 ── S04 ── S05
│           │      │      │
│           │      │      └── S06
│           │      └── S08 ── S09 ── S10 ── S11
│           └──────────────┘
└── S07 ─────────────────────────────┘
```

详细 owns 列表见各阶段文档与 [`_meta.json`](./_meta.json)。
