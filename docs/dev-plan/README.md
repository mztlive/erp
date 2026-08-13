# ERP 后端与集成分阶段开发指导

## 0. 本目录的效力

本目录是**实施合同**，不是建议。实施者按阶段文档落地自己的 `owns` 文件集，不扩大业务范围，
不发明业务文档以外的表、状态和接口。业务语义以下列只读文档为唯一来源：

| 只读契约 | 用途 |
| --- | --- |
| `docs/erp-phase-1.md` | 一期业务范围、流程、职责、规则索引 |
| `docs/erp-phase-2.md` | 二期范围、切换、集成、验收场景 |
| `docs/approval-workflow-contract.md` | 审批运行、步骤分派、多级推进、待办责任与 BPM 接入边界 |
| `docs/erp-data-model.md` | 表、字段、索引、状态机、事务不变量、物理设计强制要求 |
| `docs/erp-mall-data-mapping.md` | 旧商城数据防腐映射 |
| `docs/ui-workspaces/w*.md`、`docs/ui-glossary.md` | 页面契约与用户可见术语 |
| `backend/AGENTS.md`、`erp-client/AGENTS.md` | 编码规范与分层约束 |

阶段文档只回答"谁在什么时候、在哪些文件里、按什么标准交付什么"。它不重复业务语义。

| 顺序 | 文档 | 用途 |
| --- | --- | --- |
| 1 | 本文 | 策略、阶段矩阵、并行与调度、门禁 |
| 2 | [conventions.md](./conventions.md) | 跨阶段统一契约：注册约定、事务、错误、测试、验收证据 |
| 3 | [domains.md](./domains.md) | 域切分矩阵 D01–D34：表 ↔ 依赖 ↔ 期 ↔ 页面 ↔ 落点 |
| 4 | [P0-foundation.md](./P0-foundation.md) | **串行前置**：地基、防冲突改造、垂直样板 |
| 5 | [P1-entities.md](./P1-entities.md) … [P6-integration-tests.md](./P6-integration-tests.md) | 各层阶段说明与验收 |
| 6 | [`_meta.json`](./_meta.json) | 机器可读的阶段/子阶段/分支/依赖/验收命令 |

---

## 1. 采用的切分方式与理由

采纳**按层切分为大阶段、按域切分为可并行子阶段**的方案，即：

```
                 G1 平台   G2 伙伴   G3 商品   …   G12 集成治理     ← 子阶段（域批次，可并行）
P1 领域模型        A-G1     A-G2     A-G3          A-G12
P2 仓储            B-G1     B-G2     B-G3          B-G12   ← 只交付实现，不强制 Mongo IT
P3 服务与接口      C-G1     C-G2     C-G3          C-G12   ← 只交付实现，不强制 HTTP IT
P4 前端集成            F1 … F10（按页面批次切，依赖对应 C 单元）
P5 加固                    投影实现（E3）、治理脚本（E4）
P6 后端集成测试        I-G1…I-G12 域内 IT + I-X1/X2/X3 跨域/并发/投影 IT（最后收口）
```

每个格子（层 × 域批次）= 一个 worktree = 一个 PR = 一次独立验收。

### 1.1 为什么按层，而不是按业务域端到端

本仓库的两个端已经被钉死，因此按层切分的常见风险（下层设计不匹配上层用例）在这里基本不存在：

- **下端已钉死**：`docs/erp-data-model.md` 已给出全部表、字段、索引、状态机和事务不变量。
  领域模型层不需要"边做边发现"，它是把已写好的规格翻译成 Rust 类型。
- **上端已钉死**：`erp-client` 已有 28 个 feature、约 30 个工作台页面在 mock 上跑通，
  `features/*/api.ts` 的入参出参就是 API 形态的既成事实。

而按层切分带来的好处在这里非常实在：

- **验收标准逐层同质**：一层内所有子阶段用同一套命令、同一套证据格式验收，评审成本低。
- **并行度高且冲突面小**：同一层内不同域改的是不同文件，天然无冲突。
- **返工半径小**：一个域的实体改错，只影响该域后续层，不会把已完成的端到端链路整条推翻。

### 1.2 我对原方案的三处修改

**（一）新增 P0 串行地基阶段，且它是唯一允许改共享文件的阶段。**

上一版计划（已删除的 `docs/dev-plan/S00–S11`）的失败点是：中间阶段 `must_compile = false`，
所有注册动作攒到最后一个 S11 阶段统一应用。这等于"每个阶段都不可验收，直到最后一起验收"，
与你要的"每阶段独立验收"直接冲突。

