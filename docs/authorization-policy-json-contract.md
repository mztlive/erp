# 授权 JSON 配置执行合同

版本：1.0

## 1. 配置边界

1. 授权文件采用 `version: "1.0"`，包含 `roles`、`bindings`、`data_scopes`。文件不包含密码、Token、数据库连接或审批任务指派。
2. 角色和人员绑定必须指定 `mode: "merge" | "replace"`。merge 保留当前集合并加入声明项；replace 仅完整替换当前条目指向的角色权限或人员绑定。文件未列对象保持原配置。
3. 人员范围必须指定 `mode: "replace"`；替换边界为当前人员、当前资源、所选动作。空 grants 恢复当前业务的默认本人基础或无基础范围，不解释为撤销操作权限。
4. 角色以稳定 id 定位；新角色 id 必须以 `policy-` 开头。不得创建内建角色身份、修改或分配系统角色、恢复删除或停用角色。既有普通角色允许按其实际 id 管理。角色名称不作为身份。
5. bindings 维护实际 Casbin 绑定。1.0 仅支持即时、持续有效的角色集合；定时字段必须报错。user_roles 是独立留痕，不作为运行时角色绑定来源。
6. 部门、仓库和结算主体使用对应真实 ID。显式目标须由所属领域校验；人员范围仅接受消费者登记为可配置的动作。来源继承、任务授权、当前业务负责人、状态和职责分离继续执行原合同。
7. 1.0 只提供允许授权及集合替换；不得接受 deny、任意条件脚本、MongoDB 查询或未登记权限。角色权限继承、人员的非角色分组、非后台主体绑定、角色实体缺失但仍有授权记录时必须拒绝。角色实体不存在时仍须核验其全部继承与授权事实，不扁平化或静默认领原授权。
8. 未知字段、未知版本、重复对象、重叠人员资源动作、非法引用、超限输入必须整批拒绝。

## 2. 校验、预览与应用

1. validate 校验结构、引用、目标存在性和操作人授权，返回规范化文件及差异；与 preview 均为只读。
2. preview 返回 policy_version、review_hash、规范化文件和逐项 before/after 差异。角色修改须列出直接绑定的受影响人员。
3. apply 必须携带原文件、expected_policy_version、review_hash 和 idempotency_key。服务端在同一事务中重新授权、读取事实、生成差异并验证摘要；不得以客户端差异作为写入依据。
4. 文件中的角色、绑定、范围、命令回执和成功审计原子提交；批次审计以 command_id 关联原回执，授权变更只推进一次 policy 版本。提交后使用既有取消安全 RBAC 刷新机制。
5. 同一操作人、同一操作号及同一规范化请求重放返回原回执，不重写授权或推进版本。操作号异载荷必须冲突。重放仍校验当前身份、入口与原配置能力。提交结果未知不得自动重新执行。
6. 新操作号应用已达到目标的文件，不重复创建角色、绑定或范围；预览明确报告无差异。
   追加项、同组条件和目标 ID 按集合比较；仅顺序或重复项不同不得产生配置变更或推进版本。各追加项之间的分组边界和旧表达式标志必须保留。
7. 调用人必须具备对应入口权限，并满足公司组织配置资格、原有操作权限和权限授予上限。文件不能先给调用人增权再用新增资格授权后续条目。

## 3. 导出

1. export 必须显式选择 role_ids 和 user_ids；不得默认导出全公司授权。
2. 输出包含可重新预览的 document、policy_version 和系统授权规则说明。人员绑定按真实 Casbin 事实导出；导出涉及的角色一并输出。
3. 所选人员当前有动作资格且可独立配置的范围必须完整导出；尚未保存追加项时显式输出空 grants，使后续重新应用能够清除后来新增的范围。默认本人由业务规则计算，空 grants 不新增允许记录；来源继承及任务政策不输出独立范围。已退役范围只输出说明；无法无损表达的有效旧范围或继承角色必须拒绝。
4. 导出结果重新应用前必须再次预览。内部 ID 跨环境使用前须显式映射，禁止按名称猜测对象。
5. 已保存范围对应的动作权限已撤销时，原范围继续休眠，导出必须报错要求先治理；不得为了形成可执行文件自动恢复动作权限或丢弃该范围。

## 4. 实施与验证

1. HTTP 和脚本 CLI 复用同一身份领域用例，使用当前认证身份；CLI 不直写数据库、不提供超级管理员旁路。
2. 授权回执属于身份领域独立集合，以命令 ID 唯一索引约束。启动根复用领域公开索引登记。
3. 内建岗位模板继续使用原显式生成入口；本合同的 CLI 管理自定义及既有普通角色，不提供模板生成命令。
4. 仅执行库单元测试及静态、编译门禁；真实 MongoDB 事务、并发提交、进程恢复与部署验收须单独登记。不得把替身或纯规则测试登记为真实库验收。
5. 部署前备份 roles、casbin_rules、casbin_policy_state、person_data_scopes、人员查询资格及授权回执；创建新增索引后开放入口。回滚须保留已生效撤权及回执，不能仅恢复旧授权快照。

