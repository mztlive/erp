# Phase 3：供应商商品库、供给与公司商品销售查询

## 1. 分支与隔离

| 项目 | 约定 |
| --- | --- |
| 分支名 | `codex/backend-p1-03-catalog-product-pool` |
| 基线 | 与 Phase 1～10 相同的冻结 `BACKEND_PHASE_BASE_SHA` |
| 实现语言 | 读取冻结基线中的统一后端语言/版本决定；缺失时停止，不自行选栈 |
| 独占目录 | `backend/modules/catalog-product-pool/**` |
| 编译要求 | 不要求根工程编译；要求业务模型、命令、端口、测试向量可审查 |
| 禁止修改 | 共享入口、全局 OpenAPI、物理迁移、其他 phase 目录和前端 |

## 2. 目标与唯一写者

本 phase 统一实现第一期的：

- 分类、品牌、单位、规格字典；
- 公司 `product` / `sku` 稳定身份和不可变修订；
- 供应商商品 SPU/SKU、来源修订、映射和导入批次语义；
- `supplier_offering` / `supplier_offering_revision` 多供应商供给；
- 公司 `sku_revision` 中的销售可见价，以及由 SKU 和供给派生的销售资格；
- W14 商品/SKU 与 W21 供应商商品库所需查询和命令。

对象所有权必须严格分开：

- 公司 SKU 只保存稳定商品资料，不保存默认供应商、采购成本、履约责任、进项税率、
  代发/起批等供给字段。
- 供应商商品库保存来源身份和来源报价，不直接成为公司 SKU。
- 逐供应商的一件代发供给价、集采供给价、集采起订量、税率和有效期只属于供给修订；不设置供给方式字段，每条供给默认同时支持一件代发与集采。
- 公司商品池只是公司 `product` / `sku` 的业务称呼和销售查询视图，不是独立聚合、稳定对象或修订表。`sales_visible_price_gross` 保存于 `sku_revision`；销售查询/搜索/导出只读符合资格的 SKU 修订，绝不返回供应商成本。

依据：`erp-phase-1.md` §4.4、§5.1、§5.3；`erp-data-model.md` §6.3、§6.14、§10；
W14、W21。

## 3. 独立目录结构

```text
backend/modules/catalog-product/
  domain/{dictionary,company_product,supplier_catalog,sku_mapping,offering,sales_catalog}/
  application/{commands,queries}/
  ports/
  contracts/
  persistence-spec/
  fixtures/
  tests/
  DECISIONS.md
```

Phase 2 的供应商和 Phase 5 的采购都只通过稳定引用端口交互，不导入它们的实现。

## 4. 核心命令和查询

### 4.1 公司商品与销售查询

- `CreateCompanyProduct`、`AppendCompanyProductRevision`；
- `CreateCompanySku`、`AppendCompanySkuRevision`；

`sales_visible_price_gross` 与 `market_price` 属于 `sku_revision`。公司商品池不接受创建、
修订或启停命令；销售资格由“SKU 启用、当前业务日有效的 SKU 修订有非负销售可见价、
至少一条有效 `supplier_offering_revision`”在查询和销售提交时派生，Phase 10 负责把该
跨对象 guard 绑定到真实事务。

### 4.2 供应商商品与供给

- `ApplySupplierCatalogImportIntent`：第一期仅 `MANUAL` / `EXCEL`；
- `AppendSupplierCatalogProductRevision`、`AppendSupplierCatalogSkuRevision`；
- `MapSupplierSkuToCompanySku`；
- `CreateSupplierOffering`、`AppendSupplierOfferingRevision`、`EndSupplierOffering`；
- `CreateCompanySkuAndMapSupplierCatalogSku`：按供应商 SKU 粒度原子反向创建公司商品/SKU、
  映射和供给；不接受仅 SPU 身份。

前端当前存在三种创建能力；本次确认新增供应商 SKU 反向建公司 SKU 的入池分支。本 phase
按以下边界实现：

- W14 `/master-data/products/new` 继续独立创建公司商品及一个或多个公司 SKU；
- W21 手工录入独立创建供应商商品及一个或多个供应商 SKU，不建立公司映射；
- W14 可固定公司 `sku_id` 正向发起 W21 创建供应商商品/SKU，并在同一业务动作中建立精确
  映射与供给；不得写公司 `product` / `sku` 修订或改变销售可见价；
