# 阶段 17 导入确认复合响应修复执行证据

## 输入与归属

- 阶段 00 原始源码：`400ab4f7855255b284fe8a8e1caffe27acc96083`。
- 阶段 16 冻结源码：`72a0c79a2261d33699b869329376e534edfb1ef4`。
- 阶段 17 接收提交：`a537414eb8f78c43ebc383a3457a45c437dececc`。
- 实施树：`/private/tmp/erp-domain-crate-17-cutover`。最终源码已按末节逐 Git blob 重绑；不以仅工作树 SHA 代替冻结提交。
- 本补充授权覆盖 11 个新旧文件，精确清单及 SHA 以同名 JSON 为准。共享错误、Cargo、数据库、历史测试不在本片操作范围。

## 必须成立的最终归属

1. `ImportBusinessConfirmationWorkItemView`、`LegacyImportConfirmationView`、`CompleteImportBusinessConfirmationResult` 唯一归 `erp_processes::import_apply::dto`。原实体 `From<LegacyImportConfirmation>` 整体随目标 DTO 迁移。
2. 任务类型与状态直接使用 `erp_workflow` 的唯一 `WorkItemType`、`WorkItemStatus`；删除 `erp_import` 两个快照枚举及两个转换 helper。禁止使用字符串、JSON 值或第二份任务类型表代替真实生产类型。
3. 创建、完成、列表及 HTTP 响应仅改变相应类型导入。请求、确认领域枚举和其他领域 DTO 继续归 `erp_import`。
4. 授权投影通过 `erp_read_models::workbench::work_item_destination` 获取处理器及目标工作面。该出口只调用既有 `handler_route`，不持有第二份类型匹配表；工作台完整视图继续消费同一规则。

## 已恢复的原始行为

阶段 00 的任务类型和状态来自真实工作项。授权响应的 `handler_key` 与 `destination_workspace_id` 来自工作台实际路由。阶段 16 的单值 `ImportWorkItemType` 忽略输入类型，且授权响应将路由替换为 W18 常量，因此存在继承业务漂移。本修复明确登记为实际行为恢复，不计为单纯命名空间替换。

成功授权后，立即按实际任务的 type、业务对象和 owner role 调用唯一纯路由投影。例如 `BusinessException/integration_error_task` 返回 `business_exception/W29`，`IntegrationResultUnknown/integration_error_task` 返回 `integration_unknown/W29`。正常导入确认保持 `import_business_confirmation/W18`；未知映射和 W18 未注册责任范围保持原错误分类与字符串。此处经 root 拥有的穷尽 RM→Process 错误转换传播，不扩大错误降级策略。

原始 raw/read-only 路由本来固定 W18，继续固定。所有原有任务动作追加、责任人隐藏、只读 blocker、列表筛选和序列保持原实现。未增加类型 guard，未改变导入范围校验，未增加查询、RBAC 读取或事务步骤。

## 静态核销要求与结果

- 3 个完整 DTO 的字段、顺序、derive、serde 与阶段 00 token 相等；原实体 From 的完整实现与阶段 00、16 均相等。
- 原 raw/read-only/confirmation 投影及动作 helper 共 6 个完整定义与阶段 00 相等。
- 唯一 handler route、单据审批路由、W18 责任范围 helper 与阶段 00、16 分别相等；没有改变路由规则。
- 创建与完成命令文件从 `impl ImportApplyService` 起的全部实现 token 与修复前相等。
- 列表函数唯一变化是成功授权投影返回 `Result` 后原点传播路由错误；其余全部 token 相等。
- 共 25 项完整定义 token 检查通过；11 个实际文件与显式变换生成候选逐字节相等。
- 导入领域及 Process 的 108 个原测试定义仍存在，新增 3 个真实投影测试，合计 111 个定义。测试直接调用实际生产投影，覆盖两种非导入任务、三个状态、正常 W18、未映射路由及未知责任范围；未改变原测试函数。
- 已执行本片定向 rustfmt；未执行 Cargo、Clippy、库测试或数据库。运行结果须以 root 统一门禁为准，不将测试定义和静态核销视为运行通过。

## 交付文件

- `/private/tmp/cutover17-import-view-repair.json`：逐文件 SHA、输入提交、25 项完整定义证据、原测试清单、新投影测试清单。
- `/private/tmp/cutover17-import-view-migrate.py`、`cutover17-import-route-repair.py`：本片精确变换记录。
- `/private/tmp/cutover17-import-view-evidence.py`：可复核静态证据生成器。
- 原 202 个 Process 消费者证据必须引用本补充项；3 个受影响叶不得再声称只有命名空间变换。

## 冻结源码与统一门禁观察

- 最终 source：`39c55021d8bb1d030eade4afb3e135e4b5a8a18b`。本报告 11 个 after 文件均从实际 Git blob 读取，原证据 SHA、当前工作树与冻结 blob 三方逐字节一致。
- root 执行全 fmt/check/strict Clippy/lib 并报告退出成功；G 读取并哈希封存日志，没有重跑门禁。库测试日志逐 package 汇总为 **3655 passed / 0 failed / 68 ignored**，3 个新增 Import 投影测试均有实际 `... ok` 行。
- 日志：`/private/tmp/erp-cutover17-lib-tests-sealed.log`、`erp-cutover17-clippy-sealed.log`、`erp-cutover17-check-sealed.log`；精确 SHA 与观察边界见 JSON 的 `source_freeze`。
- 本轮只更新 /tmp 证据；没有修改仓库源码、执行 Cargo/数据库或全仓扫描。
- 本 JSON SHA-256：`1d286c43838000b76cbc23c4c33f6cb986a57e4c65bb3c271d8f9d39f5ebe97f`。
