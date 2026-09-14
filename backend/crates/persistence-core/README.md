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

## 使用与边界要求

1. 仅提供通用持久化能力，不拥有业务实体、业务集合名、业务查询或业务索引。
2. Repository 操作沿用 Executor；单集合操作按需要使用 NoTransaction，多集合原子操作使用 Transactional::with_transaction。
3. connect 创建客户端和数据库句柄；事务部署能力须另调用 ensure_transaction_support 验证，不能把句柄创建成功视为数据库健康验证。
4. 业务仓储、集合访问器和索引定义留在拥有领域；HTTP 错误映射留在应用边界。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/connection.rs](src/connection.rs) | connect 与 ensure_transaction_support |
| [src/executor.rs](src/executor.rs) | Executor 与 NoTransaction |
| [src/transaction.rs](src/transaction.rs) | Transactional |
| [src/repository/mod.rs](src/repository/mod.rs) | Repository、PageResult、Pagination 和 QueryFilter |
| [src/mongo_ops.rs](src/mongo_ops.rs) | 统一执行器下的 MongoDB 操作 |

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p persistence-core --locked
env -u ERP_TEST_MONGO_URI cargo test -p persistence-core --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
