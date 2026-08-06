# P0 地基、防冲突改造与垂直样板

| 项 | 值 |
| --- | --- |
| 分支 | `feat/erp-p0-foundation` |
| 并行度 | 1（串行，独占） |
| 依赖 | 无 |
| `must_compile` | true |
| 阻塞 | 全部 P1–P5 子阶段 |

P0 不交付业务价值。它交付两样东西：**（1）让 100+ 个后续子阶段互不冲突的文件结构；
（2）让它们照抄的参考实现**。P0 没做完之前开任何 worktree，都会在合并期付出更高代价。

---

## 1. 任务 P0-1：共享基元

### 1.1 ID 类型（`backend/entities/src/ids.rs`）

**一次性定义全部 34 个域的 ID newtype**，不留待各域自建。这是实体层能完全并行的前提：
`sales_order` 需要引用 `CustomerId`、`SkuId`、`ContractId`，若这些类型分散在各域，
G4 就必须等 G2、G3。

- 用宏批量生成 newtype：`Deref<Target = str>`、`AsRef<str>`、`From<String>`、`Display`、
  `Serialize`/`Deserialize`（透明为字符串）、`PartialEq`/`Eq`/`Hash`/`Clone`。
- 生成规则：稳定主表 → `<Entity>Id`；修订表 → `<Entity>RevisionId`；行表 → `<Entity>LineId`。
- 值由 `id_generator::next_id()` 产生（UUID v4，32 位十六进制）；ID 不承载业务含义（数据模型 4.1）。

**验收**：`entities/src/ids.rs` 覆盖 `domains.md` 全部表的主键类型；单测覆盖序列化透明性。

### 1.2 业务编号（`backend/crates/id-generator` 扩展）

`*_no` 是可展示业务编号，**一经形成正式事实不得复用**（数据模型 4.1）。当前
`id-generator` 只有 UUID，缺编号能力。

- 定义 `DocumentNumberKind`（销售单、采购单、入库单、发货单、收款单、付款单、发票…），
  每种一个前缀。
- 编号必须持久化连续性（MongoDB 原子 `findAndModify` 计数器集合），
  提供 `next_number(kind, date, executor) -> Result<String>`，可加入调用方事务。
- 逻辑删除的草稿不进入编号连续性（数据模型 4.5 第 2 条）。

**验收**：并发 1000 次取号无重复、无跳号断言；取号参与事务回滚后号段行为在文档中写明并测试。

### 1.3 定点数值（`backend/entities/src/money.rs`）

当前 workspace **没有任何定点小数依赖**，而数据模型禁止浮点金额。P0 必须选型并固化。

- 引入 `rust_decimal`（或等价库），workspace 统一版本。
- 定义 `Amount`(2)、`UnitPrice`(4)、`Quantity`(6)、`Rate`(6) 四个 newtype，
  各自封装小数位与舍入，禁止外部直接构造裸 Decimal。
- 提供且只提供一处舍入实现：`round_to_cent`。
- BSON 形态固定为 `Decimal128`，提供 `Serialize`/`Deserialize` 与往返测试。
- 提供行金额三元组计算：`line_amounts(unit_price, quantity, tax_rate) -> (gross, net, tax)`，
  满足 conventions 第 5 节的四条铁律。

**验收**：舍入边界用例（.005、负数、大额）、含税/不含税/税额一致性断言、Decimal128 往返测试。

### 1.4 公共字段基元（`backend/entities/src/common/`）

现有 `entity-core::BaseModel` 只有 `id`/`version`/`created_at`/`updated_at`/`deleted_at`，
与数据模型 4.3 要求的公共字段有明确缺口：缺 `created_by`/`updated_by`/`status`/
`current_revision_id`，以及事实类对象的 `occurred_at`/`recorded_at`/`recorded_by`/
`source_type`/`source_reference`/`reason_code`。

P0 必须做出并写明决策：**保留 `BaseModel` 作为持久化元数据，另加三个组合基元**
（不改 `BaseModel`，避免破坏现有 IAM/审计代码）：

```
common/stable.rs    StableBase   { status, current_revision_id, created_by, updated_by }
common/revision.rs  RevisionBase { revision_no }
common/fact.rs      FactBase     { fact_no, occurred_at, recorded_at, recorded_by,
                                   source_type, source_reference, reason_code, reason_text }
common/time.rs      BusinessDate / Instant
common/source.rs    SourceType 枚举（ERP、商城同步、历史回填、供应商回调、人工导入）
```

`BaseModel.version` 即数据模型的 `lock_version`，P0 在文档中写明这一对应关系，此后不再改名。

**验收**：三个基元各有单测；`common/README.md` 写明"何时用哪个基元"的判定表。

### 1.5 固定状态机基元（`backend/entities/src/common/state.rs`）

