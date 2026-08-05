# S11 最终集成汇合

## 1. 元信息

- 分支：`feat/erp-s11-integration-merge`
- 业务期：`p1p2`
- 依赖阶段：`S10`（及前置阶段 PATCHNOTES 齐备）
- **`must_compile=true`**：**唯一**允许改共享汇合点并强制 workspace 编译的阶段
- 定位：应用全部补丁、挂接路由/权限/索引/启动链路、跨域冒烟、质量门禁

## 2. 目标与业务范围

本阶段**不新增业务域实现**，只做汇合、挂接与跨域验收。

1. **注册全部共享汇合点**  
   - `entities/lib.rs`、`services/lib.rs`、`repository/mod.rs`、`extensions.rs`、`indexes.rs`  
   - `handler/**/mod.rs`、`routes/**`、`app_state`/`main` 的 mod/pub use/DatabaseExt/nest

2. **合并 ensure_indexes 与启动 ensure 链路**  
   - 对齐 data-model 必需索引；`main`：`ensure_transaction_support` → `ensure_indexes` → `ensure_root_role`  
   - 无唯一约束不得提供写服务（fail-closed）

3. **nest 全部 admin 路由 + permission 生成 + CORS/中间件无回归**  
   - `cargo check -p web-api` 再生 `permissions.generated.ts`  
   - 禁止手改生成物语义

4. **跨域冒烟（文档主路径）**  
   - 建单→二次确认→采购→履约→票款（erp-ui-flows §2）  
   - 卡券同步→映射→复核（§3/§9）  
   - 二期发布/投影/消费/结算/错误中心（§10–§11）

5. **质量门禁**  
   ```bash
   cargo fmt --all
   cargo check --workspace
   cargo clippy --workspace --all-targets --all-features
   cargo test --workspace
   ```

6. **前端联调**  
   - `WORKSPACE_ROUTES` W01–W30（**缺 W24**）  
   - query-keys 跨域 invalidate 验证

**冒烟锚点表**：`source_system`、`business_document`、`work_item`、`party`、`contract`、`sales_order`、`purchase_order`、`stock_balance`、`receivable_account`、`payable_account`、`mall_sales_sync_job`、`product_publication`、`sales_order_projection`、`mall_order`、`supplier_fulfillment_order`、`integration_error_task`

## 3. 明确不在范围

- 新增 phase/data-model「不建设」能力（消息中间件/outbox/可配置审批/完整总账/H5/动态路由/CRM 等）
- 发明未写集合；扩大 IAM 至多租户/非 Admin kind
- 重写各域业务规则（缺陷回退 owns 阶段）；手工长期维护 permissions.generated.ts
- 把 W24 补进路由表

## 4. 代码落点

### owns_modules（仅汇合文件）

| 路径 | 职责 |
| --- | --- |
| `backend/entities/src/lib.rs` | mod + pub use |
| `backend/services/src/lib.rs` | pub mod 各域 |
| `backend/database/src/repository/mod.rs` | 子模块声明 |
| `backend/database/src/repository/extensions.rs` | DatabaseExt |
| `backend/database/src/indexes.rs` | ensure_indexes 合并 |
| `backend/apps/web-api/src/core/handler/mod.rs` | handler 根 |
| `backend/apps/web-api/src/core/handler/admin/mod.rs` | admin 子域 |
| `backend/apps/web-api/src/core/routes/mod.rs` | 主路由/CORS/中间件 |
| `backend/apps/web-api/src/core/routes/admin.rs` | nest + with_permission |
| `backend/apps/web-api/src/app_state.rs` | 按需注入服务 |
| `backend/apps/web-api/src/main.rs` | 启动 ensure 无回归 |
| `backend/apps/web-api/build.rs` | 权限扫描完整 |
| `backend/fronts/admin/src/constants/permissions.generated.ts` | 构建生成物 |

### 补丁消费流程

