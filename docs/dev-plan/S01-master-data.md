# S01 基础资料

## 1. 元信息

- 分支：`feat/erp-s01-master-data`
- 业务期：`p1`（一期，见 `docs/erp-phase-1.md` §5.1 基础资料）
- 依赖阶段：`S00`（IAM/账号/角色/审计/上传基建就绪）
- 本阶段是否要求单独编译：`must_compile=false`
  - **说明**：中间阶段仅实现 `owns_modules` 内 entities / repository / services / handler 文件；允许因缺少 `lib.rs`/`mod.rs`/routes/`DatabaseExt`/`ensure_indexes` 注册而无法通过 workspace 编译。共享汇合文件**禁止本阶段并行修改**；须在 `docs/dev-plan/S01-PATCHNOTES.md`（或 PR 描述）声明应追加的 `mod`/`pub use`/`DatabaseExt`/`ensure_indexes`/nest 路由与 permission 键。合并进集成分支后由「最终集成汇合」阶段保证 `cargo check`。
- 阶段目标一句话：为合同/销售/采购/履约提供稳定可引用的伙伴、商品、仓库主数据身份与不可变修订。

## 2. 目标与业务范围

**必须交付（仅下列，禁止扩 scope）**

1. **业务伙伴 `party` 体系**（`erp-data-model.md` §6.2；W03 §1/§5）
   - 稳定 `party` + 不可变 `party_revision`（法定名称/简称/生效区间/变更原因）
   - 角色：`customer_account`、`supplier_account`（一 party 最多一有效客户角色、一有效供应商角色）
   - 附属：`party_contact`、`party_address`、`party_tax_profile`、`party_bank_account`（有效期；银行密文 + `account_number_query_hmac`）
2. **客户归属 `customer_assignment`**（data-model §6.2；W03 §5.1）
   - `OWNER`/`COLLABORATOR`；同一客户同一时点恰好一个 OWNER；有效期不重叠
3. **供应商商业资料族**（data-model §6.2；W14 §5.2.2）
   - `supplier_commercial_profile_revision`、`supplier_capability`+revision、`supplier_qualification`+revision、`supplier_rating_revision`
4. **商品字典与 SPU/SKU**（data-model §6.3；W14 §5.2.1）
   - 字典：`product_category`（树+防环）、`product_brand`、`unit_of_measure`、`sku_attribute`/`sku_attribute_value`、`product_category_attribute`
   - 稳定 `product`（`product_kind` 创建后不可变）+ `product_revision` + media；`sku`（`specification_signature` 永久唯一）+ revision
5. **卡券类目扩展** `voucher_category_profile_revision`（**不含玩法**）
6. **仓库** `warehouse`/`warehouse_revision` + `warehouse_sku_policy`（仅最低可用量预警；**不写库存余额**；写操作默认 fail-closed：`WAREHOUSE_WRITE_OWNER_UNCONFIRMED`）
7. **W14 七资源读模型**：`sellable-items`|`products`|`categories`|`brands`|`voucher-categories`|`suppliers`|`warehouses`（`sellable-items` 只读投影，非独立写入对象）
8. **W03 客户主数据写/读前置**：新建客户同事务 party+revision+customer_account+OWNER

**文档依据**：`erp-phase-1.md` §5.1；`erp-data-model.md` §5.2、§6.2、§6.3；`w14-basic-data.md`；`w03-customer-center.md`。

## 3. 明确不在范围

- 玩法规则纳入 ERP；卡券生产/卡号/实体卡/激活；完整员工档案
- 仓库调拨/盘点/批次保质期；完整生产制造与复杂 BOM
- **W21** offering 写路径、**W20** API 连接、**W22** 商城发布
- 库存余额/流水、合同/销售/采购单据
- 跨资源统一复核审批流；可配置审批流/事件总线
- 独立 `product_pool_entry`；未写入 data-model 的集合/字段

## 4. 代码落点与目录布局（强制统一风格）

对齐 `backend/AGENTS.md`：HTTP Handler → Service → Repository → MongoDB。

### 4.1 建议树（仅 owns_modules）

