# P2 仓储层（database）

| 项 | 值 |
| --- | --- |
| 分支 | `feat/erp-b-<批次>-<域简称>` |
| 并行度 | 12（域间零依赖，可再细分） |
| 依赖 | 同域 P1 已合并 |
| `must_compile` | true |
| owns | `database/src/repository/<domain>.rs`、`database/src/repository/extensions/<domain>.rs`、`database/src/indexes/<domain>.rs` |

本层把实体落到 MongoDB，并把查询细节全部关在仓储内。**本层不做事务编排、不做业务判定。**

> **集成测试策略**：真实 MongoDB 上的仓储集成测试**不在本层强制验收**，统一在
> [P6-integration-tests.md](./P6-integration-tests.md) 收口，以免拖慢并行实现节奏。
> P0 的 D01 样板仍保留可复制的 IT 写法；本层可自愿补测，但不作为合并门禁。

---

## 1. 交付内容（每个域）

1. **仓储**：每个集合一个 Repository，方法签名以 `executor: &mut dyn Executor` 结尾。
   复用 `database::Repository<'_, T>` 基类提供的 CRUD 与乐观锁；域特有查询写成具名方法。
2. **访问器**：在 `extensions/<domain>.rs` 定义 `pub trait <Domain>Ext`，
   为 `Database` 实现，返回各集合 Repository。集合名用本域常量。
3. **索引**：在 `indexes/<domain>.rs` 的 `ensure()` 中声明数据模型第 6 章列出的**全部必需索引**。

---

## 2. 硬性要求

### 2.1 执行器与事务

- Repository **不得**调用 `with_transaction`，不得自行 `start_session`。
- 多步骤方法（先删后写、读后写、批量替换）在文档注释中写明
  「必须收到事务执行器」。
- 参考 `backend/database/TRANSACTIONS.md` 与 `repository/role.rs`
  的 `replace_subject_roles` 写法。

### 2.2 索引与唯一约束

- 唯一约束**必须**由唯一索引保证，不能只靠应用层校验（`AGENTS.md` 数据治理）。
- 索引统一命名：唯一 `uk_<collection>_<字段串>`，普通 `idx_<collection>_<字段串>`。
- 软删除语义下的唯一性：参考现有 `accounts` 的处理（身份类字段全局唯一，
  避免软删除后复用破坏恢复语义），域内若采用部分唯一索引，必须在注释中写明理由与回滚方式。
- TTL：需要过期清理的数据（如失败导入诊断保留 30 天、导出文件保留 7 天，
  数据模型 §4.5.7）用 TTL 索引落地。

### 2.3 查询

- 列表查询必须使用投影，禁止返回整文档。
- 分页统一 `database::Pagination` / `PageResult`；排序字段白名单化，禁止透传任意字段名。
- 批量查询用 `$in` 一次取回，禁止 N+1。
- 正则过滤复用 `repository/regex_filter.rs`，禁止各域自拼正则（注入与性能风险）。

### 2.4 数值持久化

金额/数量按 P0 固化的 `Decimal128` 形态写入；仓储层不做任何舍入或换算。

---

## 3. 验收标准

### 3.1 命令

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

本层**不要求** `cargo test -- --include-ignored`。仓储集成测试覆盖表见
[P6 §1.1](./P6-integration-tests.md)。

### 3.2 PR 证据

conventions §7.3 模板（集成测试一栏写「延期至 P6 / I-Gx」）+ 以下两项：

- 本域索引清单与数据模型 §6.x「必需索引」的逐条对照表（含"文档要求 / 已实现 / 索引名"）
- 未实现的索引及原因（应为空）

---

## 4. 子阶段清单

| 阶段 ID | 批次 | 域 | 依赖 | 分支 |
| --- | --- | --- | --- | --- |
| B-G1 | 平台与单据基础设施 | D01–D06 | A-G1 | `feat/erp-b-g1-platform` |
| B-G2 | 业务伙伴 | D07–D09 | A-G2 | `feat/erp-b-g2-party` |
| B-G3 | 商品与仓库 | D10、D11 | A-G3 | `feat/erp-b-g3-catalog` |
| B-G4 | 合同与销售 | D12–D14 | A-G4 | `feat/erp-b-g4-sales` |
| B-G5 | 采购与供应商供给 | D15、D24 | A-G5 | `feat/erp-b-g5-procurement` |
| B-G6 | 履约与库存 | D16、D17 | A-G6 | `feat/erp-b-g6-fulfillment` |
| B-G7 | 财务往来与成本 | D18–D21 | A-G7 | `feat/erp-b-g7-finance` |
| B-G8 | 一期导入与商城同步 | D22、D23 | A-G8 | `feat/erp-b-g8-mall-sync` |
| B-G9 | 二期供给、发布与投影 | D25–D27 | A-G9 | `feat/erp-b-g9-publication` |
| B-G10 | 二期商城消费与售后 | D28–D31 | A-G10 | `feat/erp-b-g10-mall` |
| B-G11 | 二期供应商执行与结算 | D32、D33 | A-G11 | `feat/erp-b-g11-supplier-exec` |
| B-G12 | 集成治理 | D34 | A-G12 | `feat/erp-b-g12-integration-ops` |

---

## 5. 重点域提示

- **D17 库存**：`stock_balance` 与 `stock_reservation` 是高并发热点。仓储需提供
  条件更新（可用量足够才扣减）的原子方法，把"不产生负库存"做成写条件而非读后判断。
  余额与预占的一致性维护属于 P3 事务，但**原子写入口**必须在本层提供。
- **D18/D19 往来**：核销进度字段的更新必须是条件写（不超额核销），同样在本层提供原子入口。
- **D02 单据注册**：`business_document` 是跨域注册表，被所有域写入。
  本层必须提供幂等注册（同 `document_type + document_no` 重复注册不产生第二条）。
- **D23/D29 事实类**：`business_fact_key` 去重必须靠唯一索引，不靠应用层查重。
