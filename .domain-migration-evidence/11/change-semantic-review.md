# 阶段 11 采购变更与责任接口静态核验

## 1. 适用范围与证据边界

- 输入版本固定为阶段 11 工作树的输入提交 `07da7863`；输入源以 `git show HEAD:backend/services/src/purchase_order/...` 提取。
- After 源码提交固定为 `0a297ab00d29b28047e05f780b4815c0a5456a2f`；采购变更源文件与 sealed inventory 及提交字节一致。冻结指纹登记于 `/tmp/procurement11-change-sealed-source-fingerprints.json`。
- 本核验不执行 Cargo、不运行历史 `tests/**`、不连接 MongoDB。结果只作为源码语义核销依据，不能替代本地门禁或数据库验证。
- 逐函数对照记录固定为 `/private/tmp/procurement11-change-function-comparison.json`。25 个函数在导入路径和 `self.domain()` 接收者归一后，函数体全部相同。

## 2. 必须保持的采购变更执行合同

| 合同 | 当前实现核验结果 |
| --- | --- |
| 发起变更 | 请求校验、采购单版本与当前版本读取、在途变更检查、新变更 ID 生成、来源销售读取、独立 BusinessDocument 绑定、审计构造与事务写入顺序保持。纯采购读取和构造转入领域服务，不提前分配 ID。 |
| 冻结提交 | 原基准版本读取、空 lines 时读取基准明细、当前供应商名称读取、行校验/金额计算/付款代码回退、付款条件解析、提交序号查询、提交 ID 分配、来源销售当前行读取和窄事实映射、明细构造、Instant::now/submit、原请求行 Debug/SipHash 指纹顺序保持。 |
| 付款头字段 | gross/net/tax 来源仍为同一 `to_line_inputs` 和 `compute_request_totals`；supplier ID/类型/履约责任来自原采购单；供应商修订及快照来自基准版本；付款代码的 None 回退到基准快照。 |
| 当前销售行映射 | 物流行清空全部销售关联和 allocated_quantity；商品行先稳定销售 ID，再当前版本映射，再 quantity；分配量固定为变更后 quantity；错误文案及首错顺序保持。 |
| 启动回执 | change_start 全部生产函数体保持，包含幂等键规范化、start_identity/scope candidates、payload conflict、同载荷回执验证、原 runtime/开放任务/CAS/审计写入。 |
| 启动恢复 | recover_purchase_change_start 函数体保持：仅 command_may_have_committed 进入恢复；最多 8 次 fresh session 回读；在对应 executor 中读取变更/采购/销售、组织可读性、绑定及回执；主题版本和冻结提交所属对象校验保持；延迟次数和原错误回传保持。 |
| 撤回 | change_cancel 全部生产函数体保持；cancel list projection、CAS 确认、任务关闭、状态退草稿及 subject_version 不回退合同保持。 |
| 生效准备 | 原采购单读取和基准当前性检查、冻结提交 pending 检查及明细读取、revision_no、修订与修订行 ID 构造、基准版本读取、财务差额构造顺序保持。原事务入口调用该准备时仍使用原 NoTransaction 读取；本迁移未将其描述成新的事务内重验。 |
| 应付差额 | 差额保持 new_gross - base_gross；零差额无账户/分录 ID 或业务日期/posted_at 读取；非零先账户 ID 和账户校验，再分录 ID、due_date、posted_at。负差额仍由原 PayableAccount 校验拒绝，不能凭 direction=Decrease 分支声称新支持减额。 |
| 成本差额 | 输入版本 `build_change_deltas` 始终返回 `Vec::new()` 成本项。当前执行器省去该恒空循环，未增加成本构造或写入。 |
| 生效事务 | 构造审计、内存 mark_effective 后，按 SalesGuard → PrepareAllocations → Revision → Allocations → CurrentOrder → ProcurementTasks → Payable → Submission → Change → Audit 执行；所有步骤复用调用方 executor，返回仓储更新后的采购 version。 |
| 直接生效入口 | `reject_client_effect` 保留原 ConflictError 和中文错误文案。对外接口权限和接线由根集成人另行完成。 |

## 3. 责任分片外部事实与错误合同

1. `IdentityOwnerFact` 明确映射 `account.base.id`、`account.name`、`account.can_login()` 和 `account.is_kind(AccountKind::Admin)`。资格判断仍同时要求 can_login 与 is_admin；姓名只用于展示，不能代替账号 ID、角色或权限。
2. ProductKind 按 Physical、Virtual、OfflineService、Voucher 四分支逐项转换；wire 保持 PHYSICAL/VIRTUAL/OFFLINE_SERVICE/VOUCHER，不能由 category.product_kind 替换 product.product_kind。
3. 采购责任 EnableStatus 保持 active/disabled 的 snake_case wire；规则选择器和列表状态不消费枚举 ordinal。
4. CatalogBundle 的 SKU→product、product.current_revision、product_kind、revision.category、category.parent 均逐字段原值投影，稀疏缺失继续由采购纯规则检测。
5. 目录 Port 返回原 `persistence_core::Result`，故原 `Internal(error.to_string())` 包装不会多出领域 RepositoryError 前缀。账号及选择器 Port 错误经原仓储分类转换。
6. `services::Error::from(erp_procurement::Error)` 逐项 typed 匹配保留 Internal、NotFound、ValidationError、BusinessLogicError、ConflictError、ReceiptDuplicate、TransientTransaction、Forbidden、Unauthenticated、Logic、OutcomeUnknown 和 RepositoryError。HTTP 采购转换复用该 services typed 映射；原状态码路径保持。
7. 责任规则创建/更新仍使用原两处 `run_authorized_policy_transaction(policy_revision, ...)`，事务内目录引用重验、负责人资格重验、规则写入、审计写入顺序保持。

## 4. 当前核验结论与执行要求

- 本次读取的采购变更函数未发现新增回执、幂等身份、金额、时钟/ID 分配时点、写序或失败语义漂移。
- 25 个未拆分核心函数的函数体一致；拆分的 freeze/header/effect/payable 组合按第 2 节合同逐项核销。
- 采购责任字段和 typed 错误静态链路闭合；本分片定向 rustfmt 与任务范围 git diff --check 已执行通过。
- 根集成人必须继续执行阶段 11 全量门禁及最终静态合同采集。当前未执行的 Cargo、数据库和 HTTP 运行验证不得记为通过。