```text
backend/entities/src/
  party/ party_revision / contact / address / tax / bank
  customer_account/ supplier_account/ customer_assignment/
  supplier_commercial/ capability/ qualification/ rating/
  product_category/ product_brand/ unit_of_measure/ sku_attribute/
  product/ sku/ voucher_category/ warehouse/

backend/database/src/repository/  # 与上对应 command/query/transaction

backend/services/src/
  party/{mod,dto,create,revise,query,assignment,details,status,sensitive}
  supplier_profile/{mod,dto,create,revise,query,status}
  product/{mod,dto,create,revise,query,dictionary,voucher_category,status}
  warehouse/{mod,dto,query,write_gate}
  master_data/{mod,dto,query}  # 七资源投影

backend/apps/web-api/src/core/handler/admin/
  master_data/ party/ product/ supplier_profile/ warehouse/
```

### 4.2 owns_modules 与汇合

- **允许改**：规划 JSON `owns_modules` + `docs/dev-plan/S01-PATCHNOTES.md`
- **禁止改**：`entities/lib.rs`、`services/lib.rs`、`repository/mod.rs`、`extensions.rs`、`indexes.rs`、`handler/admin/mod.rs`、`routes/**`、`app_state`/`main`

### 4.3 编码约束

分层无穿透；DTO 在 services；多集合事务；乐观锁；方法≤30 行；完整文档注释；敏感字段密文+HMAC；幂等键+lock_version。

## 5. 数据模型与索引

集合：`parties`、`party_revisions`、`customer_accounts`、`supplier_accounts`、`party_contacts/addresses/tax_profiles/bank_accounts`、`customer_assignments`、`supplier_*_revisions`、`product_*`、`skus`/`sku_revisions`、`voucher_category_profile_revisions`、`warehouses`、`warehouse_sku_policies`。

关键索引：`party_no`/`unified_credit_code` 唯一；`sku (product_id, specification_signature)` 永久唯一；`warehouse_code` 唯一；OWNER 单点由 Service 事务保证。索引函数写 PATCHNOTES 供集成 `ensure_indexes`。

## 6. API 与权限草图

| 路径前缀 | 权限 resource 建议 |
| --- | --- |
| `GET/POST /admin/master-data/{products,categories,brands,voucher-categories,suppliers,warehouses}` | `master_data_*` list/get/create/create_revision/disable |
| `GET /admin/master-data/sellable-items` | 只读 list |
| `GET/POST /admin/customers`、revisions/details/assignments/disable/sensitive/reveal | `customer`/* |
| `POST /admin/master-data/sensitive/reveal` | 字段级 reveal + 审计 |

列表返回 `lifecycleStatus`、`lockVersion`、`allowedActions`、`actionBlockers`。仓库写未确认责任方 → `WAREHOUSE_WRITE_OWNER_UNCONFIRMED`。

## 7. 前端集成点

- `erp-client/features/master-data`、`customers`；`entity-comboboxes.tsx`
- 保留 mock 开关；新增 `api.ts`；query keys：`masterDataKeys`、`customerKeys`
- `sellable-items` 仅 GET；禁止本地日期推导启停

## 8. 实现任务清单

1. entities 全族 + 签名规范化/分类防环/OWNER 唯一单测
2. repository command/query（不改 extensions/indexes）
3. services：party/supplier_profile/product/warehouse/master_data
4. handler + permission 宏；PATCHNOTES 路由清单
5. 测试 + `S01-PATCHNOTES.md`

## 9. Worktree / 并行约定

```bash
git worktree add ../erp-s01 -b feat/erp-s01-master-data <S00合并点>
```

禁止触碰汇合文件与 W21/W05/W08/W10 单据。可与不碰主数据文件的阶段并行。`depends_on: S00` 约束合并顺序。

## 10. 验收标准

### 10.1 功能

- [ ] 客户新建四件套；OWNER 单点；银行密文+HMAC；敏感揭示可审计
- [ ] 供应商商务/能力/资质/评级修订；有效期不重叠
- [ ] 商品 product_kind 不可变；SKU 签名永久唯一；规格四类集合原子
- [ ] 卡券类目无玩法字段；仓库写 fail-closed；七资源投影；sellable-items 只读；幂等

### 10.2 风格

- [ ] 分层/DTO/Service 拆文件/文档注释/事务/BaseModel

### 10.3 文档范围

- [ ] 无 out_of_scope；无未列表；PATCHNOTES 完整

### 10.4 编译

- `must_compile=false`；集成阶段应用 PATCHNOTES 后 `cargo fmt/check/clippy/test --workspace`

---

*阶段 ID：S01 · 分支：feat/erp-s01-master-data · phase_tag：p1 · must_compile：false*