- W21 可选择已有公司 SKU；没有同款时调用
  `CreateCompanySkuAndMapSupplierCatalogSku`。命令固定一个 `supplierCatalogSkuId`，以
  来源同字段预填公司草稿并允许采购修改；独立 `productKind`、销售可见价和市场价必填。公司商品/SKU、映射、
  双价供给修订、审计、幂等结果和 outbox 必须在单一数据库事务提交；销售可见价和市场价写入
  新创建的 `sku_revision`，不创建商品池对象；
- W14 正向创建与 W21 反向创建都必须显式提交独立必填 `productKind`。它写入
  `product.product_kind` 后永久不可变，决定商品业务作用；`categoryId` 只参与兼容性校验，
  不得用于派生或覆盖 `productKind`。反向创建可由可靠 `sourceProductKind` 预填，但采购必须
  最终确认；来源缺失时空白必填；
- 页面导航中的 `supplierProductId`、SPU 或其他上下文不得替代正式
  `supplier_catalog_sku_id → sku_id` 映射，也不得因来源变化自动覆盖另一侧修订。

### 4.3 查询与权限投影

- W14 公司商品、SKU、销售资格及供给摘要；
- W21 供应商商品、映射、逐供应商供给和修订时间线；
- 销售端 `SalesCatalogQuery`：只返回 SKU 已启用、当前业务日有效的 `sku_revision` 含非负销售
  可见价、且至少有一条有效供给并匹配区域/履约方式的公司 SKU；返回精确 `sku_revision_id`；
- 销售单行必须保存 `sku_revision_id` 与当次 `unit_price_gross` 成交价快照；后续 SKU 价格或
  供给变化不得改写历史行，不引用商品池条目或修订；
- 采购端 `ProcurementOfferingQuery`：返回授权的供应商供给和成本事实；
- 销售导出使用专门 DTO，类型上不包含 supplier、cost、input tax、MOQ 字段。

## 5. 领域不变量

- Excel、手工和未来 API 共用一套供应商 SPU/SKU 身份；API 连接不是非 API 来源的父对象。
- W18 只编排批次、manifest 和责任确认；供应商目录行的领域校验、修订和应用仍由本
  phase 唯一负责，Phase 8 只能提交强类型导入意图。
- `supplier_catalog_intake_batch/item` 是本域的目录摄取事实；Phase 8 的
  `legacy_import_batch/row` 是开账治理记录。两者通过稳定批次引用关联，不能各自重复写
  同一供应商目录/供给事实。
- 同一供应商 SKU 同一时点只映射一个公司 SKU；一个公司 SKU 可以有多家供应商供给。
- 供应商商品修订、映射、供给修订和公司 SKU 修订是不同事实，不能互相覆盖。
- 来源代发/集采报价、采购确认后的两项供给价、销售可见价和未来商城发布价是不同价格事实。
- 供给有效期不得重叠；采购提交按业务日期重验供给修订、能力、资质和供应商状态。
- 销售资格不能凭 UI 摘要或独立商品池状态决定；必须在查询和销售提交事务内重验 SKU 启用、
  销售可见价和有效供给。
- 规格身份和基础单位被正式单据引用后不能就地改变；以新 SKU 或纠正修订处理。
- 公司商品规格编辑只按规范化 `specification_signature` 延续稳定 `sku_id`：签名未变保留，
  新签名分配新 ID，移除签名保留历史并停用旧 SKU；`sku_no`、数组位置和单行回退都不是
  身份匹配依据，既有/历史 `sku_no` 不得重绑其他签名。
- 已停用的历史签名再次出现时必须复用原 `sku_id`，追加修订并显式重新启用；命令必须携带
  原 SKU 的期望修订和变更原因，并通过权限、资料完整性、库存/预占策略、合规与业务 blocker，
  不得静默创建第二个同签名 SKU 或自动启用。
- 同一次规格编辑必须以商品及受影响 SKU 的期望修订，在一个事务内完成保留、新建、重新启用和停用；
  任一签名重绑、业务编码冲突或并发冲突整体失败。
- 销售查询、缓存、索引、导出和错误信息都不得包含采购成本。

## 6. 必须补齐的契约孔洞

以下问题属于当前文档冲突，不能由本 phase 静默选择：

1. **[已解决，2026-08-04] 供应商目录阶段归属**：供应商商品库、供给、映射相关表已由
   `erp-data-model.md` 表目录与阶段矩阵列为一期（第一期来源仅开放 `MANUAL` / `EXCEL`），
   不存在两套物理表或独立商品池表风险。该问题已解决，决策记录见 phases-10.md §102-106。
2. 一期只登记 API 能力，不建设 API 连接/同步；W21 文档混有 API 来源入口。
   本 phase 保留 `source_type=API` 的未来兼容语义但运行时拒绝 API intake，W20 属第二期。
