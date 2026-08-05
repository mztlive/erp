# Database

`database` crate 是 MongoDB 持久化边界。它负责连接、索引、通用仓储、实体专用查询、
Casbin policy 存储和事务执行；业务规则与跨仓储流程仍由 `entities` 和 `services` 负责。

## 目录

- `src/connection.rs`：创建 MongoDB `Client` 与 `Database`。
- `src/indexes.rs`：幂等创建当前模型依赖的唯一索引和查询索引。
- `src/repository/`：通用 `Repository<'a, T>`、实体专用查询和 `DatabaseExt`。
- `src/casbin_adapter.rs`：`casbin_rules` 与全局 revision 的 `MongoCasbinAdapter`。
- `src/transaction.rs`：`Client` 上的 `Transactional::with_transaction`。
- `src/errors.rs`：数据库、乐观锁和事务提交结果错误。

## 仓储访问

Service 通过 `DatabaseExt` 获取固定集合对应的仓储，不在 Service 或 Handler 中持有
MongoDB `Collection`：

```rust
use database::DatabaseExt;

let (_, db) = database::connect(uri, db_name).await?;
database::ensure_indexes(&db).await?;

let account = db.accounts().find_by_id(account_id).await?;
# Ok::<(), database::Error>(())
```

当前访问器覆盖账号、审计日志和角色。
通用 CRUD、分页、乐观锁以及事务内软删除/恢复位于 `Repository`；只有通用能力无法准确
表达的查询才放在对应的实体仓储模块中。详细约定见
[`src/repository/README.md`](src/repository/README.md)。

## 写入语义

- `update`、`update_with_session`、`soft_delete_with_session`、`restore_with_session`
  使用 `id + version + deleted_at` 做比较并交换。
- 写入命中后，传入实体的 `version`、`updated_at` 和 `deleted_at` 会同步到数据库结果。
- 没有命中预期版本或状态时返回 `OptimisticLockingError`。
- `_with_session` 方法只加入调用方提供的 `ClientSession`，不会自行提交事务。
- 多集合或多步骤原子写入由 Service 使用 `Transactional::with_transaction` 编排。
- 软删除与恢复只暴露 session 版本，防止绕过 Service 的事务和审计边界。

事务行为与提交结果未知时的处理见 [`TRANSACTIONS.md`](TRANSACTIONS.md)。

## 索引

Web API 启动时调用 `database::ensure_indexes`。索引定义集中在 `src/indexes.rs`，
包括账号和 Profile 身份唯一约束、列表/区域查询索引及 Casbin policy 查询索引。
新增或修改查询时应同步评估索引，具体清单见
[`../docs/mongodb-indexes.md`](../docs/mongodb-indexes.md)。
