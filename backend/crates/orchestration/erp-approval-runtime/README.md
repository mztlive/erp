# erp-approval-runtime

本 crate 提供审批启动、原实例回放、材料冻结及文件读取重验的跨域执行合同。调用方必须注入工作流授权 Port，并沿用业务提交的同一 `Executor`。本 crate 不分派业务审批动作，不重放未知提交的事务，不选择新的审批实例替代原回执。

公开入口为 `approval_start`、`approval_recovery`、`approval_materials`、`material_writes` 和 `approver_directory`。应用负责装配具体授权提供者，消费方负责将本地错误按变体转换到自己的边界。
