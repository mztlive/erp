# P6 后端集成测试（收口）

| 项 | 值 |
| --- | --- |
| 分支 | `feat/erp-i-<批次或主题>` |
| 并行度 | 12（域内 IT）+ 跨域/并发可串行或小并行 |
| 依赖 | 对应域的 P3（C 单元）已合并；跨域/并发子集依赖相关 C 与 E3 |
| `must_compile` | true |
| owns | 见各子阶段（主要为 `database/tests/**`、`web-api/tests/**`） |

## 0. 为什么单独成层

P2/P3 的目标是**尽快交付可编译、可联调的实现**。真实 MongoDB 副本集上的
仓储/HTTP 集成测试编写成本高、夹具重、易拖慢并行开发节奏；若在实现期同步维护 IT，
还会与后续改接口/仓储**持续漂移**。

因此：

- **P2/P3 验收不要求** `--include-ignored` 集成测试（见 [P2](./P2-repository.md)、[P3](./P3-service-api.md)）。
- **仓库现状**：`database/tests/` 与 `web-api/tests/` 已清空（仅 README 占位），
  历史域级 IT 不保留。本阶段**从零编写**，以合并时的实现与契约为准，禁止复用旧测试副本。
- **P0** 保留 `test-support`、`dev-mongo.sh`（夹具与门控，非业务用例）。
  基础设施烟雾测试可留在 `test-support/tests`；业务域用例全部在本阶段新建。
- **P6 是最后阶段**：统一补齐域内仓储 IT、域内 HTTP IT、跨域不变量 IT、并发与故障恢复 IT，
  以及投影相关的可执行测试证据。未完成 P6，**不得作为生产模型发布**（数据模型 §13）。

P4 前端联调与 P5 的投影实现/治理脚本可在 P6 之前并行推进；P6 发现的缺陷回修走
对应域的 fix 分支，不在本阶段扩大业务范围。

---

## 1. 交付范围

### 1.1 域内仓储集成测试（原 P2 强制项）

路径：`backend/database/tests/<domain>_repository.rs`

| 用例 | 断言 |
| --- | --- |
| 创建 + 按 ID 读取 | 往返一致，含 Decimal128 金额、时间字段 |
| 更新乐观锁成功 | version 递增，`updated_at` 更新 |
| 更新乐观锁冲突 | 用陈旧 version 更新返回 `OptimisticLockingError` |
| 软删除与恢复 | 仅匹配对应删除状态；正式事实类集合**不提供**软删除方法 |
| 唯一索引冲突 | 重复身份写入返回明确错误，不是静默覆盖 |
| 索引存在 | `assert_indexes` 断言 `ensure()` 后全部必需索引就位 |
| 事务参与 | 同一 session 内两次写入，回滚后两者都不可见 |
| 列表查询 | 分页边界、排序白名单、投影字段集合正确 |
| 多步骤方法 | 事务内先删后写，冲突时整体回滚 |

### 1.2 域内 HTTP 集成测试（原 P3 强制项）

路径：`backend/apps/web-api/tests/<domain>_api.rs`

| 用例 | 断言 |
| --- | --- |
| 未带 token | 401 |
| 带 token 但无权限 | 403 |
| 请求体校验失败 | 400 + 稳定错误结构 |
| happy path | 200 + 契约形状（字段名与前端 `api.ts` 类型一致） |
| 乐观锁/并发冲突 | 409 |
| 事务不变量 | 关键写入路径：断言多集合结果同时生效；注入失败后断言全部不可见 |
| 幂等 | 资金/状态类入口重复提交只产生一条正式事实 |
| 分页与排序 | 边界页、非法排序字段被拒 |

#### 1.2.1 D03 审批运行时强制用例

路径：`backend/apps/web-api/tests/approval_runtime_api.rs`、
`backend/database/tests/approval_runtime_repository.rs`

