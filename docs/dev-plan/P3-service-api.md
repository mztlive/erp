# P3 服务与接口层（services + web-api）

| 项 | 值 |
| --- | --- |
| 分支 | `feat/erp-c-<批次>-<域简称>` |
| 并行度 | 12（前提是遵守 §2 的跨域协作规则） |
| 依赖 | 同域 P2 已合并 + `domains.md` 中「依赖域」的 P2 已合并 |
| `must_compile` | true |
| owns | `services/src/<domain>/**`、`web-api/src/core/handler/<domain>/**`、`web-api/src/core/routes/<domain>.rs` |

本层交付**可被前端调用的接口**。实现验收以编译门禁 + 接口清单 +（可选）手工/联调冒烟为准；
完整 HTTP 集成测试统一在 [P6](./P6-integration-tests.md) 收口。

Handler 与 Service 合并在同一阶段，因为 Handler 按 `AGENTS.md` 只做协议适配，
薄到不值得单列一层；而只有接口可对接，P4 才有确定的对接对象。

> **集成测试策略**：401/403/400/409/happy path 等自动化 HTTP IT **不在本层强制验收**，
> 以免拖慢服务层并行交付。P0 的 D01 样板保留可复制的 HTTP IT 写法；
> 本层可自愿补测，但不作为合并门禁。

---

## 1. 交付内容（每个域）

1. `services/src/<domain>/dto.rs`：请求/响应 DTO（含 `validator` 校验属性）。
2. `services/src/<domain>/mod.rs`（单文件时）或按用例拆分的 service 文件：流程编排、
   事务边界、跨域仓储调用。方法命名：查询用名词（`sales_order_list`），写用动词（`submit`）。
3. `web-api/src/core/handler/<domain>/`：每组接口一个文件，复用 service DTO，
   标注 `#[permission_macros::permission(...)]`。
4. `web-api/src/core/routes/<domain>.rs`：填充 P0 预留的 `routes()`，挂 JWT + RBAC。

---

## 2. 跨域协作规则（并行的前提）

> **跨域写入调用对方域的 Repository，禁止 Service 依赖 Service。**

例如「销售单采购确认通过」（数据模型 §8.1.1）需要在一个事务内同时写销售版本、
销售状态、应收原始分录、待办和审计。实现方式：

```rust
// services/src/sales_review/mod.rs
db.client().with_transaction(|session| Box::pin(async move {
    // 本域：锁定提交、校验覆盖、形成销售版本、更新当前版本与状态
    tx_db.sales_order_revisions().create(&revision, session).await?;
    tx_db.sales_orders().update(&order, session).await?;
    // 跨域：直接用 D18 的 Repository，不经过 D18 的 Service
    tx_db.receivable_entries().create(&entry, session).await?;
    // 跨域：D03 待办、D02 审计
    tx_db.work_items().create(&work_item, session).await?;
    tx_db.audit_events().create(&audit, session).await?;
    Ok::<(), database::Error>(())
})).await?;
```

配套规则：

- 跨域写入所需的**业务判定**必须来自 `entities`（对方域的实体方法或 `common` 值对象），
  不得在本域复制对方域的规则。若发现需要复制，说明该规则应下沉——走地基修订 PR。
- 跨域**读取**同样走对方 Repository。
- 若两个域互相需要对方的编排（真正的双向依赖），说明域边界划错了，
  在 PR 中提出并调整 `domains.md`，不要用 service→service 绕过。

违反本节会把 12 个并行单元退化成一条串行链，评审时按"打回"处理。

---

## 3. 事务要求

- 边界只在 Service，统一 `database::Transactional::with_transaction`。
- 单集合、无跨步骤原子性要求的 CRUD 传 `&mut NoTransaction`。
- 涉及 RBAC policy 的写入用 `run_authorized_audited_policy_transaction`；
  其他跨审计集合写入用 `run_audited_transaction`（见 `database/TRANSACTIONS.md`）。
- 事务内不做外部 HTTP、文件 I/O、CPU 密集工作。二期供应商 API 调用
  （D25、D32）必须在事务**之外**完成，用 `inbox_message` + `integration_error_task`
  承接结果，禁止把外部调用放进事务闭包。
- `CommitOutcomeUnknown` 映射为 `services::Error::OutcomeUnknown`，不得当作确定回滚重放。
- 涉及资金或状态机变更的入口必须有幂等键或去重机制（`AGENTS.md` 外部依赖容错）。

### 3.1 本域必须实现的事务不变量

数据模型第 8 章逐条分配到域，实施者在 PR 中列出本域覆盖的条目号：

| 不变量 | 归属域 |
| --- | --- |
| §8.1.1 实物及服务销售单采购确认通过 | D14 |
| §8.1.2 二期卡券运营审批通过 | D14 + D27 |
| §8.1.3 销售/采购变更生效 | D14、D15 |
| §8.1.4 采购财务审核通过 | D15 |
| §8.1.5 `PREPAY` 采购履约门槛 | D15 + D16 |
| §8.2 库存与履约（入库过账、仓发过账等） | D16、D17 |
| §8.3 票款与发票 | D18、D19 |
| §8.4 商城与供应商 | D29、D30、D32、D33 |

