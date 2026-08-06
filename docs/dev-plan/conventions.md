# 跨阶段统一契约

本文是所有子阶段共享的实现约束。`backend/AGENTS.md` 与 `erp-client/AGENTS.md` 仍然全量生效，
本文只补充分阶段并行开发特有的约定，冲突时以 `AGENTS.md` 为准。

---

## 1. 所有权模型

一个子阶段只能修改自己 `owns` 列表内的文件。`owns` 按域展开为固定模式（`<domain>` 见
[domains.md](./domains.md)）：

| 层 | owns |
| --- | --- |
| P1 实体 | `backend/entities/src/<domain>/**` |
| P2 仓储 | `backend/database/src/repository/<domain>.rs`（或同名目录）、`backend/database/src/repository/extensions/<domain>.rs`、`backend/database/src/indexes/<domain>.rs` |
| P3 服务与接口 | `backend/services/src/<domain>/**`、`backend/apps/web-api/src/core/handler/<domain>/**`、`backend/apps/web-api/src/core/routes/<domain>.rs` |
| P4 前端 | `erp-client/features/<feature>/**`、该批次页面路由目录 |
| P6 后端集成测试 | `backend/database/tests/<domain>_repository.rs`、`backend/apps/web-api/tests/<domain>_api.rs`；跨域为 `web-api/tests/invariants/**`、`concurrency/**` 等（见 P6） |

新增文件也必须落在上述前缀内。需要放在别处的文件，说明该内容不属于本子阶段。

---

## 2. 共享文件冻结清单

以下文件**只在 P0 修改**，此后对所有子阶段只读：

```
backend/entities/src/lib.rs
backend/entities/src/ids.rs
backend/entities/src/money.rs
backend/entities/src/common/**
backend/database/src/lib.rs
backend/database/src/repository/mod.rs
backend/database/src/repository/base.rs
backend/database/src/repository/extensions/mod.rs
backend/database/src/indexes/mod.rs
backend/database/src/executor.rs
backend/database/src/transaction.rs
backend/database/src/mongo_ops.rs
backend/services/src/lib.rs
backend/services/src/errors.rs
backend/services/src/page.rs
backend/services/src/query.rs
backend/apps/web-api/src/main.rs
backend/apps/web-api/src/app_state.rs
backend/apps/web-api/src/core/routes/mod.rs
backend/apps/web-api/src/core/routes/admin.rs
backend/apps/web-api/src/core/handler/mod.rs
backend/apps/web-api/src/core/response.rs
backend/apps/web-api/src/core/errors.rs
backend/apps/web-api/build.rs
erp-client/lib/api/**
erp-client/lib/query-client.ts
```

若某个子阶段确实需要修改冻结文件，**不要直接改**：在 PR 描述中提出，由一次独立的
"地基修订 PR"（分支 `chore/erp-p0-amend-<主题>`）单独完成并合并，其他 worktree 随后 rebase。
一次地基修订只做一件事。

---

## 3. 实体层约定

1. 每个实体带 `BaseModel`（`#[serde(flatten)]`），派生
   `#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]`。
2. **ID 一律使用 `entities::ids` 中 P0 预定义的 newtype**，禁止在域内自定义 ID 类型，
   禁止用裸 `String` 作为跨表引用。
3. 金额、单价、数量、税率一律使用 `entities::money` 中的定点类型（见第 5 节），
   禁止 `f64`、禁止 `String`。
4. 三类公共字段基元由 P0 提供，域内按对象性质选用（数据模型 4.3）：
   - `StableBase`：稳定基础资料与可编辑草稿（`status`、`current_revision_id`、`lock_version`、
     `created_by`、`updated_by`）
   - `RevisionBase`：不可变修订（`revision_no`）
   - `FactBase`：正式事实（`fact_no`、`occurred_at`、`recorded_at`、`recorded_by`、
     `source_type`、`source_reference`、`reason_code`、`reason_text`）
5. 状态是固定业务代码，一律实现为 `enum` + 显式邻接矩阵函数
   `fn can_transition_to(&self, next: Self) -> bool`，禁止运行时可配置流转（数据模型 4.6、13.3）。
6. 构造与更新在 `new()` / `update()` 内完成全部校验与规范化；不变式不得外泄到 Service。
7. 单个业务方法 ≤ 30 行有效代码；公共方法必须有多行文档注释（参数、返回值、错误）。

---

## 4. 仓储层约定