| 场景 | 断言 |
| --- | --- |
| `DIRECT` 激活 | 解析唯一有效用户并直接创建“我的待办”；不存在开始处理动作 |
| `DIRECT` 解析失败 | 步骤和实例进入 `BLOCKED`；不回退公共池、不创建开放待办 |
| `POOL` 开始处理并发 | 两名合格用户并发时恰好一人取得责任；同一用户重试幂等 |
| 任务版本 | 队列及嵌入投影返回任务自身版本；陈旧 `expected_task_version` 返回 409 和最新安全摘要 |
| 首次时间保留 | 退回、再次开始处理和转交不覆盖 `assigned_at/started_at`，只更新当前责任时间 |
| 责任资格变化 | 提交时重新校验角色、数据范围、对象参与权和岗位分离；失效后拒绝 |
| 串行多级通过 | 当前待办完成、当前步骤通过、下一步骤激活和下一待办创建处于同一事务 |
| 最终通过 | 审批决定、领域正式事实、实例终态和任务完成同时生效；注入失败后全部回滚 |
| 驳回与重提 | 驳回不激活下一步骤；新业务版本重提创建新实例并保留原历史 |
| 退回与转交 | 只更新原 `OPEN` 任务责任并写审计；不创建同义后继任务 |
| 非法通用完成 | 不存在公共 `complete_work_item` 路由；客户端不能提交下一步骤或完成动作代码 |
| 运行时端口 | 业务 Handler 只依赖 `ApprovalRuntimePort`，同一 HTTP 契约不泄漏 `INTERNAL/BPM` 差异 |
| 阻塞恢复 | 原因未消除时保持 `BLOCKED`；消除后只恢复原步骤并创建或校正唯一待办，不跳步、不形成决定 |
| 阻塞待办投影 | 阻塞前已有开放待办时保留任务身份和责任，队列返回受阻状态、空动作且不计入可立即处理数 |
| 管理和历史范围 | `managed` 可查授权范围内已分派下属任务；`history` 只读终态；越权及非法状态组合被拒绝 |

每个事务场景必须同时具备成功例、中途失败回滚例和幂等重试例。测试夹具不得继续构造
`UNCLAIMED / IN_PROGRESS`、领取令牌、租约或客户端责任状态。

### 1.3 跨域事务不变量（原 P5 E1）

路径：`backend/apps/web-api/tests/invariants/**`

把数据模型第 8 章的每一条不变量写成跨域集成测试（每条正例 + 中途失败回滚例）。
必须覆盖 §8.1.1–§8.1.5、§8.2 入库/仓发等、§8.3 票款与发票、§8.4 商城与供应商，
以及第 9 章关键业务断言。每条不变量一个具名测试函数。

### 1.4 并发与故障恢复（原 P5 E2）

路径：`backend/apps/web-api/tests/concurrency/**`

数据模型 §13.6：每个正式过账入口必须具备并发、重复提交、超额核销、负库存和故障恢复测试。

| 场景 | 断言 |
| --- | --- |
| 并发过账 | 同一单据两个请求并发，恰好一个成功，另一个 409；无双重事实 |
| 重复提交 | 相同幂等键重复提交只产生一条正式事实 |
| 超额核销 | 核销金额超过开放余额被拒，余额不变 |
| 负库存 | 并发出库超过可用量被拒，`stock_balance` 不为负 |
| 预占竞争 | 并发预占同一批可用量，总预占不超过可用量 |
| 提交结果未知 | 注入 `UnknownTransactionCommitResult`，断言返回 `OutcomeUnknown` 且不自动重放 |
| 故障恢复 | 事务中途中断后重启，无半写状态；待办与状态可自愈或有明确人工入口 |

过账入口清单：销售确认、采购审核、入库、发货、电子交付、服务确认、收款、付款、
开票、退款、库存调整、商城消费、供应商结算确认——逐个有测试。

### 1.5 投影相关测试（配合 P5 E3）

在 E3 投影实现已合并的前提下，补齐：

- 投影重建幂等性（重建两次结果一致）
- 新鲜度延迟上界
- 重建失败不污染正式事实

---

## 2. 硬性约定

1. 需要数据库的测试统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（conventions §7.2）。
2. 每个测试用独立随机库名，结束 drop，禁止共享固定库名。
3. 复用 P0 的 `test-support`（`TestDb`、`require_mongo!`、种子账号、HTTP 客户端、`assert_indexes`）。
4. **本阶段默认不改业务实现**；若测试暴露缺陷，另开 fix 分支修实现，再回到本阶段补绿。
5. 不得在本阶段扩大业务范围、新增接口或改数据模型语义。

---

## 3. 子阶段清单

### 3.1 域内 IT（按 G 批次并行）

