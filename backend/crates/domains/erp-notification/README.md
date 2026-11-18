# erp-notification：通知系统执行合同

## 职责

`erp-notification` 是统一通知投递入口，拥有 MongoDB 的 `notifications` 集合，负责个人收件箱、站内信去重和已读状态。站外投递只通过注入的渠道端口执行。`RefreshHub` 提供进程内、无正文的刷新分发，不保存消息，也不承担可靠投递。

通知产生的业务条件、收件人选择和 outbox 归生产者；应用负责装配投递渠道、运行 worker、维护数据库订阅及暴露登录后的 HTTP/SSE 接口。通知和刷新都不授予业务对象的访问权限。

## 目录结构

| 文件 | 职责 |
| --- | --- |
| [Cargo.toml](Cargo.toml)、[src/lib.rs](src/lib.rs) | 依赖声明和公开入口；本 crate 不依赖生产者领域或应用层 |
| [src/entity.rs](src/entity.rs) | `Channel`、`NotificationData`、`Notification`；内容规范化、站内跳转约束和同键重放判定 |
| [src/dto.rs](src/dto.rs)、[src/error.rs](src/error.rs) | 收件箱游标、分页与展示投影，以及通知错误分类 |
| [src/service.rs](src/service.rs)、[src/ports.rs](src/ports.rs) | `NotificationService` 负责投递分流和收件箱用例；`NotificationChannel` 定义站外渠道合同 |
| [src/repository.rs](src/repository.rs)、[src/indexes.rs](src/indexes.rs) | 集合读写、去重查回、已读条件更新和公开索引登记入口 |
| [src/refresh.rs](src/refresh.rs) | 刷新主题、收件账号过滤、订阅可用状态与有界广播 |

## 架构设计

### 站内信、去重与事务

- 生产者调用 `NotificationService::notify`，不得直接写 `notifications`。服务先规范化去重键、收件账号、标题和正文，校验必填项、长度及可选站内跳转，再按渠道分流。一次调用只投递给一位收件人、一个渠道。
- `Channel::Inbox` 依靠 `(recipient_id, dedup_key)` 唯一索引去重。仓储先尝试插入；遇到唯一键冲突，按收件账号和去重键查回原记录。规范化内容相同则成功且不改动原记录，内容不同或查不到对应记录则返回 `Conflict`。已读消息不会因重放恢复为未读。
- 已读操作按消息 ID、收件账号和未读条件更新，只写入首次已读时间。列表和未读总数是两次独立读取，不承诺同一事务快照。
- 本域服务以 `NoTransaction` 调用站内仓储，不接入生产者事务。需要与业务写入一致时，在同一业务事务内保存生产者 outbox，提交后由 worker 调用 `notify`；业务写入、通知落库和 outbox 投递结果回写不是同一次提交。若通知已落库但结果未回写，重试复用原键和原内容，不回滚已成功的投递。

### 站外投递能力

`Channel::External(name)` 按名称查找注入的 `NotificationChannel` 并传入同一份规范化内容；它不写收件箱，也不复用站内唯一索引。[当前应用装配](../../../apps/erp-server/src/wiring/application.rs)传入空渠道表，合法消息请求未注册渠道时返回 `ChannelUnavailable`，不会被记为成功。

接入站外渠道时，适配器须设置网络超时，按 `(recipient_id, dedup_key)` 实现提供方幂等，以提供方确认接收为成功，区分 `Retryable`、`Rejected` 和 `ChannelUnavailable`。本 crate 没有站外投递状态或重试队列；生产者须按渠道独立记录结果和安排重试，不得用某一渠道的失败回滚或遮蔽其他渠道的成功。

## 用户消息生产者接入

1. 在业务事务中用同一执行器保存所属领域的 outbox，固定收件人、消息内容（或生成内容所需的模板参数）和稳定业务事件键。提交前不调用 `notify` 或外部提供方。
2. 当前接入的是审批通知。[工作流领域](../erp-workflow/src/service/approval/execution/runtime_service/notifications.rs)持久化审批 outbox；[应用 worker](../../../apps/erp-server/src/worker/approval_outbox.rs)轮询，[工作流投递端口](../erp-workflow/src/service/approval/execution/notification_worker.rs)领取租约并负责结果回写、退避和死信。只运行 web-api 不会消费审批 outbox。
3. [组合层 `ApprovalNotifications`](../../../apps/erp-server/src/wiring/notification.rs)把审批 outbox 的事件逐个收件人投递到 `Channel::Inbox`，使用 `approval:` 前缀加原事件键。全部收件人成功后才回写 `Delivered`；部分成功不会撤销，若整条 outbox 重试，已成功的收件人按原键、原内容重放。非法内容、内容冲突和拒绝投递归为永久失败，其余错误交给既有重试机制。
4. 当前审批 outbox 不提供可导航的对象 ID，消息统一进入 `/workspace/tasks`。不得从去重键推测实例 ID；精确跳转需先定义明确的来源引用合同。

## 个人接口

[账号路由](../../../apps/erp-server/src/http/routes/account.rs)复用 `/account` 的登录认证。[通知处理器](../../../apps/erp-server/src/http/handler/notification.rs)只使用认证会话账号作为收件人，不接受调用者指定收件人，不要求另行授予消息读取权限。收件箱是发送时归属该账号的历史消息；跳转仍须通过目标业务的授权。