1. 每个 Repository 方法签名以 `executor: &mut dyn Executor` 结尾，Repository 不开启事务。
2. 多步骤方法（先删后写、读后写）必须在文档注释中声明"必须收到事务执行器"。
3. 集合名常量定义在本域 `indexes/<domain>.rs`，Repository 与索引共用同一常量，禁止字面量重复。
   **冻结结构约束（P0-5 实证）**：`indexes/` 与 `repository/` 是冻结文件内部的私有子树，
   模块路径无法互相引用。P0-5 采用的既定模式：集合名常量作为
   `repository/extensions/<domain>.rs` 的 **`SourceRegistryExt` 关联常量**
   （单一权威来源，indexes 与 Repository 两侧共用）；筛选/投影行类型用
   `<Database as <Domain>Ext>::...` 关联类型对外暴露。后续 33 个域照抄该模式。
4. 索引在本域 `indexes/<domain>.rs` 的 `pub(crate) async fn ensure(db: &Database) -> Result<()>`
   内声明。数据模型第 6 章列出的"必需索引"必须逐条落地，唯一约束一律用唯一索引表达。
5. `DatabaseExt` 访问器写在本域 `repository/extensions/<domain>.rs` 的
   `pub trait <Domain>Ext`，P0 已在 `extensions/mod.rs` 把它并入 `DatabaseExt` supertrait。
6. 查询必须使用投影，禁止在列表接口返回整文档；分页统一用 `database::Pagination` / `PageResult`。

---

## 5. 数值与时间

由 P0 在 `entities/src/money.rs` 与 `entities/src/common/time.rs` 固化，全域唯一实现：

| 语义 | 类型 | 小数位 | 舍入 |
| --- | --- | --- | --- |
| 金额（含税/不含税/税额） | `Amount` | 2 | 银行家舍入到分，规则由 P0 固定并测试 |
| 单价 | `UnitPrice` | 4 | 行金额计算后舍入到分 |
| 数量 | `Quantity` | 6 | SKU 基础单位 |
| 税率、配赠率 | `Rate` | 6 | 显式数值，禁止百分号字符串 |
| 卡张数 | `u32` | — | 非负整数 |
| 业务日期 | `BusinessDate` | — | 自然日 |
| 业务时间/记录时间 | `Instant` | — | 统一时基持久化，展示层转业务时区 |

金额计算铁律（数据模型 4.2，P5 逐条测试）：

1. 逐行分别计算并舍入 `gross_amount` / `net_amount` / `tax_amount`；
2. 表头合计只汇总**已舍入**的行金额；
3. 发票尾差写 `rounding_adjustment_amount` 与原因，不反改销售/采购单价；
4. 收付款、合同、应收、应付用含税金额；利润指标用不含税金额；
5. 进项税率与销项税率分别保存。

BSON 持久化形态由 P0 固定（`Decimal128`）并提供序列化测试；域内不得自行选择存储形态。

---

## 6. 服务与接口层约定

HTTP 传输契约（信封/错误码/分页/时间与数值/权限生成物）见 [api-contract.md](./api-contract.md)。

1. **事务边界只在 Service**，统一走 `database::Transactional::with_transaction`。
   单集合无原子性要求的 CRUD 传 `&mut NoTransaction`；跨集合写入必须事务。
2. 跨域协作调用对方域的 **Repository**，禁止 `services::a::XService` 依赖 `services::b::YService`
   （README 3.2 第 3 条）。跨域可复用的判定逻辑下沉到 `entities`。
3. 涉及权限或审计的写入使用已有模板：`run_authorized_audited_policy_transaction`（RBAC 相关）
   或 `run_audited_transaction`（其他跨审计集合写入），见 `backend/database/TRANSACTIONS.md`。
4. DTO 定义在 `services/src/<domain>/dto.rs`，Handler 直接复用；
   禁止在 handler 内重复定义同构 Request/Response。
5. Handler 只做协议适配：`Validate` → Service 调用 → `ApiResponse`。禁止直连数据库。
6. 管理端接口一律挂 `admin` 路由，走 JWT + RBAC，并标注
   `#[permission_macros::permission(...)]`。权限键命名 `resource` 用域内对象单数名，
   `action` 取 `list|detail|create|update|delete|submit|approve|reject|post|export` 等固定动词。
7. **`AppState` 不随域增长**：Service 在 handler 内按需从 `state.db()` 构造。
   任何"把新 Service 塞进 AppState"的改动都属于冻结文件修改，需走地基修订 PR。
8. 错误映射稳定：`OutcomeUnknown → 500`（提示"操作结果暂无法确认"）、
   `TransientTransactionConflict → 409`、`OptimisticLockingError → 409`、
   校验失败 → `400`、权限失败 → `403`。域内不得新增顶层错误语义。