根因不是分阶段方式，而是**共享注册文件**：

| 文件 | 现状 | 每加一个域是否必须改 |
| --- | --- | --- |
| `backend/entities/src/lib.rs` | 平铺 `pub mod` | 是 |
| `backend/database/src/repository/mod.rs` | 平铺 `mod` | 是 |
| `backend/database/src/repository/extensions.rs` | 单个 `DatabaseExt` trait + 单个 impl | 是 |
| `backend/database/src/indexes.rs` | 单函数 `ensure_indexes` 顺序调用 | 是 |
| `backend/services/src/lib.rs` | 平铺 `pub mod` | 是 |
| `backend/apps/web-api/src/core/routes/admin.rs` | 单函数逐个 `.merge` | 是 |
| `backend/apps/web-api/src/core/handler/mod.rs` | 平铺 `pub mod` | 是 |

P0 的核心工作是把这七处**一次性预声明全部 34 个域**（生成空模块 / 空 router / 空索引函数），
此后任何域的实施者只写自己的文件，永不触碰共享文件。共享文件冲突归零之后，
`must_compile = true` 与 `cargo test --workspace` 就可以对**每一个**子阶段生效。

**（二）P3 把 handler/routes 与 service 合并为同一阶段，而不是单列一层。**

`backend/AGENTS.md` 已把 Handler 限定为协议适配（权限宏 + DTO 复用 + `ApiResponse`），
它薄到不值得单独一层；更重要的是，只有 HTTP 可调通才是前端能对接的验收物。
因此 P3 的验收标准直接定为"接口可鉴权调用并返回契约形状"，前端阶段才有确定的对接对象。

**（三）P0 内包含一条垂直样板（tracer），先打穿一个窄域的全部四层。**

12 个批次 × 4 层同时开工前，必须先有一份可抄的参考实现，否则每个 worktree 会各自发明
测试夹具、错误映射、分页形状和事务写法，合并时形成规范漂移。P0 选 **D01 来源与外部身份**
（表少、无跨域依赖、被所有域引用）走完 entities → repository → service → handler → 前端调用，
产出的不是业务价值，而是**后续 100+ 个子阶段直接复制的骨架**。

**（四）真实 Mongo 集成测试从 P2/P3 剥离，统一到最后阶段 P6 从零收口。**

原方案在每个 B/C 子阶段强制 `include-ignored` 仓储/HTTP IT，编写夹具与排障成本高，
显著拖慢并行实现，且易与实现漂移。调整后：

- P2/P3 只要求编译与单元门禁 + 索引/接口/不变量**实现**证据；
- **仓库不保留**历史域级 IT：`database/tests/`、`web-api/tests/` 仅 README 占位；
- P0 只落地 `test-support` 与 `dev-mongo.sh` 夹具（非业务用例）；
- **P6** 按当前实现与契约**从零**编写全部 repository/HTTP IT，再补跨域不变量、并发与投影 IT；
- 未通过 P6 **不得**作为生产模型发布（数据模型 §13）。

---

## 2. 阶段总表

| 阶段 | 名称 | 并行度 | `must_compile` | 独立验收物 | 文档 |
| --- | --- | --- | --- | --- | --- |
| **P0** | 地基、防冲突改造与垂直样板 | 1（串行） | true | 全部 34 域模块骨架就位；样板域端到端跑通 | [P0-foundation.md](./P0-foundation.md) |
| **P1** | 领域模型（entities） | 12 | true | `cargo test -p entities` 覆盖该域全部不变式 | [P1-entities.md](./P1-entities.md) |
| **P2** | 仓储（database） | 12 | true | 仓储实现 + 索引清单；**不强制** Mongo IT | [P2-repository.md](./P2-repository.md) |
| **P3** | 服务与接口（services + web-api） | 12 | true | 可对接 HTTP 实现 + 权限产物；**不强制** HTTP IT | [P3-service-api.md](./P3-service-api.md) |
| **P4** | 前端集成（erp-client） | 10 | — | 该页面批次 mock 引用归零，对真后端联调通过 | [P4-frontend.md](./P4-frontend.md) |
| **P5** | 跨域加固（投影 + 治理） | 2 | true | E3 投影实现、E4 治理脚本接入 CI | [P5-hardening.md](./P5-hardening.md) |
| **P6** | 后端集成测试（收口） | 12+3 | true | 域内 IT + 跨域/并发/投影 IT；`include-ignored` 全绿 | [P6-integration-tests.md](./P6-integration-tests.md) |