| 接口 | 行为 |
| --- | --- |
| `GET /account/notifications` | 返回 `items`、`unread_count`、`has_more`；每页最多 20 条，按 `created_at DESC, id DESC` 排序；未读数是本人全量未读，不限于当前页 |
| `GET /account/notifications?before_created_at=…&before_id=…` | 使用上一页最后一条的时间和 ID 继续读取；两参数须同时提供，非法游标返回 400 |
| `POST /account/notifications/{id}/read` | 幂等标记本人消息；非本人或不存在均返回 404，重复操作保留首次已读时间 |
| `GET /account/notifications/events` | 使用 `Authorization: Bearer …` 建立 SSE，凭证不进入 URL |

列表中的每条消息只返回 `id`、`title`、`body`、`href`、`created_at`、`read_at`，不公开去重键和其他内部字段。

## 刷新事件来源

- `notify` 不直接推送 SSE。审批 worker 投递站内信后，API 通过 `notifications` 的已提交变更收到刷新信号；没有站内信的业务变化也可以独立触发刷新，刷新本身不会写入收件箱。
- 每个 API 进程由[应用订阅任务](../../../apps/erp-server/src/http/notification_feed.rs)建立一条数据库级 MongoDB 变更流，接收同库 API、worker 和其他实例的已提交变化。回滚不产生刷新；进程内广播仅负责本机 SSE 分发，缓冲区为 256。
- 拥有领域声明来源，[应用组合层](../../../apps/erp-server/src/wiring/notification_sources.rs)把来源映射为主题：`inbox` 对应站内信（包括新增和已读）；`copies` 对应审批抄送；`tasks`、`approvals`、`inquiries`、`selection`、`jobs` 对应工作项、审批实例、询价、选品册和后台任务；`sales`、`purchase`、`finance`、`fulfillment`、`authorization` 对应相关单据、财务、履约及授权来源。
- [持久化变更流](../../foundation/persistence-core/src/change_feed.rs)只投影主题、收件账号和驱动所需的变更字段，不传业务正文或对象 ID。`inbox`、`copies` 按不可变收件账号分发；个人来源在删除等情况下取不到账号时，当前实现退化为该主题的无正文共享刷新，不猜测收件人。共享刷新不绑定操作人、创建人或当前负责人，客户端须重新执行原授权查询。
- 订阅中断会广播不可用并关闭 SSE；应用尝试重建，恢复后要求全量同步。数据库级失效信号和广播接收落后也触发全量同步。不持久化消费位置，不通过刷新流重放业务命令。

## 推送协议与前端

- [SSE 处理器](../../../apps/erp-server/src/http/handler/notification/feed.rs)先订阅再发送含全部主题的 `snapshot`。后续以 200 毫秒窗口合并变化；批次包含 `inbox`、`tasks` 或 `authorization` 时发送 `snapshot`（`unread_count`、`todo_count`、`topics`），否则发送只含 `topics` 的 `refresh`，不查询计数。待办数沿用 `WorkItemReadService::work_item_search` 的 `mine` 范围，无列表权限时为 `null`，不是零。
- 连接约 60 秒后正常结束；发送业务帧及约 15 秒的空闲心跳前复验会话，心跳不查业务计数。凭证失效发送 `auth` 后关闭；会话设施、订阅或查询故障发送 `error` 后关闭，不以零或空流掩盖错误。
- [前端订阅](../../../../erp-client/features/notifications/hooks/queries.ts)由顶栏发起，同页其他组件读取共享缓存；正常结束后约 1 秒重连，失败后约 10 秒重试。重连取最新快照，不按事件条数累加。[刷新队列](../../../../erp-client/features/notifications/lib/refresh.ts)按主题合并、串行刷新对应查询，授权主题先刷新账号资料；即使数量不变，也按主题重新查询列表。

## 数据发布与回滚

1. 目标数据库须支持变更流，API 数据库账号须具备目标数据库的 `find`、`changeStream` 权限；部署前核对权限。订阅失败可见于服务日志和 SSE 错误。反向代理须关闭响应缓冲，保留 `text/event-stream` 和 `X-Accel-Buffering: no`，空闲超时大于 75 秒。
2. 先部署后端，由[启动索引登记](../../../apps/erp-server/src/indexes.rs)调用本域索引入口，或执行[通知索引迁移](../../../scripts/migrations/20261104-notifications.mongosh.js)。索引冲突须停止上线并核对数据，不得自动删除冲突记录。刷新流本身不增加集合、索引或业务字段。
3. 索引就绪后启动新版 `erp-worker`，再部署前端。worker 会消费仍可投递的历史 `Pending` 审批通知，首次上线可能出现历史消息，不得擅自清理 outbox。
4. 回滚时停止新版 worker，再恢复旧后端和前端。保留 `notifications` 集合、索引及历史数据，不改审批 outbox 结构，不重置或删除已 `Delivered` 的记录。

## 验证要求

仅改文档时，按[后端开发规则](../../../AGENTS.md#验证)只检查目标文件的 `git diff --check`。涉及实际推送的变更时，静态检查和单元测试不能代替真实 MongoDB、多实例和反向代理的部署验证。