9. 权限生成物（`build.rs` 产出）必须随 PR 提交且 CI 校验无漂移。

---

## 7. 测试与验收证据

### 7.1 分层测试形态

| 层 | 位置 | 依赖 | 何时强制 | 必须覆盖 |
| --- | --- | --- | --- | --- |
| P1 | `entities/src/<domain>/**` 内联 `mod tests` | 无 | P1 合并门禁 | 构造/更新校验、边界、状态邻接、金额舍入 |
| P2 | 实现 only；IT 文件属 P6 | — | P2 **不强制** IT | 索引清单与编译门禁；IT 见 P6 §1.1 |
| P3 | 实现 only；IT 文件属 P6 | — | P3 **不强制** IT | 接口清单、不变量实现落点；IT 见 P6 §1.2 |
| P4 | `erp-client` 类型检查 + 联调记录 | 真实后端 | P4 合并门禁 | 列表/详情/写操作/错误提示/权限隐藏 |
| P5 | 投影实现 + 治理脚本 | 相关 C | P5 合并门禁 | E3 可运行投影、E4 脚本化治理；跨域/并发 IT 见 P6 |
| P6 | `database/tests/*`、`web-api/tests/*` | 对应 C（及 E3） | **发布前置** | 域内仓储/HTTP IT + 跨域不变量 + 并发/故障 + 投影 IT |

**策略**：P2/P3 优先并行交付实现；需真实 MongoDB 的**域级**集成测试集中在 **P6 从零编写**，
避免前期拖慢进度与实现/测试漂移。`database/tests/`、`web-api/tests/` 在 P6 前仅 README 占位。
P0 提供 `test-support` 与 `dev-mongo.sh` 夹具；禁止在 P2/P3 PR 中提交域级 IT。

### 7.2 集成测试执行

P0 提供 `backend/scripts/dev-mongo.sh` 启动单节点副本集（事务需要副本集，standalone 不支持）。
需要数据库的测试统一用 P0 提供的 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控宏，
使无数据库环境的 `cargo test --workspace` 仍然全绿。

- **P0–P5 日常门禁**：`cargo test --workspace`（不含 ignored）。
- **P6 与发布 / CI 集成段**：`cargo test --workspace -- --include-ignored`。

每个测试用独立随机数据库名，测试结束 drop，禁止共享固定库名。

### 7.3 PR 必附证据

```markdown
## 验收证据
- 范围：<阶段 ID> / <域列表>
- 契约来源：erp-data-model.md §6.x、§7.x、§8.x；erp-phase-N.md §x.x；ui-workspaces/wNN.md
- 门禁：fmt / check / clippy / test 全绿（贴命令与结论）
- 集成测试：<命令 + 用例清单 | 或「延期至 P6 / I-Gx」>
- 覆盖的不变量：<数据模型第 8 章条目号>
- 未实现且已知的缺口：<列表，或"无">
```

"未实现且已知的缺口"为空是可以的，但**不允许留空不写**。
P2/P3 PR 的「集成测试」栏必须写明延期至哪个 I-Gx，不得省略该行。

---

## 8. 前端集成约定

1. 网络请求一律经 TanStack Query；`features/<domain>/api.ts` 是唯一请求函数落点，
   `queries.ts` 是唯一对外消费入口（`erp-client/AGENTS.md` 第 2 节）。
2. P4 的动作是**替换 `api.ts` 的实现**，保持其导出签名与类型不变；
   页面组件与 `queries.ts` 原则上不改。签名确需变化时，在 PR 中单列"契约变更"。
3. 每个 feature 用 P0 提供的开关（`erp-client/lib/api/feature-source.ts`）在 mock 与真实
   实现之间切换，使未集成的页面继续在 mock 上可用。**该 feature 集成完成时删除其 mock 文件与开关分支。**
4. 错误统一映射到 `ApiError`（`kind`/`message`/`status`），提示集中在 `useErrorHandler`。
5. 用户可见文案必须过 `docs/ui-glossary.md`。
6. 金额、数量在前端不做二次运算取整；后端返回已舍入值，前端只负责展示格式化。

---

## 9. 提交与合并

- 一个子阶段一个分支一个 PR；PR 标题 `[<阶段ID>] <批次名>`。
- 合并前必须 rebase 到最新 `main` 并重跑全部门禁。
- 跨层合并顺序：下层先合。同层任意顺序。
- 出现共享文件冲突即说明有人越界，**回退越界改动**，不要在 PR 里手工解冲突。
