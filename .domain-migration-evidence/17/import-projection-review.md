# 阶段 17 Import 复合响应与路由恢复独立核验合同

## 1. 输入与判定

- 实际树：`/private/tmp/erp-domain-crate-17-cutover`。
- 阶段 00 权威：`400ab4f7855255b284fe8a8e1caffe27acc96083`。
- 阶段 16 冻结业务源码：`72a0c79a2261d33699b869329376e534edfb1ef4`；阶段 17 接收输入：`a537414eb8f78c43ebc383a3457a45c437dececc`。
- 最终业务源码：`39c55021d8bb1d030eade4afb3e135e4b5a8a18b`。
- 独立核验范围为 G 补充清单的 11 个真实文件，以及唯一 Workflow 枚举、RM DTO 导出、Process Error 三个直接 provider。所有 14 个登记 after 文件已逐实际 Git blob 核对，不以 HEAD 文本代替内容绑定。
- 32 项完整定义或实现 token 检查全部满足；未发现这次补充修复引入的非授权读取、授权、业务规则或错误顺序变化。完整检查名称、输入 blob SHA、after SHA 及批准的命名空间 token 归一化保存于同名 JSON。

## 2. 唯一 DTO 与 wire 合同

下列三个 DTO 的唯一真实出口为 `erp_processes::import_apply::dto`：

| DTO | 必须保留的实际合同 |
| --- | --- |
| `ImportBusinessConfirmationWorkItemView` | 13 个字段的名称、顺序、类型及 derive/serde 与阶段 00 完整声明相同；`work_item_type` 和 `status` 直接使用唯一 Workflow 枚举。 |
| `LegacyImportConfirmationView` | 16 个字段及完整实体 From 与阶段 00 相同；确认领域状态仍来自 Import，任务子视图来自 Process；原秒级时间转换、None 和版本字段不变。 |
| `CompleteImportBusinessConfirmationResult` | 6 个字段及 derive/serde 与阶段 00 相同；结果状态与 next_step 仍来自 Import，组合任务及确认视图来自 Process。 |

唯一 `WorkItemType` 和 `WorkItemStatus` 的完整 enum、derive 和 `SCREAMING_SNAKE_CASE` serde 声明同时保持阶段 00 与 16 的原定义。删除的 `ImportWorkItemType`、`ImportWorkItemStatus` 及两个转换 helper 不再有实际 Rust 消费者。不得通过单值枚举、字符串或 JSON 值再次代替任务真实类型。

原三个 DTO 及实体 From 从 Import 删除后，余下 Import DTO 代码 token 与阶段 16 相同。请求及确认领域枚举保持原拥有者。HTTP 直接导入 Process 组合响应；所有 HTTP 函数及 permission 宏完整尾部 token 保持阶段 16 原值。

## 3. 唯一路由 wrapper 合同

`erp_read_models::workbench::work_item_destination` 的实际实现严格等于：

```rust
pub fn work_item_destination(
    work_item_type: WorkItemType,
    business_object_type: &str,
    owner_role: &str,
) -> Result<(&'static str, &'static str)> {
    let route = handler_route(work_item_type, business_object_type, owner_role)?;
    Ok((route.handler_key, route.destination_workspace_id))
}
```

该 wrapper 没有 Database、RBAC、Executor、clock、ID、异步调用、类型 guard 或第二份路由表。`handler_route`、`document_approval_route` 和 `w18_confirmation_scope` 的完整函数 token 同时等于阶段 00 和 16。原完整 Workbench view 文件在扣除新增 wrapper 后保持阶段 16 token，不改变其他 mapper。

公开链为 `workbench::dto::view` 的真实函数，经既有 `dto` 的 `pub use view::*` 与 workbench 根显式导出。Process 调用后使用既有穷尽 RM→Process Error 转换，保留原 `ValidationError(String)` 类别和消息。

## 4. 授权投影、首错与逐行顺序

实际 `confirmation_list` 保持下列顺序：

