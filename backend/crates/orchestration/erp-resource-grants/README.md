# 独立自动读取授权

本 crate 由独立 `erp-worker` 装配。处理 `InvoiceApproved`、`InvoiceTaskReassigned`、`SalesOrderApprovalAssigned`、`PurchaseOrderApprovalAssigned`，规则版本为 1；不得执行或重放业务审批、派工或财务命令。

## 事件合同

- 生产方须在业务事务内调用身份域 `append_event`。开票事件只能在最终审批通过并创建执行任务后追加。
- 审批事件的 `assignment_id` 必须为正式 `ApprovalNodeExecution.id`，`node_id` 为冻结节点键，事实版本为审批实例版本。禁止使用候选审批人或定义绑定 ID。
- 开票事件的事实版本为申请版本。申请、审批实例、执行任务、应收子账及销售单必须全部匹配；任务历任正式责任人必须包含事件接收人。
- 完成、取消、转派后仍根据历史正式分配事实计算读取权。开票转派通过独立 `InvoiceTaskReassigned` 生成新接收人的来源；事件绑定任务与不可变命令回执，使用回执中的任务版本和接收人，不得伪造 `InvoiceApproved`。

## 执行合同

1. 每次领取使用独立令牌，租约为 300 秒；过期 Processing 事件允许重新领取。
2. 发布事务内重读事件、来源及当前业务事实；来源永久终止时仅完成事件，不恢复授权。
3. 授权行、来源、修订号、审计及事件 Applied 状态同事务提交。当前事实版本不得小于事件版本；旧结果不得覆盖更高版本。
4. 事实缺失、关联冲突、接收账号失效及持久化失败保留原授权并记录失败。退避从 2 秒开始，上限 900 秒。
5. 提交结果未知时不立即重放；保留领取状态，待租约恢复后按事件键读取已提交状态。事务 Future 不设置可中断超时；停止时完成当前事务后退出。
6. 每 30 轮扫描最多 100 条正式事件，按唯一主键续页；只重新排队 Applied 事件，不抢占处理中或失败退避事件。扫描结束后开始下一轮。
7. `recompute` 支持指定来源、固定规则、资源类型及全量分页。禁止从当前负责人或用户全量授权 ID 重建历史。
8. `readiness` 只接受服务端正式事件并重验事实；`NotReady` 表示合法来源尚未生成当前版本授权。该状态不能代替身份、功能权限及当前办理资格检查。

## 运维与接入

执行 `scripts/migrations/20261109-resource-grant-worker.mongosh.js` 建立新增索引；服务启动的身份域索引入口使用相同定义。回滚先停止 worker，再按迁移脚本列出的名称删除新增索引，保留事件、来源、授权和审计数据。

worker 记录积压量、最老未完成秒数、失败量及最近来源事实版本；单事件保存失败原因与领取信息。进程监督器检测循环退出，统一停止与排空覆盖本 worker。

业务生产方在审批启动、节点推进、恢复正式分配及开票最终批准事务内追加事件。业务入口通过 `source_readiness` 按精确来源读取正式事件并核对当前事实；HTTP 使用 `RESOURCE_AUTHORIZATION_NOT_READY` 表示资料授权处理中。前端只轮询资料查询，不重放业务命令。不得产生历史分配猜测事件；存量业务回填必须使用下述分页维护接口。

## 验证范围

规则单元测试执行实际校验逻辑，覆盖四类合法来源与审批、节点、接收人、任务、子账、销售单关联错误。身份域测试覆盖来源幂等、版本倒退、同版本异载荷和永久终止。

真实 MongoDB 事务原子性、并发领取、未知提交恢复及迁移执行须在获准的运行环境另行验收；单元测试和静态门禁不得作为该验收证据。

## 管理接口

所有接口均位于 `/admin/resource-grants`，入口要求 `data_scope:create`；服务另验同角色的全公司 `org_unit:manage`，存在个人范围上限时拒绝。来源查询同样要求维护资格；授予、撤销、重算和回填的资格检查与写入共享事务。

| 方法及路径 | 输入 | 结果 |
| --- | --- | --- |
| `POST /admin/resource-grants` | `command_id`、`recipient`、`targets: [{resource, id}]`、`reason` | 人工来源及其版本；`resource` 仅为 `sales_order`、`purchase_order` |
| `GET /admin/resource-grants/sources` | 查询参数 `recipient`、可选 `after` | 最多 100 个来源及 `next_cursor`，包含永久终止记录 |
| `POST /admin/resource-grants/revoke` | `source: {rule, business_type, business_id, recipient}`、`expected_version`、`reason` | 永久终止后的来源 |
| `POST /admin/resource-grants/recompute` | `selection`、可选 `after` | 最多重排 100 条已完成事件，返回下一页游标；空值表示结束 |
| `POST /admin/resource-grants/backfill` | `kind`、可选 `after`、`dry_run`（默认 true） | 最多扫描 25 条正式证据，返回统计、无效证据和下一页游标 |

人工来源固定 `rule=manual`、`business_type=manual`，版本固定从 1 开始。相同操作号、接收人及目标重复提交不增加授权；同来源异目标拒绝。改变授权须终止原来源后使用新的操作号。人工来源与自动来源互不覆盖，所有来源均仅授予读取权。

`selection` 仅接受 `{"kind":"all"}`、`{"kind":"source","value":来源身份}`、`{"kind":"resource","value":"sales_order"或"purchase_order"}`、`{"kind":"rule","value":规则代码}`。规则代码固定为四类事件的 snake_case 名称。重算不抢占处理中或失败退避事件，不恢复永久终止来源。

## 存量回填与切换

1. 执行新增索引脚本 `scripts/migrations/20261110-resource-grant-maintenance.mongosh.js`，确认索引建立成功后启用维护入口。先部署支持四类事件的 worker，再部署转派生产方；所有参与记录写入实例均须同步维护账号修订号。
2. 分别以 `kind=invoice_approvals`、`order_assignments`、`invoice_reassignments`、`dry_run=true` 从无游标开始预览，使用每页 `next_cursor` 继续，直至返回空游标。保留每页报告并核查 `invalid`。
3. 开票批准证据必须存在最终批准实例、对应版本申请、申请绑定的执行任务及一致的子账和销售单；初始接收人取任务持久化责任历史的首项。转派接收人必须存在有效命令回执和相应责任历史。审批分配须有冻结节点和实际生成的审批任务。缺失证据不得从当前负责人推断。
4. 以相同类型重新从无游标开始，显式传 `dry_run=false` 执行。每页重新读取和验证事实；`eligible` 为本次可新增来源数，`appended` 为新增事件数，`existing` 为已有事件来源数，`terminated` 为永久终止来源数，`invalid` 为未通过的记录。无适用订单或无实际任务的证据不生成事件。
5. 一页内的授权事件与维护资格检查共享事务；数据库故障回滚整页。无效业务证据记入报告，不产生该证据的授权。提交结果未知时不得自动重试；按同一页参数人工重查。已有来源事件不重复追加，终止来源持续排除。worker 只处理授权，不重放审批、派工或财务命令。
6. 回填结束后检查 worker 队列及失败记录，按设计文档验收 S001/S002 隔离、转派后新接收人授权、旧来源保留和永久撤销。预览、编译及单元测试不得代替实际回填和真实数据库验收。
7. 回滚先关闭维护与转派事件生产，再停止 worker；保留事件、来源、授权、审计及参与修订号。旧 worker 不支持新事件，不得直接降级消费。删除新增索引按迁移脚本的显式 `rollback` 模式执行；不删除业务数据或撤销记录。