## 5. HTTP 与权限

| 方法与路径 | 请求 | 入口权限 |
| --- | --- | --- |
| POST /admin/authorization-policies/validate | 授权文件 | authorization_policy:preview |
| POST /admin/authorization-policies/preview | 授权文件 | authorization_policy:preview |
| POST /admin/authorization-policies/apply | document、expected_policy_version、review_hash、idempotency_key | authorization_policy:apply |
| POST /admin/authorization-policies/export | role_ids、user_ids | authorization_policy:export |

1. 所有响应使用现有 ApiResponse 信封。validate 与 preview 都执行完整只读预检，不写入预览记录。
2. 校验、预览和应用要求同一有效角色具有 `authorization_policy:preview`、`authorization_policy:apply`、`admin:list`、`role:list`、`data_scope:list` 及公司范围 `org_unit:manage`。文件声明角色时追加 `role:create` 和 `role:update`；声明绑定时追加 `admin:update`；声明范围时追加 `data_scope:create`。此入口只允许当前已具备应用能力的操作人预览。
3. 导出要求同一有效角色具有 `authorization_policy:export`、`admin:list`、`role:list`、`data_scope:list` 及公司范围 `org_unit:list`。
4. 精确权限和通配权限必须匹配当前运行版本的路由或领域政策权限目录。后台校验目录与前端权限生成物使用同一构建来源；不得信任客户端提供的目录。
5. 文件最多100个角色、100个人员绑定、200项人员资源配置；角色权限最多1000项、每人角色最多100项、每项范围动作最多32项、追加项最多32项、每项条件最多16项。直接影响超过1000人的角色必须拒绝，不能裁剪影响清单。
6. 未知字段、类型及版本由 JSON 提取器返回422；业务校验失败沿用统一业务错误；授权不足返回403；版本、审核摘要或操作号冲突返回409。数据库和未知提交结果保持原稳定错误语义。
7. 绑定现有销售或采购来源角色时沿用人员查询资格的首次初始化规则；已有终止资格保持终止，解绑不自动终止资格。此行为随预览明确说明。

## 6. CLI 执行

使用 Node.js 22 或更高版本，在仓库根执行。完整示例为 [authorization-policy.json](examples/authorization-policy.json)，编辑器校验采用 [JSON Schema](authorization-policy.schema.json)。先将示例中的人员和部门占位符替换为目标环境的实际 ID。

```bash
# 通过会话环境提供当前操作人的 ERP_ACCESS_TOKEN；不得写入文件或命令参数。
export ERP_API_BASE=http://127.0.0.1:10001

# 完整只读预检。
node scripts/authorization-policy.mjs validate --file policy.json

# 保存审核文件；核对 changes 中的 before、after 和 affected_user_ids。
node scripts/authorization-policy.mjs preview --file policy.json --out plan.json

# 审核后提交原计划；operation-key 由调用方分配且同次操作保持不变。
node scripts/authorization-policy.mjs apply --plan plan.json --operation-key iam-change-001 --out result.json

# 仅导出指定人员和角色；当前绑定的普通角色一并导出。
node scripts/authorization-policy.mjs export --users USER_ID --roles ROLE_ID --out exported.json

# 导出结果可直接作为下一轮预览输入。
node scripts/authorization-policy.mjs preview --file exported.json --out restore-plan.json
```

1. 输出文件仅创建，不覆盖已有文件；权限为0600。API 已成功但文件保存失败时，完整结果输出到 stdout，退出码非零；不得将文件失败解释为服务端回滚。
2. 网络请求30秒超时，禁止重定向和自动重试。应用结果未知时保留原计划及操作号；核实服务恢复情况后仅使用原请求重试，不分配新操作号重放。
3. 清空某人全部普通角色须使用 bindings 的 replace 模式及空 role_ids；移除某个角色使用完整剩余角色列表。撤销某角色全部操作权限须显式将其 permissions 设为空数组。
4. 减少人员范围须保存剩余 grants；空 grants 保留该业务默认本人基础。完全撤销动作须从全部有效角色撤销相应权限。
5. 转换旧范围必须先核对预览展示的原表达式与新增本人基础，再显式设置 replace_legacy=true。此字段只允许当前文件声明的动作转换。
6. 输入文件最多32 MiB，用于容纳预览差异和影响人员清单；实际发送的请求体最多2 MiB。超限配置必须缩小本次对象选择。
7. HTTP 成功状态及 success=true 不能代替完整业务回执。响应缺少数据、命令号、版本或统计等必要字段时必须失败；apply 保留提交状态未知语义，禁止输出成功或分配新操作号。

## 7. 代码验收

1. 在仓库根执行 `node scripts/check-authorization-policy.mjs` 验证 CLI 传输、计划复用、凭证传递和结果文件行为；该命令只连接本进程 HTTP 替身。
2. 后端执行 `backend/AGENTS.md` 规定的库单元测试、编译及架构门禁；不得执行真实外部服务或集成测试命令。
3. 验收级别及未执行事项以 [授权配置验证记录](authorization-policy-verification.md) 为准。
