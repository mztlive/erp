# S04 采购二次确认采购单与供应商商品库

## 1. 元信息

- 分支：`feat/erp-s04-procurement`
- 业务期：`p1`
- 依赖阶段：`S01`、`S03`
- `must_compile=false`；PATCHNOTES：`docs/dev-plan/S04-PATCHNOTES.md`

## 2. 目标与业务范围

### 2.1 W07 采购二次确认

- 仅实物与服务销售单；确认对象=不可变 `sales_order_submission`
- `PROCUREMENT_CONFIRMATION`：领取/草稿/通过/驳回/暂挂
- 通过同事务：确认事实+销售生效协作+完成 work_item+`procurementCreationBasisId`
- 驳回：结构化 reason；旧任务 COMPLETED；**不**建后继任务
- 分行绑定有效 `supplier_offering_revision`；**不**注册 business_document

### 2.2 W08 采购单 + 变更

- 建单入口：W07 依据 / W05「去建单」；拆单键 supplier×purchase_type×payment_term×fulfillment_responsibility
- draftEditToken + lock_version；首次提交冻结 submission+purchase_no+PURCHASE_ORDER_REVIEW
- 财务通过 → revision+allocation+应付协作接口；变更完整目标提交

### 2.3 W21 供应商商品库

- 来源仅 MANUAL/EXCEL；统一 catalog + mapping + offering 双价
- Excel intake / 手工录入 / 映射或反向建品同事务

依据：phase-1 §4.4/§7；w07/w08/w21；data-model §6.5/§6.6/§6.14。

## 3. 明确不在范围

询价比价；动态比价/智能路由；H5 供应商；履约过账（属 S05）；API 目录连接器（S09）；卡券二次确认链路；采购建单 work_item；应付付款写路径（W12）；汇合文件。

## 4. 代码落点

### owns_modules

- entities：`procurement_confirmation`、`purchase_order`、`purchase_change`、`supplier_catalog`、`supplier_offering`
- repository 同上；services：`procurement`、`purchase`、`supplier_catalog`
- handler：`admin/procurement`、`purchase`、`supplier_catalog`

跨阶段：S03 销售生效/驳回协作；work_item 仅注册表类型；W14 建品接口；应付 trait 依赖（不自建 payable 表）。

## 5. 数据模型与索引

表清单见阶段 JSON `data_model_tables`。关键：确认同 submission 一批；purchase_no 仅正式；offering 有效期不重叠；映射同时一有效目标。状态枚举禁止自创。

## 6. API 与权限草图

- 确认：queue、draft、complete、defer；work-items claim 复用 W02
- 采购：list/creation-bases/from-basis/draft-token/draft/submit/review/void/changes
- 目录：catalog CRUD、intake、link-company-sku、create-company-product、offerings
- 幂等：`GET .../actions/{idempotencyKey}`
- permission keys 写入 PATCHNOTES

## 7. 前端集成点

- `features/procurement-confirmation`、`purchase-orders`、`supplier-catalog`
- 替换 api.ts；keys 不变；成功写后 invalidate `unifiedQueueKeys`
- 一期禁用 source_type=API 写入

## 8. 实现任务清单

建模 → 仓储 → services（approve 事务/from_basis/intake）→ handler → S04-PATCHNOTES → 测试（覆盖规则/拆单键/双价/拒绝 API source）

## 9. Worktree / 并行约定

`feat/erp-s04-procurement`；合并前须 S01+S03；接口冻结：ApproveSalesFromProcurement、CreateCompanyProductSku、work_item 类型码。

## 10. 验收标准

- [ ] W07 领取→通过得 basisId；驳回无后继；暂挂回 UNCLAIMED
- [ ] W08 草稿→提交→审核通过 revision；变更不覆盖原版
- [ ] W21 Excel/手工→映射/建品→双价；API 写入拒绝
- [ ] 风格/文档范围；`must_compile=false`

---

*阶段 ID：S04 · 分支：feat/erp-s04-procurement · phase_tag：p1 · must_compile：false*