---

## 4. 接口要求

1. 管理端接口一律在 `admin` 路由，JWT + RBAC 中间件，标注权限宏。
2. Handler 复用 service DTO；仅在 HTTP 形态差异（路径参数拆分、上下文注入）时
   允许最小包装并实现 `From/Into`，且注释说明原因。
3. 分页、排序、筛选参数按 P0 固化的形状；排序字段白名单在 Service 层校验。
4. 错误映射按 conventions §6.8，不新增顶层错误语义。
5. 响应中的金额与数量按 P0 约定序列化（字符串），时间按秒级时间戳。
6. 权限键：`resource` = 域内对象单数名，`action` 取固定动词表。
   新增权限键后必须重新生成并提交 `permissions.generated.ts`，CI 校验无漂移。
7. **接口形状以 `erp-client/features/<feature>/api.ts` 的现有类型为参照**。
   若后端契约与前端 mock 形态不一致，在 PR 的"契约变更"一节列出差异，
   由 P4 对应批次同步调整——不要默默改成任意形状。

---

## 5. 验收标准

### 5.1 命令

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
git diff --exit-code erp-client/lib/permissions.generated.ts   # 无漂移
```

本层**不要求** `cargo test -- --include-ignored`。HTTP 集成测试覆盖表见
[P6 §1.2](./P6-integration-tests.md)。

### 5.2 实现侧验收（非自动化 IT）

| 项 | 要求 |
| --- | --- |
| 接口清单 | 方法、路径、权限键、对应页面章节齐全 |
| 事务不变量 | PR 列出本域覆盖的第 8 章条目号，并说明实现落点（service 方法名） |
| 契约形状 | DTO 字段与前端 `api.ts` 类型可对照；差异记入「契约变更」 |
| 权限 | 权限宏齐全，生成物无漂移 |

### 5.3 PR 证据

conventions §7.3 模板（集成测试一栏写「延期至 P6 / I-Gx」）+ 以下两项：

- 本域接口清单：方法、路径、权限键、对应页面（`ui-workspaces/wNN.md` 章节号）
- 本域覆盖的第 8 章不变量条目号与实现方法名（自动化测试函数名在 P6 补齐）

---

## 6. 子阶段清单

| 阶段 ID | 批次 | 域 | 依赖（P2 单元） | 分支 |
| --- | --- | --- | --- | --- |
| C-G1 | 平台与单据基础设施 | D01–D06 | B-G1 | `feat/erp-c-g1-platform` |
| C-G2 | 业务伙伴 | D07–D09 | B-G1、B-G2 | `feat/erp-c-g2-party` |
| C-G3 | 商品与仓库 | D10、D11 | B-G1、B-G3 | `feat/erp-c-g3-catalog` |
| C-G4 | 合同与销售 | D12–D14 | B-G1–B-G4、B-G7 | `feat/erp-c-g4-sales` |
| C-G5 | 采购与供应商供给 | D15、D24 | B-G2–B-G5、B-G7 | `feat/erp-c-g5-procurement` |
| C-G6 | 履约与库存 | D16、D17 | B-G3–B-G7 | `feat/erp-c-g6-fulfillment` |
| C-G7 | 财务往来与成本 | D18–D21 | B-G4–B-G7 | `feat/erp-c-g7-finance` |
| C-G8 | 一期导入与商城同步 | D22、D23 | B-G1、B-G2、B-G4、B-G8 | `feat/erp-c-g8-mall-sync` |
| C-G9 | 二期供给、发布与投影 | D25–D27 | B-G3、B-G4、B-G5、B-G9 | `feat/erp-c-g9-publication` |
| C-G10 | 二期商城消费与售后 | D28–D31 | B-G7、B-G9、B-G10 | `feat/erp-c-g10-mall` |
| C-G11 | 二期供应商执行与结算 | D32、D33 | B-G5、B-G7、B-G9–B-G11 | `feat/erp-c-g11-supplier-exec` |
| C-G12 | 集成治理 | D34 | B-G1、B-G10、B-G11、B-G12 | `feat/erp-c-g12-integration-ops` |

依赖列是**跨域 Repository 依赖**，不是 Service 依赖——被依赖批次只需完成到 P2。

---

## 7. 二期专属约束

- 二期 P0/P1 能力（商品发布、销售投影、支付回流、供应商下单/拒单/取消/退款/
  余额恢复、人工异常与对账）**必须同批具备生产能力**；未整批闭环前不得开放
  `T` 后自动履约（数据模型 §10 末条）。C-G9 至 C-G12 因此按一个交付批次评审，
  可并行开发，但不单独上线。
- 外部 HTTP 调用统一设置超时、重试上限与错误分类；失败降级为可观测错误并
  记录 `account`/`request_id` 上下文（`AGENTS.md` 外部依赖容错）。
- 禁止实现 outbox、消息中间件或投递状态机；集成表是普通表组（数据模型 §5.4）。
