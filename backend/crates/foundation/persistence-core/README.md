# persistence-core：MongoDB 基础设施合同

提供各业务仓储共用的 MongoDB 连接、通用数据操作、分页与事务执行机制。

各领域拥有自己的集合和查询，但参与事务、执行通用 CRUD、处理分页的技术方式应保持一致。本 crate 提供这些底层机制。

## 使用场景

- 实现业务 Repository 时复用通用 Repository、分页及过滤工具。
- 接入数据库连接，或让多步写入共享同一个事务执行器。

## 协作示例

采购与财务写入需要一起成功时，Process 开启事务并将同一个 Executor 传给各领域仓储。这里的 Executor 表示本次数据访问使用的执行上下文；具体采购和应付查询仍由对应领域定义。

## 负责的数据与能力

- MongoDB 连接、事务能力检查、执行器与事务封装。
- 通用 Repository、分页过滤、正则字面量过滤和 MongoDB 操作辅助。
- 命名普通、唯一及部分唯一索引的通用构造；领域必须提供索引名称、键顺序与过滤条件，组合根必须保持既定登记顺序。
- `rotation` 提供调用方集合内游标读取、版本比较写入和标量到数组迁移机制；集合名、匹配与分派策略由拥有领域提供。

## 使用与边界要求

1. 仅提供通用持久化能力，不拥有业务实体、业务集合名、业务查询或业务索引。
2. Repository 操作沿用 Executor；独立自动提交操作使用 NoTransaction，需要交付复验的非事务读取使用同一个 ReadExecutor，多集合原子操作使用 Transactional::with_transaction。
3. connect 创建客户端和数据库句柄；事务部署能力须另调用 ensure_transaction_support 验证，不能把句柄创建成功视为数据库健康验证。
4. 业务仓储、集合访问器和索引定义留在拥有领域；HTTP 错误映射留在应用边界。

## 读取边界

`Transactional::with_transaction` 在开启 snapshot 读取和 majority 提交的事务后，向回调提供带唯一 `ExecutionBoundary` 的执行器。边界同时标识事务和当前读取阶段。同一会话开启下一事务时，必须取得新边界。

`Executor` 仅由本 crate 实现。业务使用本库提供的执行器，不能通过自定义实现重新挂载旧边界。边界只持有阶段的弱引用；复制边界不能延长阶段生命周期。执行器退出、事务重启、阶段推进或原始会话暴露后，全部旧边界失效。

非事务授权读取创建 `ReadExecutor::default()`，查询生成、仓储读取和结果复验必须复用该实例。开始下一读取阶段时调用 `Executor::next_read()`。`NoTransaction` 仅用于独立自动提交操作，不提供授权凭证所需边界。

统一写操作在尝试写入前调用 `next_read()`；失败的写入尝试也使旧读取凭证失效。只读操作保持当前边界。仓储使用 `mongo_ops`、`aggregate` 或通用 Repository 执行数据访问，不直接取得事务会话。调用 `Executor::session()` 会永久撤销当前事务执行器的边界证明，不能据此跳过授权依赖复验。

`test-support` feature 只允许在 dev-dependency 中启用。`testing::SnapshotExecutor` 用于无数据库的执行合同测试，不提供真实 MongoDB 事务或回滚保证。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/connection.rs](src/connection.rs) | connect 与 ensure_transaction_support |
| [src/executor.rs](src/executor.rs) | Executor、ExecutionBoundary、ReadExecutor 与 NoTransaction |
| [src/transaction.rs](src/transaction.rs) | Transactional |
| [src/repository/mod.rs](src/repository/mod.rs) | Repository、PageResult、Pagination 和 QueryFilter |
| [src/mongo_ops.rs](src/mongo_ops.rs) | 统一执行器下的 MongoDB 操作 |
| [src/indexes.rs](src/indexes.rs) | 命名索引构造；不拥有业务索引定义或执行登记 |
| [src/reset.rs](src/reset.rs) | 显式全库重置机制，按领域提供的物理身份保留文档，须在事务中调用 |

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p persistence-core --lib --locked
```

代码变更还须执行[统一质量门禁](../../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