**没有 `must_compile = false` 的阶段。** 任何子阶段的 PR 若不能通过**该阶段适用**的门禁，即为未完成。

---

## 3. 并行与调度

### 3.1 层是验收门，不是调度门

`P1 → P2 → P3` 的先后关系**按域批次逐格生效**，不是整层齐步走。
`B-G2`（伙伴域仓储）只要 `A-G2`（伙伴域实体）已合并即可开工，无需等待 `A-G12`。

因此推荐**流水线调度**而非分层波次调度：

| 时段 | 同时在跑的 worktree（示例，n=6） |
| --- | --- |
| T0 | P0（独占，其余人做业务文档精读与用例清单） |
| T1 | A-G1、A-G2、A-G3、A-G4、A-G5、A-G6 |
| T2 | B-G1、B-G2、B-G3 + A-G7、A-G8、A-G9 |
| T3 | C-G1、C-G2 + B-G4、B-G5、B-G6 + A-G10、A-G11 |
| T4 | F1、F2 + C-G3、C-G4、C-G5 + B-G7… |
| … | 依 `_meta.json` 的 `depends_on` 滚动 |

### 3.2 依赖规则（三条，必须遵守）

1. **实体层无跨域依赖。** 域之间只通过 ID 引用，全部 ID newtype 由 P0 在 `entities/src/ids.rs`
   统一预定义。快照字段按数据模型 4.4 结构化复制，不持有对方实体类型。因此 12 个 A 单元完全并行。
2. **仓储层无跨域依赖。** 每个 Repository 只负责自己的集合。因此 12 个 B 单元完全并行。
3. **服务层跨域协作只允许"调用对方域的 Repository"，禁止 service → service 依赖。**
   事务边界在 Service，Repository 已在 P2 稳定，跨域写入（如销售审批通过写应收）由发起方
   Service 在自己的事务闭包里调用对方域 Repository 完成。可复用的业务规则按 `AGENTS.md`
   的下沉原则放进 `entities`，不放进对方 Service。
   这条规则是 P3 能并行的唯一原因，**违反它会把 12 个并行单元退化成一条串行链**。

### 3.3 worktree 操作

```bash
# 每个子阶段一个 worktree，一个分支
git worktree add ../erp-A-G2 -b feat/erp-a-g2-party origin/main
cd ../erp-A-G2

# 完成后：先在 worktree 内跑完门禁，再合回
cargo fmt --all && cargo check --workspace \
  && cargo clippy --workspace --all-targets --all-features -- -D warnings \
  && cargo test --workspace
```

分支命名固定为 `feat/erp-<层字母小写>-<批次小写>-<域简称>`，见 `_meta.json` 的 `branch` 字段。

合并顺序：同层内任意；跨层必须先合下层。**任何时候都不允许两个 worktree 同时修改
`conventions.md` 第 2 节列出的共享文件。**

---

## 4. 全局门禁

每个子阶段 PR 必须附带以下证据，缺一项视为未完成：

| 门 | 命令 | 适用 |
| --- | --- | --- |
| 格式 | `cargo fmt --all -- --check` | 后端全部阶段 |
| 编译 | `cargo check --workspace` | 后端全部阶段 |
| Lint | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 后端全部阶段 |
| 测试 | `cargo test --workspace` | 后端全部阶段（不含 ignored） |
| 集成测试 | `ERP_TEST_MONGO_URI=... cargo test --workspace -- --include-ignored` | **P0 样板 + P6 收口**（发布前置）；P2–P5 不强制 |
| 权限产物 | 生成文件无漂移（见 conventions 第 6 节） | P3 |
| 前端 | `npm run lint` + `npx tsc --noEmit` | P4 |
| mock 归零 | 该批次 feature 内 `@/mock` 引用计数为 0 | P4 |

---

## 5. 范围边界（禁止事项）

1. 禁止新增 `docs/erp-data-model.md` 未定义的表、集合、字段、状态或状态迁移。
2. 禁止建设 `docs/erp-phase-1.md` 第 5.2 节"第一期不建设"清单内的能力。
3. 禁止用 JSON 半结构化字段承载金额、状态、主外键、核销关系或库存关系（数据模型 4.4）。
4. 禁止在 P0 之外的任何阶段修改共享注册文件。
5. 禁止在 P3 之外的阶段新增 HTTP 路由或权限键。
6. 二期 P0/P1 能力（商品发布、销售投影、支付回流、供应商下单/拒单/取消/退款/余额恢复、
   人工异常与对账）未整批闭环前，不得开放 `T` 后自动履约（数据模型第 10 章末条）。