1. 收集 depends_on 闭包内全部 `Sxx-PATCHNOTES.md`  
2. 顺序：entities → repository mod → DatabaseExt → indexes → services → handler → routes → app_state/main  
3. 冲突以 data-model 表名为准  
4. 生成 permissions → 全 workspace 门禁 → 冒烟勾选  

风格：Handler→Service→Repository；事务在 Service；公共方法文档注释；集合名=表名。

## 5. 数据模型与索引

- 集合名 = data-model 标识符  
- DatabaseExt 方法与 `Repository::new(db, "collection")` 一致  
- 合并各域索引；**不破坏** accounts/roles/audit_logs/casbin 基线  
- 启动顺序保持 connect → AppState → ensure_transaction_support → ensure_indexes → ensure_root_role → serve  

## 6. API 与权限草图

- **S11 不设计新 REST 契约**；路径以各阶段 PATCHNOTES 与 handler 为准  
- 代表面须可注册：IAM、work-items、customers、contracts、sales-orders、procurement、purchase-orders、fulfillment、inventory、receivable/payable、card-funds、mall-sync、publications、projections、mall-orders、supplier-fulfillment、settlements、integration-errors  
- 前缀 `/admin/*`；JWT + with_permission；ApiResponse  

## 7. 前端集成点

- `erp-client/lib/workspace-registry.ts`：WORKSPACE_ROUTES 无漂移  
- features 主路径 mock 已由 S10 替换；S11 验证跨域 invalidate：  
  - salesOrderKeys ↔ contracts / procurement / fulfillment  
  - supplierPayablesKeys ↔ purchaseOrderKeys ↔ fulfillmentKeys  
  - publication/projection ↔ integrationErrorKeys  
  - unifiedQueueKeys 与单据 detail 双失效  
- W19 加载生成后的 permission groups  

## 8. 实现任务清单

1. 切分支并合并 S10 及前置；汇总 PATCHNOTES checklist  
2. entities/lib 注册  
3. repository mod + DatabaseExt + indexes  
4. services/lib 注册；抽查事务边界  
5. handler mod + admin routes merge；CORS 无回归；生成 permissions  
6. app_state/main 仅按需扩展  
7. 前端联调 WORKSPACE_ROUTES + invalidate  
8. 一期主路径 + 二期主路径冒烟；四项 cargo 门禁  

## 9. Worktree / 并行约定

- 仅本阶段改 owns 汇合文件  
- 禁止改中间阶段 owns 业务实现（除非消除编译冲突的最小 diff 且 PR 说明）  
- 中间阶段回顾：不得改本清单汇合文件，只写 PATCHNOTES  
- 合并顺序：… → S10 → **S11**（p1p2 门闩）  
- 冲突裁决：集合名/permission resource 以 data-model 与 IAM 宏约定为准  

## 10. 验收标准

### 10.1 功能

- [ ] 全部 PATCHNOTES 项已落入汇合文件  
- [ ] 启动 ensure 全通过；JWT/RBAC/CORS 无回归  
- [ ] 一期冒烟：W05→W07→W08→W09→W06→W11  
- [ ] 卡券冒烟：同步→映射→W13→W11  
- [ ] 二期冒烟：W22→W23→W25→W26→W27→W29  
- [ ] WORKSPACE_ROUTES 可达；permissions.generated 覆盖全部 admin 权限点  

### 10.2 风格

- [ ] 无 Handler 直连 DB；无 handler 重复 DTO；Service 拆文件；事务/乐观锁；rustdoc；clippy 无新增 warning  

### 10.3 文档范围

- [ ] 未实现「不建设」项；未新增 data-model 外集合；冒烟可回溯文档  

### 10.4 编译门禁（must_compile=true）

**必须**通过：

```bash
cargo fmt --all
cargo check --workspace
cargo clippy --workspace --all-targets --all-features
cargo test --workspace
```

中间阶段「缺注册无法编译」在此**清零**。

---

*阶段 ID：S11 · 分支：feat/erp-s11-integration-merge · phase_tag：p1p2 · must_compile：true*
