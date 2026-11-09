# 任务链路迁移执行合同

## 数据与接口

- `work_items.links` 保存 `{ object_type, object_id }` 数组，包含直接对象与全部上级业务单据，按类型和 ID 去重，最多 1000 项。该字段不得包含姓名、单号或业务材料。
- `approval_subject_snapshots.links` 保存提交时的同一链路。审批入口、后续节点和驳回待办必须继承匹配实例、对象及提交版本的快照。
- 查询归 `erp-workflow::WorkItemReadService`。人员显示及权限范围直接使用 identity；来源查询只读取 `work_items`。链路缺失不得回退到跨域查询。
- 前端通过人员目录、销售单目录和采购单目录选择对象，向任务接口提交 `filters.handlers`、`filters.sales_orders`、`filters.purchase_orders` 的 ID 数组。既有逗号分隔 ID 参数保留兼容；两种表达同时出现时取交集，空交集返回校验错误。
- `q` 不查询人员姓名。人员目录自身的姓名搜索只用于选择 ID。任务访问范围、首末授权版本复验与跨页范围版本校验继续执行。

## 发布顺序

1. 停止会创建、更新审批或工作项的 HTTP 与 worker 写入；备份目标数据库。禁止旧进程在回填期间或新读取启用后继续生成缺链路任务。
2. 在目标库使用 `scripts/migrations/20261110-work-item-links.mongosh.js`。连接信息由执行环境提供，禁止写入仓库或日志。
3. 默认 `MODE=audit`，先处理 `COLLECTION=approval_subject_snapshots`，再处理 `COLLECTION=work_items`。每批默认 200 条，可用 `BATCH_SIZE` 设置 1 至 1000；用输出的 `last_id` 作为下一批 `AFTER_ID`，直到 `scanned=0`。
4. 有 `unresolved` 或 `conflicts` 时停止切换，补齐准确来源或处理并发冲突，再从失败批的原游标重跑。历史付款申请必须使用精确提交快照；缺失记录、软删除来源或无法证明的历史版本不得采用当前草稿补齐。
5. 审计全部通过后，设置 `MODE=apply WRITES_STOPPED=yes`，按同样顺序与分页执行。脚本仅补缺失或空链路，写入前保存原值与版本，使用版本条件更新并递增版本。脚本创建 `idx_work_items_links_owner_status` 索引。
6. 重新从空游标执行审计，两个集合均须没有未补齐行；核验抽样任务的人员、销售来源、采购来源及两类来源交集，验证列表总数、分页和统计口径。
7. 启动本次版本的 HTTP 与 worker，重新加载任务第一页取得新范围版本。生产数据回填、索引构建和查询计划须在目标环境单独验收；本地单测不构成该验收。

## 回滚

1. 停止新版本业务写入并恢复旧应用；保留新增字段及索引时，旧应用可忽略它们。
2. 需要撤销本次回填时，设置 `MODE=rollback WRITES_STOPPED=yes`，分别按两个集合及 `AFTER_ID` 分批执行。
3. 回滚仅允许恢复仍处于回填后版本、且链路与备份一致的记录；恢复原字段状态并继续递增版本，不回退版本号。出现冲突时不得覆盖后续业务修改。
4. 保留 `work_item_link_migration_backups` 和新增索引用于追溯；本脚本不删除业务记录。回滚后再次启用新版本前，必须重新审计并处理已回滚备份。
