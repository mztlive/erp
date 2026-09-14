# application-core：共享应用合同

提供多个业务用例共同使用的应用层约定：分页结果、调用人上下文、命令身份、重试匹配和错误分类。

不同领域需要用一致方式表达“谁发起请求、怎样分页、重试的是不是同一个操作”。在这里统一这些与具体业务对象无关的类型和规则。

## 使用场景

- 复用分页参数、分页结果、查询规范化或调用人审计上下文。
- 为命令构造稳定指纹，判断重放结果，或使用关键异步操作等待辅助。

## 协作示例

同一个命令再次提交时，调用方用 CommandIdentity 和 CommandFingerprint 判断是否与原请求一致；实际结果和审计记录由拥有领域保存。[erp-core](../erp-core/README.md) 提供金额等基础值类型，[persistence-core](../persistence-core/README.md) 提供数据库机制。

## 负责的数据与能力

- 分页与查询规范化、调用人审计上下文、应用错误分类。
- 版本化命令指纹、命令身份、回执事实与幂等匹配。
- 在独立 Tokio 任务中运行关键操作，使调用方取消不终止收尾流程的异步等待辅助。

## 使用与边界要求

1. 仅依赖共享内核及通用技术库；不得引入领域实体、领域仓储或组合层。
2. CommandFingerprint 的字段顺序由调用方固定；集合输入须按业务语义规范化。不得用 Debug 或不稳定序列化代替稳定指纹。
3. 回执事实的存储由拥有领域提供；本 crate 只保留通用合同与匹配规则。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/command.rs](src/command.rs) | CommandFingerprint、CommandIdentity 与 CommandReceipt |
| [src/context.rs](src/context.rs) | AuditActor |
| [src/page.rs](src/page.rs) | Page |
| [src/query.rs](src/query.rs) | PageView、分页与排序规范化 |
| [src/query_ids.rs](src/query_ids.rs) | QueryIds 及过滤结果类型 |
| [src/error.rs](src/error.rs) | Error 与 ErrorClass |
| [src/owned_task.rs](src/owned_task.rs) | await_owned |

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p application-core --locked
env -u ERP_TEST_MONGO_URI cargo test -p application-core --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
