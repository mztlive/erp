# P1 领域模型层（entities）

| 项 | 值 |
| --- | --- |
| 分支 | `feat/erp-a-<批次>-<域简称>` |
| 并行度 | 12（G1–G12 全并行，可再按域细分） |
| 依赖 | P0 完成 |
| `must_compile` | true |
| owns | `backend/entities/src/<domain>/**` |

本层把 `docs/erp-data-model.md` 第 6/7 章的规格翻译成 Rust 类型与不变式。
**本层不写任何数据库代码、不写任何服务编排。**

---

## 1. 交付内容（每个域）

1. `entities/src/<domain>/mod.rs`：模块声明与 re-export，不堆业务代码。
2. 每张表一个实体文件（或聚合根 + 行项一个文件），含：
   - 结构体（`BaseModel` flatten + P0 公共字段基元）
   - `new()` / `update()`：完整校验与规范化
   - 状态 `enum` + `DocumentState` 实现（有状态机的对象）
   - 值对象：把重复的规范化/判定逻辑封装成类型（`AGENTS.md`「类型内聚与下沉」）
   - `Data` 结构：创建/更新入参，不含系统字段
3. 内联 `#[cfg(test)] mod tests`。

---

## 2. 逐项对照要求

实施者必须按下表逐条核对，PR 中列出核对结论：

| 契约来源 | 必须落地的内容 |
| --- | --- |
| 数据模型 §6.x 字段字典 | 域内每张表的每个领域关键字段；字段名与文档完全一致，禁止同义分叉（§13.8） |
| 数据模型 §4.2 | 金额/单价/数量/税率一律用 P0 的定点类型；逐行舍入 |
| 数据模型 §4.3 | 稳定资料/修订/事实分别使用 `StableBase`/`RevisionBase`/`FactBase` |
| 数据模型 §4.4 | 稳定主表 + 不可变修订表的拆分；正式版本保存结构化快照字段 |
| 数据模型 §4.5 | 正式事实不设业务软删除；草稿可逻辑删除；敏感字段标注加密/HMAC 需求 |
| 数据模型 §7.x | 状态 `enum` 与邻接矩阵；禁止运行时扩展 |
| 数据模型 §8.x | 单个实体内可判定的不变式在此实现；跨聚合部分留给 P3 并在注释中标注条目号 |
| `erp-phase-N.md` | 业务规则（如责任规则、时序约束）中属于实体不变式的部分 |

### 2.1 敏感字段处理

`party_bank_account`、`party_contact` 的手机号、履约地址等：实体层定义
**加密值 + 带密钥 HMAC 查询指纹**两个字段（数据模型 §4.5.5），提供
`fn fingerprint(plain: &str, key: &Key) -> String`。禁止裸摘要，禁止在 `Display`/
`Debug` 中输出明文（自定义 `Debug` 实现并测试）。

ERP 不保存卡号、卡密、卡实例绑定手机号（§4.5.6）——D28 的实体不得出现这些字段。

### 2.2 快照字段

正式版本（`*_revision`）必须内联当时的客户名称、合同编号、结算主体、税务、付款条件、
商品名称、规格、单位、供应商名称等**结构化**快照字段，不是 JSON blob（§4.4）。
这些字段由 P3 在形成版本时填充，P1 只负责定义并校验。

---

## 3. 跨域约束

- 引用其他域的对象**只用 `entities::ids` 中的 ID newtype**，不 `use` 对方域的实体类型。
- 需要对方域的判定逻辑时，说明该逻辑应下沉为共享值对象：
  放 `entities/src/common/`，并在 PR 中标为"地基修订"（走 `chore/erp-p0-amend-*` 流程）。
- 因此 G1–G12 的 A 单元之间**零依赖，可完全并行**。

---

## 4. 验收标准

### 4.1 命令

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p entities
cargo test --workspace
```

### 4.2 测试覆盖要求

每个实体至少覆盖：

1. **happy path**：合法输入构造成功，字段被正确规范化（trim、截断、去重）。
2. **失败路径**：必填为空、超长、列表越界、关联不一致各一条。
3. **状态机**：合法迁移通过；非法迁移被拒；邻接矩阵闭包完整（用 P0 的
   `assert_adjacency_closed`）。
4. **金额**：涉及金额的实体，逐行三元组一致性（gross = net + tax，按分舍入）。
5. **敏感字段**：`Debug`/`Display` 不泄漏明文；指纹稳定且带密钥。

### 4.3 PR 证据

按 conventions §7.3 模板填写，其中"契约来源"必须精确到数据模型的小节号，
"覆盖的不变量"列出本域在第 8 章中已实现的条目与留给 P3 的条目。

---

## 5. 子阶段清单

| 阶段 ID | 批次 | 域 | 分支 |
| --- | --- | --- | --- |
| A-G1 | 平台与单据基础设施 | D01–D06 | `feat/erp-a-g1-platform` |
| A-G2 | 业务伙伴 | D07–D09 | `feat/erp-a-g2-party` |
| A-G3 | 商品与仓库 | D10、D11 | `feat/erp-a-g3-catalog` |
| A-G4 | 合同与销售 | D12–D14 | `feat/erp-a-g4-sales` |
| A-G5 | 采购与供应商供给 | D15、D24 | `feat/erp-a-g5-procurement` |
| A-G6 | 履约与库存 | D16、D17 | `feat/erp-a-g6-fulfillment` |
| A-G7 | 财务往来与成本 | D18–D21 | `feat/erp-a-g7-finance` |
| A-G8 | 一期导入与商城同步 | D22、D23 | `feat/erp-a-g8-mall-sync` |
| A-G9 | 二期供给、发布与投影 | D25–D27 | `feat/erp-a-g9-publication` |
| A-G10 | 二期商城消费与售后 | D28–D31 | `feat/erp-a-g10-mall` |
| A-G11 | 二期供应商执行与结算 | D32、D33 | `feat/erp-a-g11-supplier-exec` |
| A-G12 | 集成治理 | D34 | `feat/erp-a-g12-integration-ops` |

工作量最大的是 A-G1（6 个域）、A-G4 与 A-G7（各 3–4 个域且规则密集）。
若并行资源充足，优先把这三个批次按域拆成独立 worktree。

---

## 6. 常见偏差（评审重点）

| 偏差 | 处理 |
| --- | --- |
| 用 `String` 存 ID 或金额 | 打回；必须用 P0 newtype |
| 校验逻辑写在 Service 而非实体 | 打回；`AGENTS.md`「类型内聚与下沉」 |
| 状态用 `String` + 运行时判断 | 打回；数据模型 §13.3 |
| 用 JSON 字段承载金额/状态/外键/核销关系 | 打回；数据模型 §4.4 |
| 新增数据模型中不存在的字段或表 | 打回；README §5.1 |
| 方法超过 30 行有效代码 | 打回；`AGENTS.md` 方法长度约定 |
| 公共方法缺多行文档注释 | 打回；`AGENTS.md` 注释约定 |