1. `params.validate()`，然后 `normalized()`。
2. 按原字段构造 filter，查询确认分页。
3. 收集该页关联任务 ID，执行原一次任务预取并构造 HashMap。
4. 用原 db/rbac 构造 workflow service，按分页 items 原序逐行调用 `authorize_work_item(...).await`。
5. 授权成功时，先调用唯一 `work_item_destination(...)?`，然后映射原动作、追加确认动作、组装 blockers 和任务字段。
6. `Forbidden` 或 `NotFound` 仅使用预取实体形成原只读投影；其余授权错误立即返回。路由错误也立即返回，不进入只读降级，不继续后续行。
7. 当前行全部完成后才 push；最后用原分页信息组装响应。

原阶段 00 的动态 route 在 `WorkItemView::from_fields` 中先校验，随后确认投影取出 handler/destination。修复后的 route 同样位于授权成功之后、确认动作和结果行生成之前。原 `WORK_ITEM_HANDLER_UNMAPPED`、`IMPORT_CONFIRMATION_SCOPE_UNMAPPED`、已退役类型、业务对象未注册及单据审批映射错误保留类型和原字符串；route 的业务对象判定仍先于 W18 责任角色判定。

实际成功例保持唯一规则：正常 Import 为 `import_business_confirmation/W18`；`BusinessException/integration_error_task` 为 `business_exception/W29`；`IntegrationResultUnknown/integration_error_task` 为 `integration_unknown/W29`。未知角色不被默认成 SALES，未知对象不被默认成 W18。

raw 和 read-only 两个投影完整 token 等于阶段 00：始终保留原 W18 handler/destination 常量，同时返回关联任务的真实 type/status；只读投影继续隐藏 owner_user_id，只有 Open 状态追加原只读 blocker，不产生动作。`append_confirmation_actions` 的 Pending+Process 条件和两个动作追加顺序保持原值，不新增任务类型限制。

阶段 16 的单值类型 helper 与授权路径固定 W18 是本次明确授权修复的继承漂移，不能用“与阶段 16 不等”否定恢复。该结论仅覆盖本次复合 DTO 与目标路由修复，不将之前完整 Workbench 读取迁移冒称为本次重新审查的对象。

## 5. 命令与测试边界

创建与完成两个命令文件，从 `impl ImportApplyService` 起的全部实现及测试尾部与阶段 16 保持 token 等价，唯一归一化是已批准的 Services Error/identity adapter 路径。原事务、回执恢复、写入次序、时间和 ID 调用随完整实现保留。

3 个新增测试直接调用真实生产投影：

- `raw_and_read_only_views_preserve_actual_linked_task_type`：正常导入及两个非导入类型的序列化、真实 owner 与只读隐藏、固定 W18。
- `authorized_projection_keeps_actual_type_status_and_existing_action_rules`：三个真实状态 wire、版本、blocker、原动作和动态 W29。
- `authorized_projection_uses_registered_destinations_and_preserves_route_errors`：W18、W29、未知路由和未知责任范围的实际 typed 错误。

本审查未执行 Cargo、HTTP 应用或数据库。root 提供的封存日志另作为观察证据登记：库测试总计 3655 passed、0 failed、68 ignored；上述三个实际测试以及主错误审查的 10 个 HTTP 黄金测试均在日志中为 ok。494 表示这 10 个黄金测试完整执行时的响应 case 数量，不是测试函数数量。日志 SHA 和具体测试行保存于 JSON 的运行观察字段；未取得真实 MongoDB 验证。

## 6. 交付与重绑

- 本报告及 `/private/tmp/cutover17-import-projection-review.json` 保存独立判断和真实源码绑定。
- `/private/tmp/audit-cutover17-import-projection-review.py` 为静态审查生成器；它仅写 `/private/tmp`，不得将其静态结果替代 root 门禁。
- 仅按已登记文件进行最终 commit blob 重绑；发生源码差异时必须先复核相关实际符号，不得只更新提交号。