数据模型第 7 章定义了 8 组固定状态机，第 13 章要求"状态邻接矩阵必须固化，
禁止运行时动态扩展"。

- 提供 `trait DocumentState: Sized + Copy + Eq` 与 `fn allowed_next(self) -> &'static [Self]`。
- 提供 `ensure_transition(from, to) -> Result<()>`，失败返回统一错误码。
- 各域在自己模块内实现该 trait；P0 只提供 trait 与测试辅助（`assert_adjacency_closed`）。

**验收**：trait + 一份样板实现（用 D01 的状态）+ 邻接闭包测试辅助可用。

---

## 2. 任务 P0-2：防冲突结构改造

对照 `README.md` 第 1.2 节的七个共享文件，逐个改造为"聚合器 + 每域一文件"，
并**一次性预声明全部 34 个域**（生成空实现）。

### 2.1 `entities`

```
entities/src/lib.rs         ← 预声明 pub mod <domain>;  × 34（P0 后冻结）
entities/src/<domain>/mod.rs ← 空模块 + 文件头注释标明所属域与契约章节
```

### 2.2 `database`

```
database/src/repository/mod.rs           ← 预声明 mod <domain>;  × 34（冻结）
database/src/repository/<domain>.rs      ← 空
database/src/repository/extensions/mod.rs ← pub trait DatabaseExt: <D01>Ext + … + <D34>Ext {}
                                            impl<T: …> DatabaseExt for T {}（冻结）
database/src/repository/extensions/<domain>.rs ← pub trait <Domain>Ext { }（空，含 blanket impl for Database）
database/src/indexes/mod.rs              ← ensure_indexes 顺序调用 34 个域 ensure()（冻结）
database/src/indexes/<domain>.rs         ← pub(crate) async fn ensure(db) -> Result<()> { Ok(()) }
```

`DatabaseExt` 改为 supertrait 聚合后，各域只在自己的 `extensions/<domain>.rs` 里加访问器方法，
`extensions/mod.rs` 永不再改。现有 `accounts()`/`audit_logs()`/`roles()` 迁入
`extensions/access_control.rs`，**调用点签名保持不变**。

### 2.3 `services`

```
services/src/lib.rs        ← 预声明 pub mod <domain>;  × 34（冻结）
services/src/<domain>/mod.rs、dto.rs ← 空
```

现有 `iam`、`auth`、`audit` 模块保持原位不动，不纳入 34 域改造。

### 2.4 `web-api`

```
core/handler/mod.rs        ← 预声明 pub mod <domain>;  × 34（冻结）
core/handler/<domain>/mod.rs ← 空
core/routes/mod.rs         ← 预声明 mod <domain>;（冻结）
core/routes/<domain>.rs    ← pub fn routes(rbac: &SharedRbacService) -> Router<AppState> { Router::new() }
core/routes/admin.rs       ← 一次性 .merge(<domain>::routes(&rbac_service)) × 34（冻结）
```

空 `Router::new()` 合并是合法的，`admin.rs` 从此不再改。

**验收（P0-2 整体）**：`cargo check --workspace` 与 `cargo test --workspace` 通过；
用 `git grep -c "pub mod"` 核对 34 个域在四处均已声明；现有 IAM/审计 HTTP 测试无回归。

---

## 3. 任务 P0-3：测试夹具

当前后端**没有任何 `tests/` 目录**。P2/P3 实现阶段**不强制**跑真实 Mongo IT；
但 P0 必须先把夹具与样板 IT 建好，供最后阶段 [P6](./P6-integration-tests.md) 批量复制。

1. `backend/scripts/dev-mongo.sh`：启动单节点副本集容器（`--replSet rs0` + 自动 `rs.initiate()`），
   输出连接串；`docker-compose.yml` 增加对应 profile。
2. `backend/crates/test-support`（新 crate，dev-dependency）：
   - `TestDb`：按随机库名连接、创建、`Drop` 时清理；
   - `require_mongo!()` 宏：读 `ERP_TEST_MONGO_URI`，缺失时跳过并打印原因；
   - `seed_*` 辅助：最小账号/角色/权限种子，供 HTTP 集成测试鉴权；
   - `assert_indexes(db, collection, &[names])`：索引存在性断言；
   - HTTP 测试客户端：启动 `web-api` Router，带 JWT 发请求。
3. `#[ignore]` 门控约定与 CI 两段式执行（见 conventions 7.2）：
   - 日常/P1–P5：`cargo test --workspace`；
   - P0 样板 + P6 / 发布：`cargo test --workspace -- --include-ignored`。

**验收**：`cargo test --workspace` 在无数据库环境全绿；
`ERP_TEST_MONGO_URI=... cargo test --workspace -- --include-ignored` 在有库环境全绿
（至少覆盖 D01 样板 IT 与既有 IAM 相关测试）。