| 阶段 ID | 批次 | 覆盖 | 依赖 | 分支 |
| --- | --- | --- | --- | --- |
| I-G1 | 平台与单据基础设施 | D01–D06 的 repository + HTTP IT | C-G1 | `feat/erp-i-g1-platform` |
| I-G2 | 业务伙伴 | D07–D09 | C-G2 | `feat/erp-i-g2-party` |
| I-G3 | 商品与仓库 | D10、D11 | C-G3 | `feat/erp-i-g3-catalog` |
| I-G4 | 合同与销售 | D12–D14 | C-G4 | `feat/erp-i-g4-sales` |
| I-G5 | 采购与供应商供给 | D15、D24 | C-G5 | `feat/erp-i-g5-procurement` |
| I-G6 | 履约与库存 | D16、D17 | C-G6 | `feat/erp-i-g6-fulfillment` |
| I-G7 | 财务往来与成本 | D18–D21 | C-G7 | `feat/erp-i-g7-finance` |
| I-G8 | 一期导入与商城同步 | D22、D23 | C-G8 | `feat/erp-i-g8-mall-sync` |
| I-G9 | 二期供给、发布与投影 | D25–D27 | C-G9 | `feat/erp-i-g9-publication` |
| I-G10 | 二期商城消费与售后 | D28–D31 | C-G10 | `feat/erp-i-g10-mall` |
| I-G11 | 二期供应商执行与结算 | D32、D33 | C-G11 | `feat/erp-i-g11-supplier-exec` |
| I-G12 | 集成治理 | D34 | C-G12 | `feat/erp-i-g12-integration-ops` |

每个 I-Gx 的 `owns`：

- `backend/database/tests/<domain>_repository.rs`（该批次全部 domain）
- `backend/apps/web-api/tests/<domain>_api.rs`（该批次全部 domain）

### 3.2 跨域与系统级 IT

| 阶段 ID | 主题 | 依赖 | 分支 | owns |
| --- | --- | --- | --- | --- |
| I-X1 | 跨域事务不变量 | I-G4、I-G5、I-G6、I-G7 | `feat/erp-i-x1-invariants` | `web-api/tests/invariants/**` |
| I-X2 | 并发与故障恢复 | I-G6、I-G7 | `feat/erp-i-x2-concurrency` | `web-api/tests/concurrency/**` |
| I-X3 | 投影集成测试 | E3、I-G1、I-G7、I-G10 | `feat/erp-i-x3-projection` | 投影相关 `tests/**` |

---

## 4. 验收标准

### 4.1 域内 I-Gx

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
ERP_TEST_MONGO_URI=mongodb://127.0.0.1:27017/?replicaSet=rs0 \
  cargo test -p database --test <domain>_repository -- --include-ignored
ERP_TEST_MONGO_URI=... \
  cargo test -p web-api --test <domain>_api -- --include-ignored
```

PR 证据：conventions §7.3 + 本批次 domain 列表 + 用例清单（对照 §1.1 / §1.2）。

### 4.2 跨域 I-X*

```bash
ERP_TEST_MONGO_URI=... cargo test -p web-api -- --include-ignored
```

PR 证据：

- I-X1：「章节条目 ↔ 测试函数名」对照表，第 8/9 章无遗漏
- I-X2：过账入口清单与对应测试函数名
- I-X3：投影幂等/新鲜度/失败隔离的测试函数名

### 4.3 P6 完成判定（发布前置）

- [ ] 全部 I-G1…I-G12 合并，`include-ignored` 域内用例全绿
- [ ] I-X1、I-X2、I-X3 合并并通过
- [ ] D03 审批运行时强制用例全部通过，覆盖直接指派、责任池并发、串行多级审批和强类型完成
- [ ] CI 两段式：`cargo test --workspace` 与 `cargo test --workspace -- --include-ignored` 均绿
- [ ] 数据模型 §13 要求的可执行证据齐备

---

## 5. 常见偏差

| 偏差 | 处理 |
| --- | --- |
| 在 I-Gx 中改业务逻辑「顺便修」 | 允许最小 fix，但须在 PR 中单列「缺陷修复」；大改回对应 C/B 分支 |
| 跳过 ignore 门控、让无库 `cargo test` 失败 | 打回 |
| 用共享固定库名跑测试 | 打回 |
| 把 P6 提前到 P3 同期强制 | 不建议；若个人愿先写 IT 可作为该 I-Gx 提前合并，但不阻塞 C 单元 |
| 仅写 happy path 无 401/403/409 | 打回；覆盖表见 §1.2 |
| 沿用旧领取/租约夹具或只测单级审批 | 打回；覆盖表见 §1.2.1 |
