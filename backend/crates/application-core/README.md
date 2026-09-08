# application-core：共享应用合同

## 职责合同

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
| [src/error.rs](src/error.rs) | Error 与 ErrorClass |
| [src/owned_task.rs](src/owned_task.rs) | await_owned |

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p application-core --locked
env -u ERP_TEST_MONGO_URI cargo test -p application-core --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
