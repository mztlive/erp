# ERP 分阶段开发计划（p1p2）

## 1. 文档目的与阅读路径

本目录定义 **p1p2** 范围的并行 worktree 开发与最终集成顺序。实现者按阶段文档落地 `owns_modules`，不扩大业务范围。

| 顺序 | 文档 | 用途 |
| --- | --- | --- |
| 1 | 本文 `README.md` | 全局策略、阶段总表、并行波次、合并顺序 |
| 2 | [branch-map.md](./branch-map.md) | 开 worktree：分支 ↔ owns 路径 |
| 3 | `S00`…`S11` 阶段文档 | 单阶段目标、落点、验收、PATCHNOTES 义务 |
| 4 | 各阶段 `Sxx-PATCHNOTES.md`（实现时产出） | 汇合注册清单，仅 S11 应用 |
| 5 | 业务契约（只读） | `docs/erp-phase-*.md`、`erp-data-model.md`、`ui-workspaces/*` |

机器可读摘要见 [`_meta.json`](./_meta.json)。

---

## 2. 全局编译策略

| 阶段类型 | `must_compile` | 规则 |
| --- | --- | --- |
| S00–S10 | `false` | 各 worktree **仅**实现自有 `owns_modules` 内 entities/repository/services/handler（S10 为 erp-client）。允许因缺少 `lib.rs`/`mod.rs`/routes/`DatabaseExt`/indexes 注册而无法通过 workspace 编译。 |
| S11 | `true` | **唯一**改共享汇合文件并强制 `cargo fmt/check/clippy/test --workspace` 的阶段。 |

**共享汇合文件（中间阶段禁止并行修改）**

- `backend/entities/src/lib.rs`
- `backend/services/src/lib.rs`
- `backend/database/src/repository/mod.rs`
- `backend/database/src/repository/extensions.rs`
- `backend/database/src/indexes.rs`
- `backend/apps/web-api/src/core/handler/mod.rs`、`handler/admin/mod.rs`
- `backend/apps/web-api/src/core/routes/**`、`app_state.rs`、`main.rs`
- `backend/apps/web-api/build.rs`、`fronts/admin/.../permissions.generated.ts`（S11）

中间阶段须在 `docs/dev-plan/Sxx-PATCHNOTES.md`（或 PR 描述）声明应追加的 `mod`/`pub use`/`DatabaseExt`/`ensure_indexes`/nest 路由与 permission 键；由 S11 集中应用。

`depends_on` DAG **约束合并顺序**，不阻止契约稳定后并行编码自有模块。

---

## 3. 全局风格契约

强制对齐：

- `backend/AGENTS.md`
- 仓库 rust-coding-standards / 阶段 JSON `style_contract`

要点（实现时 checklist）：

1. **分层固定**：HTTP Handler → Service → Repository → MongoDB。Handler 只做协议适配 / 权限宏 / `ApiResponse`，禁止直连 DB。
2. **开发顺序**：文件边界 → entities → repository（汇合阶段注册 DatabaseExt）→ services 按用例拆文件 → handler → routes/permission（汇合）。
3. **`mod.rs`**：仅声明 / 结构体 / 构造 / re-export；禁止主业务堆叠。
4. **DTO**：定义在 `services`，Handler 复用；禁止 handler 同构重复类型。
5. **事务**：Service 经 `Transactional::with_transaction`；多集合写必须事务；软删/更新走 `id+version` 乐观锁。
6. **实体**：含 `BaseModel`；查询名词、写动词；方法 ≤30 行有效代码；公共方法完整文档注释。
7. **admin**：JWT + RBAC + permission 宏；禁止建设文档「不建设」清单能力；禁止发明未写入 data-model 的单据或表。

---

## 4. 阶段总表

| id | 标题 | 分支 | 依赖 | must_compile | 文档 |
| --- | --- | --- | --- | --- | --- |
| S00 | 共享基元与单据基础设施 | `feat/erp-s00-document-infra` | — | false | [S00-document-infra.md](./S00-document-infra.md) |
| S01 | 基础资料 | `feat/erp-s01-master-data` | S00 | false | [S01-master-data.md](./S01-master-data.md) |
| S02 | 客户中心与合同 | `feat/erp-s02-customer-contract` | S01 | false | [S02-customer-contract.md](./S02-customer-contract.md) |
| S03 | 销售单变更与客户验收 | `feat/erp-s03-sales` | S02 | false | [S03-sales.md](./S03-sales.md) |
| S04 | 采购二次确认采购单与供应商商品库 | `feat/erp-s04-procurement` | S01, S03 | false | [S04-procurement.md](./S04-procurement.md) |
| S05 | 履约与库存 | `feat/erp-s05-fulfillment-inventory` | S04 | false | [S05-fulfillment-inventory.md](./S05-fulfillment-inventory.md) |
| S06 | 财务往来票款成本与经营分析 | `feat/erp-s06-finance` | S03, S04 | false | [S06-finance.md](./S06-finance.md) |
| S07 | 待办工作台 | `feat/erp-s07-workbench` | S00 | false | [S07-workbench.md](./S07-workbench.md) |
| S08 | 一期商城同步映射与期初导入 | `feat/erp-s08-mall-sync-p1` | S02, S03 | false | [S08-mall-sync-p1.md](./S08-mall-sync-p1.md) |
| S09 | 二期集成域 | `feat/erp-s09-integration-p2` | S01, S03, S04, S08 | false | [S09-integration-p2.md](./S09-integration-p2.md) |
| S10 | 前端 API 集成 | `feat/erp-s10-frontend-api` | S05–S09 | false | [S10-frontend-api.md](./S10-frontend-api.md) |
| S11 | 最终集成汇合 | `feat/erp-s11-integration-merge` | S10 | **true** | [S11-integration-merge.md](./S11-integration-merge.md) |