---

## 4. 任务 P0-4：API 与前端接入基座

### 4.1 后端契约固化

- `ApiResponse` 保持现状，P0 只补文档：成功/失败信封、错误码枚举、
  分页响应形状（`items` + `total` + `page` + `page_size`）。
- 列表查询参数统一：`page`、`page_size`、`sort_by`、`sort_dir`、域内筛选字段扁平传递。
- 时间统一以秒级 Unix 时间戳传输，金额与数量以**字符串**传输（避免 JS 浮点失真），
  前端用 `lib/fixed-decimal.ts` 消费。此项必须与 `erp-client` 现有 mock 形态核对后确定并写进文档。
- **权限产物落点修正**：`apps/web-api/build.rs` 输出到
  `erp-client/lib/permissions.generated.ts`，并在 CI 校验无漂移。

### 4.2 前端接入基座（`erp-client/lib/api/`）

`erp-client` 目前**没有任何 HTTP 客户端**，全部走 mock。P0 建立：

```
lib/api/client.ts         fetch 封装：base URL、JWT 头、超时、统一解包 ApiResponse
lib/api/errors.ts         ApiError { kind, message, status, responseData }（AGENTS 第 10 节）
lib/api/paging.ts         分页/排序参数与响应类型
lib/api/session.ts        登录态、token 存取、401 处理
lib/api/feature-source.ts 按 feature 切换 mock / 真实实现的开关
```

**验收**：用 D01 样板域的接口，在 `erp-client` 里完成一次真实 `useQuery` 取数并渲染；
其余 27 个 feature 仍走 mock 且无回归（`npm run lint`、`npx tsc --noEmit`）。

---

## 5. 任务 P0-5：垂直样板（域 D01 `source_registry`）

选 D01 的理由：表少（3 张）、无跨域依赖、被几乎所有域引用、覆盖"稳定主表 + 映射表"两种形态。

打穿全部四层，产出**可被复制的骨架**：

| 层 | 产物 | 作为样板演示的点 |
| --- | --- | --- |
| 实体 | `entities/src/source_registry/` | ids 用法、StableBase、状态机 trait、内联单测 |
| 仓储 | `repository/source_registry.rs`、`indexes/source_registry.rs`、`extensions/source_registry.rs` | Executor 传参、投影查询、唯一索引、乐观锁；**含**样板仓储 IT 写法供 P6 复制 |
| 服务 | `services/src/source_registry/{mod.rs,dto.rs}` | 事务模板、审计事务、DTO 定义、错误映射 |
| 接口 | `handler/source_registry/`、`routes/source_registry.rs` | 权限宏、路由挂载、`ApiResponse`；**含**样板 HTTP IT 写法供 P6 复制 |
| 前端 | `erp-client/features/mall-sync` 中来源相关取数 | `api.ts` 替换姿势、开关用法、错误提示 |

**验收**：外部身份映射的建立/查询接口可鉴权调用；D01 样板 IT 覆盖 happy path、403、400、409（乐观锁）
（后续域的同类 IT 在 P6 批量补齐，不阻塞 P2/P3）；前端能取到真实数据。

---

## 6. 任务 P0-6：文档与 CI

1. 在 `backend/AGENTS.md` 追加一节"分阶段并行开发约束"，指向本目录，并写明冻结文件清单。
2. CI 两段：默认 `cargo test --workspace`；集成段（Mongo service）
   `cargo test --workspace -- --include-ignored`（P0 样板 + 后续 P6 全量）。
   另含权限生成物漂移校验、`erp-client` 的 `npm run lint` 与 `tsc --noEmit`。
3. 在 `docs/dev-plan/_meta.json` 校验分支名与 owns 前缀无重叠（可用一次性脚本）。

---

## 7. P0 完成判定

全部满足才可以开第一个并行 worktree：

- [ ] `entities/src/ids.rs` 覆盖全部域主键类型
- [ ] 业务编号生成器可用且有并发测试
- [ ] `Amount`/`UnitPrice`/`Quantity`/`Rate` 与唯一舍入实现落地，Decimal128 往返测试通过
- [ ] `StableBase`/`RevisionBase`/`FactBase`/`SourceType`/状态机 trait 落地
- [ ] 四处共享文件已改造为聚合器 + 34 域预声明，且全部冻结
- [ ] `test-support` crate 与 `dev-mongo.sh` 可用，两段式测试全绿
- [ ] `erp-client/lib/api/` 基座可用，样板域前端取数成功
- [ ] D01 端到端跑通，四层各有可复制骨架
- [ ] 权限生成物落到 `erp-client` 且 CI 校验无漂移
- [ ] `cargo fmt --all -- --check`、`cargo check --workspace`、
      `cargo clippy --workspace --all-targets --all-features -- -D warnings`、
      `cargo test --workspace` 全绿