3. W21 修订命令出现 `input_tax_rate`，而统一模型不允许供应商目录保存进项税率；该字段必须
   移到供给修订，不得进入目录修订。
4. **[已确认，2026-08-04] 提升入池的唯一正式粒度是 supplier catalog SKU**：单项命令
   必须显式携带 `supplierCatalogSkuId + targetCompanySkuId`；批量操作只接受由这些单项组成的
   `items[]`。supplier product/SPU 只作为页面容器和批量选择范围，不得形成 SPU 级映射，
   也不得隐式映射未选择的兄弟 SKU。
5. `procurement_confirmation_line` 缺少精确 `supplier_offering_revision_id`；本 phase 先提供
   `OfferingRevisionRef`，Phase 10 补真实 FK、同供应商/SKU和业务日有效性约束。
6. 供给不设置 `supply_mode`；入池命令必须同时提交一件代发供给价、集采供给价和集采
   起订量，缺任一项均 fail-closed，不能把两项价格折叠成单一 `confirmed_cost_gross`。
7. **[已确认，2026-08-04] 供给条件预填**：进项税率、供给区域和生效日期必须显式展示并
   允许采购修改；仅当存在供应商开票资料/税务策略、供应商能力/供给策略或服务端业务日期
   时自动预填，并携带策略修订或业务日期快照。无可靠来源时空白必填；禁止 `0.13`、“全国”、
   浏览器当天或单一确认成本成为静默默认。

## 7. 测试要求

1. 同一来源批次/行幂等，A→B→A 修订仍保留历史。
2. 供应商 SKU 并发映射、一个有效映射限制和多供应商供给。
3. 供给生效区间重叠、未来生效、失效和业务日重验。
4. `SalesCatalogQuery` 与销售提交在 SKU 停用、当前 `sku_revision` 无销售可见价或无有效供给时均拒绝；不以独立商品池状态判断。
5. 销售 DTO、搜索索引和导出结构无成本/供应商敏感字段。
6. 采购端在权限不足时只返回可销售事实，不通过错误文案泄露成本。
7. 同一 SPU 的多个供应商 SKU 可分别映射不同公司 SKU；部分选择不得影响未选 SKU，
   任一单项缺供给价/集采起订量或引用错误 offering revision 时，该单项 fail-closed。
8. API 来源在一期能力 gate 下拒绝，MANUAL/EXCEL 正常进入同一模型。
9. 税率、区域、生效日期无可靠来源时缺失全部 fail-closed；有来源时断言策略版本/服务端
   业务日期快照被审计，采购修改后的最终值而非建议值进入供给修订。
10. W14 正向创建和 W21 反向创建缺少 `productKind` 都必须拒绝；可靠
    `sourceProductKind` 只能预填，采购最终确认值写入稳定 `product.product_kind`；分类与
    类型不兼容时拒绝，创建后任何修改 `productKind` 的命令都拒绝。
11. 商品规格组合未变时保持原 `sku_id`；新增组合获得新 `sku_id`；移除组合的旧 SKU 被停用
    且历史修订仍可读取。伪造相同 `skuNo`、复用旧 `skuId` 绑定新签名、调整数组顺序或触发
    单行回退都不能改变结果，任一非法重绑使整次商品编辑回滚。
12. 已停用签名再次加入规格集合时，查询返回原 `sku_id`、期望修订与 blocker；只有明确
    重新启用意图和原因且 blocker 为空时，原 SKU 才追加修订并重新启用。缺确认、存在 blocker
    或并发冲突时，连同同次保留、新建和停用变化全部回滚。
13. 重新启用后，原有 W21 `supplier_catalog_sku_id → company_sku_id` 映射仍指向同一 SKU，
    不创建或迁移映射；销售资格仍由重新启用后的当前 SKU 修订和当前有效供给重新派生。
14. W21 反向创建分支不接受客户端 `companySkuId`，始终为新签名分配新公司 SKU 身份；
    关联已有分支固定 `companySkuId + expectedCompanySkuRevisionId` 且不能改变规格签名。

## 8. 完成标准

- W14 与 W21 的对象归属、SKU 修订价格事实、派生销售资格和权限边界由类型与测试共同证明。
- 仅写独占目录，不创建全局路由、物理 DDL 或跨 phase 外键。
- API 供应商连接、商城商品发布等二期能力没有被一期入口误启用。
- 向 Phase 10 交付逻辑 schema、端口、DTO、错误码、跨对象 guard 和六项文档冲突清单。