---

## 5. 建议 worktree 并行波次

| 波次 | 可并行阶段 | 前置条件 |
| --- | --- | --- |
| W0 | **S00** | 基线 IAM/auth/audit 已存在（不重建） |
| W1 | **S01**、**S07** | S00 契约/模块文件就绪（合并可后置，编码可并行） |
| W2 | **S02** | S01 主数据身份稳定 |
| W3 | **S03**、**S08** | S02 完成后；S08 另依赖 S03 销售聚合契约 |
| W4 | **S04** | S01 + S03 |
| W5 | **S05**、**S06** | S05 依赖 S04；S06 依赖 S03+S04，可与 S05 并行 |
| W6 | **S09** | S01、S03、S04、S08 就绪 |
| W7 | **S10** | S05–S09 后端契约稳定；独占 erp-client，与后端文件零重叠 |
| W8 | **S11** | S10 及全部 PATCHNOTES 齐备 |

说明：W09 履约在 `erp-ui-design`/`erp-ui-flows` 为一期核心面；`ui-workspaces` 索引缺独立 w09 文档，仍纳入 **S05**。既有 IAM/auth/audit 基线不在本计划重建，仅 S11 挂接扩展权限与 `data_scope` 时复用。

---

## 6. 建议 merge 顺序

严格按 `depends_on` 拓扑：

```text
S00 → S01 → S02 → S03 → S04 → S05
                ↘     ↘     ↘
                 S07   S08   S06
                        ↘
                         S09 → S10 → S11
```

线性建议合并序列（与 `merge_order` 一致）：

**S00 → S01 → S02 → S03 → S04 → S05 → S06 → S07 → S08 → S09 → S10 → S11**

（S07 可在 S00 后尽早合入；S06 与 S05 合并先后可互换，但均须在 S04 之后、S10 之前。）

---

## 7. 范围边界

| 允许 | 禁止 |
| --- | --- |
| 以 `erp-phase-1/2`、`erp-data-model`、`ui-workspaces` 已写能力为准 | 超出 phase / data-model / ui-workspaces 的能力 |
| 本目录阶段文档与 PATCHNOTES | 改写 `docs/erp-phase*.md` 或业务契约文档 |
| 各阶段 `owns_modules` 内实现 | 在中间阶段改共享汇合文件 |
| S11 汇合 + 质量门禁 | 在本计划内写业务实现代码进 `backend/`（文档集不写代码） |
| mock→api 替换（S10） | 发明未写业务单据/状态；可配置审批流；完整总账；消息中间件/outbox |

`scope=p1p2`：一期主路径 + 二期协同表组；卡券生产/玩法/CRM/询价等仍出界。

---

## 8. 与前端集成总策略（mock → api）

1. **契约冻结**：各域 `features/*/types.ts` 与 W 文档 §8 对齐；query key 形状（`all`/`list`/`detail`）保留。
2. **后端阶段**：S00–S09 交付 admin REST + DTO + `allowedActions`/`actionBlockers`/`lockVersion`/幂等语义；前端可继续 mock。
3. **S10**：独占 `erp-client/features/**` 与 `lib/`；将 `api.ts`（或 queries 内 fetch）换真实 HTTP；退役正式写路径对 `mock/*` / `session-state` 的依赖；对接 permission 生成物与 W19。
4. **S11**：workspace 路由联调、跨域 invalidate、后端汇合编译；不改页面路由表（W01–W30 缺 W24）。
5. **硬规则**：禁止前端按角色名推导 `allowedActions`；禁止本地重算正式余额/毛利；用户可见文案遵守 `ui-glossary.md`。

---

## 9. 相关索引

- 分支对照：[branch-map.md](./branch-map.md)
- 元数据：[_meta.json](./_meta.json)
- 业务契约：`docs/erp-phase-1.md`、`docs/erp-phase-2.md`、`docs/erp-data-model.md`、`docs/ui-workspaces/`
- 后端规范：`backend/AGENTS.md`
- 前端规范：`erp-client/AGENTS.md`

---

*scope: p1p2 · stages: S00–S11 · 唯一 must_compile: S11*
