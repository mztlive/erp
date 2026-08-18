# W02 · 统一待办队列（已废止）

> 状态：已废止
> 废止日期：2026-08-17
> 权威合同：`../approval-workflow-contract.md` §16.4
> 能力去向：并入 [W01 · 我的工作台](w01-today-workspace.md)
> 旧路由：`/workspace/tasks` 永久重定向到 `/workspace`

## 1. 废止声明

W02 不再是有效实施依据。原「今日工作台」与「统一待办队列」已合并为唯一页面 W01。

任何实现、测试、导航或文档再把 W02 写成独立待办入口、保留 `/workspace/tasks` 第二套页面，或继续提供「团队待处理 / 领取 / 开始处理 / 退回团队」，均为阻断。

## 2. 并入 W01 的能力

下列能力必须在 W01 实现，不得留在本文件或 `erp-client/features/unified-task-queue/**`：

1. 跨领域待办列表的查询、筛选、排序、游标分页；
2. 列表 + 详情主从，页内连续处理；
3. 审批任务在详情内提交通过 / 驳回；
4. 受阻样式与服务端授权的恢复 / 改派 / 取消；
5. 我发起的审批、管理视图（与本人口径分开计数）。

## 3. 随废止删除的能力

下列能力随合同 §13.2 从全系统删除，W01 也不得复活：

- 责任范围 `scope=team` 与「团队待处理」分区；
- `assignment_mode=POOL`、空 `owner_user_id` 的开放任务；
- 领取、开始处理、退回团队；
- 审批任务的通用转交与通用关闭；
- `RETRY_CURRENT_STEP` 作为通用阻塞恢复。

## 4. 实现清理

| 项 | 要求 |
| --- | --- |
| 路由 | `/workspace/tasks` 只保留永久重定向，不渲染本页 |
| 前端目录 | `erp-client/features/unified-task-queue/**` 的列表能力并入 `features/workspace/**` 后整目录删除 |
| 导航 | 侧栏不得再出现「待办队列」二级入口 |
| 文案 | `ui-glossary.md` 中 W02 的用户提示名改为「我的工作台」 |
| 引用 | 其它 W 文件、`erp-ui-flows.md`、`erp-ui-design.md` 不得再把本页当作有效处理面 |

## 5. 业务依据

- `approval-workflow-contract.md` §16.4、§13.2。
- `docs/approval-workflow-implementation-plan/08-frontend-document-and-workbench.md` §5。
