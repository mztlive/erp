# 通知系统执行合同

## 职责与入口

- `erp-notification` 唯一拥有 `notifications` 集合、个人收件箱、通知去重和已读状态。
- 生产者必须调用 `NotificationService::notify(NotificationData, Channel)`。禁止绕过服务直接写集合。
- `Channel::Inbox` 完成持久化即返回成功；`Channel::External(name)` 调用组合根注册的 `NotificationChannel`。
- 首期不注册站外渠道。未注册渠道必须返回 `ChannelUnavailable`，不得记录为投递成功。
- 渠道适配必须设置网络超时，以 `(recipient_id, dedup_key)` 实现提供方幂等，区分 `Retryable`、`Rejected` 和 `ChannelUnavailable`。接入新渠道时必须配置独立投递状态，禁止某一渠道的失败回滚或遮蔽其他渠道的成功。

## 生产者接入

1. 在业务事务内保存所属领域的通知 outbox；保存不可变收件人、内容和稳定业务事件键。
2. 在事务提交后由 worker 调用统一通知服务。禁止在业务事务内调用 `notify` 或外部提供方。
3. 一条通知以一个收件人和一个渠道为投递单位。多收件人发生部分成功时，重试必须复用原键和原内容。
4. 站内信唯一索引为 `(recipient_id, dedup_key)`。同内容重放成功，异内容重放返回 `Conflict`；已读状态不得因重放恢复为未读。
5. 首期生产者为审批 outbox。组合层 `ApprovalNotifications` 将全部审批事件转入个人收件箱。沿用现有租约、退避和死信，不修改审批事务或 BPM。
6. 旧审批 outbox 没有独立导航对象 ID，消息统一进入 `/workspace/tasks`。禁止从去重键推测实例 ID；后续精确跳转必须先增加明确的来源引用合同。

## 个人接口

接口挂载在既有 `/account` 登录认证链路。收件人必须取会话账号，禁止接收用户指定的 recipient 参数。该接口与已有本人抄送接口一样，不要求管理员分配消息读取权限；消息跳转目标仍执行原业务授权。

| 接口 | 行为 |
| --- | --- |
| `GET /account/notifications` | 返回 `items`、`unread_count`、`has_more`；每页 20 条，按 `created_at DESC, id DESC` 排序 |
| `GET /account/notifications?before_created_at=…&before_id=…` | 按上一页最后一条继续读取；两个参数必须同时存在，同秒消息按 ID 继续 |
| `POST /account/notifications/{id}/read` | 幂等标记本人消息；非本人及不存在均返回 404；重复操作保留首次已读时间 |
| `GET /account/notifications/events` | Bearer Header 认证的 SSE；禁止把 token 放在 URL |

列表响应只提供展示字段，不公开去重键和内部系统字段。收件人变更不得转移历史收件箱。历史通知为发送当时的个人消息；跳转不授予单据查看权限。

## 推送协议与前端

- SSE `snapshot` 载荷为 `{ "unread_count": 0, "todo_count": 0 }`。`todo_count: null` 表示当前账号没有任务列表权限，禁止将其解释为零。
- 首帧发送最新计数，服务端每 5 秒重新读取一次。连接最长约 60 秒；前端正常结束后 1 秒重连，失败后 10 秒重连。每次重连重新发送全量计数，不累计增量、不依赖事件历史。
- 每轮发送前重新验证 JWT 和当前账号会话，并检查任务权限。失效发送 `auth` 后关闭；暂时故障发送 `error` 后关闭，禁止将故障伪装为零。
- 待办总数必须复用 `WorkbenchReadService::work_item_search` 的 `mine` 范围和当前授权。不从消息未读数、当前页任务数或全库任务数推算。
- 当前实现为服务端定时查询加 SSE 下发，跨 worker、跨实例读取同一持久化数据。每个浏览器页面拥有一条连接；部署容量评估必须计入每连接每 5 秒的权限、未读和任务查询成本。
- 顶栏建立唯一前端订阅，TanStack Query 管理取消、错误和缓存。侧栏“待我处理”只消费推送缓存；其他现有菜单计数维持原合同。
- 收件箱点击消息先标记已读，再进入站内地址；失败时显示重试提示。未读徽标通过下一帧同步，正常情况下不超过一个 5 秒查询周期。
- 待办数量变化时使工作台查询失效。筛选后的分类/期限计数仍由各自授权查询提供，不用全局待办数覆盖。
- 反向代理必须关闭此接口响应缓冲，保留 `text/event-stream` 和 `X-Accel-Buffering: no`；空闲超时必须大于 75 秒。

## 数据发布与回滚

1. 先部署创建索引的后端，执行正常启动索引登记或 `scripts/migrations/20261104-notifications.mongosh.js`。
2. 索引成功后启动新版 `erp-worker`，再部署前端。只运行 web-api 不会消费审批 outbox。
3. 新版 worker 会消费过去保留的 Pending 通知，因此首次上线可能收到历史审批消息；不得擅自清除历史 outbox。
4. 回滚时停止新版 worker、恢复旧后端和前端；保留 `notifications` 集合和索引。已投递的审批 outbox 保留 Delivered，不得重置或删除其事实。
5. 本变更新增集合，不修改已有 outbox 数据结构，不删除历史数据。索引冲突必须停止上线并核对数据，禁止自动删除冲突记录。

## 验证要求

- 运行通知域、审批 outbox 和 HTTP 通知适配的内联单元测试；运行 workspace 编译、受影响 crate clippy、领域边界、BPM 边界和权限漂移检查。
- 前端运行 lint、格式检查、TypeScript 和构建。
- 真实 MongoDB、多实例推送、反向代理缓冲、已读并发和浏览器视觉验收必须单独记录；静态检查与单元测试不得替代这些证据。
