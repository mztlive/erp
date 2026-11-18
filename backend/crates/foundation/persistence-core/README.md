# persistence-core：MongoDB 持久化基础设施

## 职责

本 crate 为业务仓储提供共用的 MongoDB 连接与事务能力检查、执行器与读取边界、通用 CRUD、分页、过滤、聚合和持久化错误分类。它提供跨仓储访问同一事务的机制，但不决定哪些步骤必须原子执行、怎样授权或何时重试业务命令。

业务实体、集合名、集合访问器、业务查询及索引定义由所属领域维护；HTTP 错误映射留在应用边界。命名普通索引、唯一索引和部分唯一索引的构造器只组装 MongoDB 索引模型；领域提供名称、有序键和适用的过滤条件，组合根调用领域的公开入口并保持既有逐集合登记顺序。

## 目录结构

| 入口 | 用途 |
| --- | --- |
| [`Cargo.toml`](Cargo.toml)、[`src/lib.rs`](src/lib.rs) | crate 依赖、`test-support` feature 与公开 API 入口。 |
| [`src/connection.rs`](src/connection.rs) | `connect` 创建客户端和数据库句柄；`ensure_transaction_support` 检查部署是否支持事务。 |
| [`src/executor.rs`](src/executor.rs)、[`src/executor/boundary.rs`](src/executor/boundary.rs) | `Executor` 统一会话接入，`ExecutionBoundary` 标识当前读取阶段。 |
| [`src/transaction.rs`](src/transaction.rs)、[`src/errors.rs`](src/errors.rs) | `Transactional::with_transaction` 管理事务生命周期与提交重试；错误模块保留驱动错误分类。 |
| [`src/repository/`](src/repository/mod.rs) | `Repository`、`PageResult`、`Pagination`、`QueryFilter`，以及 ID 查询和正则字面量过滤。 |
| [`src/mongo_ops.rs`](src/mongo_ops.rs)、[`src/aggregate.rs`](src/aggregate.rs)、[`src/aggregate/cursor.rs`](src/aggregate/cursor.rs) | 统一有会话与无会话的 MongoDB 读写、聚合和分批游标。 |
| [`src/indexes.rs`](src/indexes.rs) | 构造命名索引，不定义业务索引，也不执行登记。 |
| [`src/change_feed.rs`](src/change_feed.rs)、[`src/dirty_marks.rs`](src/dirty_marks.rs) | 已提交变更的刷新信号（不传业务文档），以及派生投影的待重算标记和按标记序号写回。 |
| [`src/rotation.rs`](src/rotation.rs) | 在调用方集合中读取轮转游标、在事务内按版本比较写入，并提供标量到数组的启动迁移；集合名、匹配与分派策略由领域决定。 |
| [`src/reset.rs`](src/reset.rs) | 显式全库重置：事务外准备非系统集合清单，事务内按领域指定的 BSON `_id` 保留文档，删除其余文档，保留集合及索引。 |
| [`src/testing.rs`](src/testing.rs) | 无数据库的执行合同测试替身。 |

## 架构设计

### 连接与部署检查

应用装配用 `connect` 创建共享客户端和数据库句柄。创建成功不代表数据库健康或支持事务；启动时须另用 `ensure_transaction_support` 发送 `hello`，检查逻辑会话及副本集或分片集群的事务能力，拒绝 standalone 和不符合要求的部署。

### 执行器与跨仓储事务

`Executor` 表示**调用方选定的数据访问上下文**：提供操作应加入的会话和可核验的读取边界，两者都可能不存在。执行器自身不开启或提交事务。业务仓储接收调用方的 `&mut dyn Executor`，通过 `mongo_ops`、`aggregate` 或通用 `Repository` 访问数据；这些封装负责将操作和游标接到同一会话。通用分页的取数和计数使用同一筛选条件和执行器，但只有在同一个快照事务内读取时才有一致快照保证。

本域 Service 或跨域 Process 在用例入口调用共享客户端的 `with_transaction`。该方法创建因果一致的会话，以 `snapshot` 读关注和 `majority` 写关注开启事务，再把受管执行器交给回调。参与事务的仓储使用该客户端的集合句柄；决定写入的授权与数据校验、Repository、Port 和业务步骤都复用这个执行器，下游不另开事务或独立提交。回调成功后，设施提交事务，提交成功才返回结果。回调失败则尽力中止事务；若中止也失败，只记日志并保留原错误。外部 HTTP、S3 调用和通知留在事务外；需要与业务写入一致时，在事务内写入 outbox。

