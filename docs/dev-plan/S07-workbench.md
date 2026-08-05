# S07 待办工作台

## 1. 元信息

- 分支：`feat/erp-s07-workbench`
- 业务期：`p1`
- 依赖阶段：`S00`（**复用** work_item 仓储，不平行实现第二套任务表）
- `must_compile=false`；`docs/dev-plan/S07-PATCHNOTES.md`

## 2. 目标与业务范围

| 能力 | 依据 |
| --- | --- |
| W01 今日工作台：指标/筛选/分组预览/新鲜度/轮询 | w01；phase-1 §4.6.2/§5.1.13 |
| 读模型：直查正式表或 ≤1 分钟可重建投影；**不另建分析库** | phase-1 §5.1.13 |
| W02 统一队列 list/filter/sort/`queueContextId` | w02；data-model work_item |
| 编排：CLAIM/batch-claim、DEFER、TRANSFER、CLOSE、WORK_ITEM_ACTION | W02 §7–§8.2 |
| **无**独立「标记完成」；完成=领域 completion_action | W02 §8.2 |
| 角标：`todo-count`/`confirm-count`/`delivery-count`/`warehouse-count` | workspace-registry / shell |

口径：待我处理仅个人有效任务；指标禁止前端求和；领取条件更新；CLOSE 仅误派/重复/有替代。

## 3. 明确不在范围

可配置审批流；企微待办；对外消息；W01 上通过/驳回/核销；「批量通过全部」；发明任务类型；新建分析库；用户偏好表未登记前禁止发明（recent 可空）；实现词上屏。

## 4. 代码落点

### owns_modules

```text
backend/services/src/workspace/
  mod.rs dto.rs query_today.rs query_tasks.rs query_badges.rs claim.rs action.rs family_map.rs freshness.rs
backend/apps/web-api/src/core/handler/admin/workspace/
  mod.rs today.rs badges.rs tasks.rs actions.rs
```

依赖 S00：`work_item` repository/entities。汇合文件仅 PATCHNOTES。

## 5. 数据模型与索引

主读写 `work_items`；可选 `workflow_actions`/`background_jobs`/`bulk_selection_*`。固定类型清单仅消费不扩展。任务族映射为展示分类（approval/finance/fulfillment/exception）。

## 6. API 与权限草图

| 方法路径 | 说明 |
| --- | --- |
| `GET /admin/workspace/today` | W01 |
| `GET /admin/workspace/nav-badges` | 四键角标 |
| `GET /admin/workspace/tasks`、`/{id}` | W02 队列 |
| `POST /admin/workspace/tasks/batch-claim` | 批量领取 |
| `POST /admin/workspace/tasks/{id}/actions` | 统一动作 |

permission：`workspace` / today|nav_badges|task_list|task_get|task_claim|task_action。

## 7. 前端集成点

- `features/workspace/`、`unified-task-queue/`
- shell 角标优先 nav-badges；query key 含 user/role/permissionVersion/timezone
- 状态码统一四态；destinationWorkspaceId 受控

## 8. 实现任务清单

DTO 对齐 W01/W02 §8 → 调用 S00 仓储 → query_today/tasks/badges/claim/action → handler → S07-PATCHNOTES → 分计/领取竞态/CLOSE 拒绝审批类单测

## 9. Worktree / 并行约定

`feat/erp-s07-workbench`；S00 后最早并行；不改交易域。

## 10. 验收标准

- [ ] W01 四指标分计正确；无业务写；统计可 stale
- [ ] W02 list/claim/batch/DEFER/TRANSFER/CLOSE 规则；无伪 complete
- [ ] nav-badges 四键；风格/文档；`must_compile=false`

---

*阶段 ID：S07 · 分支：feat/erp-s07-workbench · phase_tag：p1 · must_compile：false*