例如，[供应商结算复核入口](../../orchestration/erp-supply-settlement/src/supply_settlement/review.rs) 开启事务，[过账步骤](../../orchestration/erp-supply-settlement/src/supply_settlement/review_posting.rs) 沿用同一个执行器校验授权与业务事实，并写入结算单、任务、适用的应付与成本记录、回执及审计。各领域仍负责自己的查询和写入规则。

### 提交重试与命令重试

- **只重试提交。** `with_transaction` 只在提交错误带 `UnknownTransactionCommitResult` 标签时，在同一会话上重试 `commit_transaction`。重试预算从首次提交开始计算，为 120 秒；预算耗尽且结果仍未知时返回 `Error::CommitOutcomeUnknown`，保留驱动错误。其他提交错误按原分类返回，不走这个重试分支。事务回调最多执行一次，提交结果未知不等于回滚。
- **是否重开事务由用例决定。** 驱动转换时优先识别重复键；其后将 `TransientTransactionError` 分类为 `Error::TransientTransactionConflict`，但本 crate 不自动重跑事务体，也不把乐观锁冲突当作瞬态错误。在明确允许整笔重试的用例中，由用例判断错误阶段、重试次数和安全条件，在新事务中重新授权、读取和校验。例如，[公开选品写入](../../orchestration/erp-order-to-cash/src/sales_selection/public_retry.rs) 仅重试进入回滚路径的事务体瞬态冲突，不重试初始化失败、提交失败或业务冲突。
- **提交结果未知时不自动重放写入。** 有回执机制的用例可在新快照中按原请求、原幂等键和当前授权只读查证；无法确认时保留未知状态。调用方重试有幂等键的命令时须沿用原幂等键，不能另造命令。

## 读取边界

| 场景 | 执行器及保证 |
| --- | --- |
| 独立自动提交操作，不需要授权读取凭证 | `NoTransaction`：不附加会话，也不签发读取边界。 |
| 不要求一致快照，但需要交付前复验的授权读取 | `ReadExecutor::default()`：没有 MongoDB 会话，不保证多次查询看到同一版本；授权查询生成、仓储取数与交付复验须使用同一实例、同一读取阶段，由授权层按合同重读相关依赖。 |
| 需要一致快照的查询，或决定原子写入的事务内读取 | 使用 `with_transaction` 回调收到的受管执行器，在同一事务、同一读取阶段完成授权、取数和交付复验或写前校验。 |

只读快照不是另一种执行器，也不是只能读的事务 API。ReadModel 或本域查询用例使用同一个 `with_transaction`，约束回调只读。快照保证该事务内的读取一致，不代表提交后事实仍未变化，也不替代授权凭证的边界及有效期检查。若用例要求在交付前检查最新状态，应在新快照中重新读取并复验，不带入原快照的凭证。例如，采购列表在[首个快照](../../read-models/erp-read-models/src/purchase_center/scope.rs)中读取授权、列表与版本，[交付前](../../read-models/erp-read-models/src/purchase_center/query.rs)再以独立快照重新取得范围和业务版本指纹并比较。

`ExecutionBoundary` 在受管事务内标识该事务的当前读取阶段，在 `ReadExecutor` 中只标识该实例的当前阶段。它持有阶段的弱引用，复制凭证不能延长阶段寿命；退出执行器作用域、切换到下一事务或调用 `Executor::next_read()` 后，原阶段凭证失效。`next_read()` 只推进阶段，不重开事务、不刷新快照。统一写操作在尝试写入前推进阶段，即使写入失败也不会恢复旧凭证；未尝试写入的空批插入不推进阶段，只读操作也不推进。

`Executor` 由本 crate 封闭实现，业务不能自行实现并重新挂载旧边界。原始 `ClientSession` 可传给数据访问封装，但没有受管读取边界；在受管事务执行器上调用 `Executor::session()` 直接取得原始会话，会永久撤销该执行器的边界证明，不能据此跳过授权依赖复验。业务仓储应使用统一读写封装，不直接取得原始事务会话。

`test-support` 仅允许在 `dev-dependencies` 中启用。`testing::SnapshotExecutor` 用于无数据库的执行合同测试，只模拟边界和阶段变化，不提供真实 MongoDB 会话、事务或回滚保证。

## 验证要求

仅修改本文档时，只对本文档执行 `git diff --check`。代码变更遵循[后端开发规则](../../../AGENTS.md#验证)和[统一质量门禁](../../README.md#质量门禁)。本 crate 的定向库单元测试命令在 `backend/` 执行，不运行集成测试或依赖真实 MongoDB 及其他外部服务的测试：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p persistence-core --lib --locked
```
