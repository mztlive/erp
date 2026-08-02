# W19 · 权限与审计

> 状态：草稿
> 页面模式：M2 高密度查询列表（权限配置与审计双视图）
> 主要路由：`/system/access-audit`
> 主要角色：系统管理员、权限管理员、安全审计人员
> 最后更新：2026-08-01

## 1. 定位与目标

### 1.1 用户目标

- 权限管理员能回答“这个人为什么能或不能看某模块、某批数据、某个字段，以及为什么不能执行某动作”。
- 系统管理员能配置角色、用户角色、数据范围和字段访问策略，提交前看清影响范围，提交后获得稳定结果编号。
- 审计人员能按操作者、动作、对象、请求追踪号和时间查询追加式审计记录，而不接触不应展示的敏感旧值、新值或密钥。

### 1.2 业务目标

- 角色、用户、团队、权限和数据范围配置化，不把具体用户或角色枚举硬编码进业务状态机。
- 将模块/动作权限、数据范围、字段权限和对象状态约束明确分层，避免用“没权限”掩盖业务状态、数据范围或功能开关问题。
- 每次授权变更可追溯、可解释、可并发校验；权限变化能让打开中的工作面清除陈旧敏感值。
- 审计只追加、不修改业务事实；敏感字段只记录发生变更及安全摘要，不记录完整旧值和新值。

### 1.3 不在本工作面完成

- 不配置销售单、采购单、库存和资金单据的业务状态机；固定状态与流转不由管理员自由编排。
- 不在 W19 修改正式业务对象来“验证权限”，也不提供绕过岗位分离、对象状态或审批条件的开关。
- 不把审计记录当作业务版本；对象版本与正式动作进入对应对象中心。
- 不显示连接密钥、卡号、卡密、完整员工手机号、完整地址、完整银行账号或其它禁止字段。
- 不提供任意数据库查询、日志全文检索或临时“以某用户身份操作”的入口。

## 2. 用户、权限与数据范围

### 2.1 管理角色

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 权限管理员 | “权限配置”视图 | 被授权组织、角色和用户；不自动获得业务数据查看权 | 维护角色权限和数据范围；用户角色时间策略、字段粒度策略分别配置后才开放对应编辑，策略缺失时仅保留紧急撤权或只读 |
| 系统管理员 | “权限配置”或“审计”视图 | 系统配置范围；业务敏感值仍按字段权限裁剪 | 查看有效权限解释、处理配置异常、停启用角色 |
| 安全审计人员 | “审计查询”视图 | 经授权的审计事件范围 | 查询、打开审计详情；服务端导出策略已配置且允许时受控导出 |
| 业务部门负责人 | 受控只读入口 | 本部门角色与人员的授权摘要 | 查看并提供线下业务范围依据；不能直接扩大权限，也不在 W19 执行复核 |
| 普通用户 | 不进入 W19 | 仅可在个人设置查看自身角色摘要（若启用） | 无配置动作 |

### 2.2 四类判断必须分开

| 层次 | 回答的问题 | 服务端事实 | 界面表达 |
| --- | --- | --- | --- |
| 模块与动作权限 | 能否进入 Wxx、能否执行某类动作 | 角色权限及授权策略 | 无模块权限隐藏入口；动作无权时不提交 |
| 数据范围 | 能看哪些客户、团队、组织和单据 | `data_scope`、负责人/协作关系、历史参与者 | 无范围与范围内无记录使用不同空态 |
| 字段权限 | 字段可见、掩码、短时揭示、编辑或导出到什么程度 | 字段访问策略 | 标签与布局保留，值按策略隐藏；导出再次裁剪 |
| 对象状态与业务条件 | 当前单据状态、主责、岗位分离是否允许动作 | 对象 `allowedActions` / `actionBlockers` | 动作保留但禁用并说明条件，不伪装成权限配置问题 |

功能尚未上线或开关关闭不属于上述四类，入口直接隐藏。客户负责人变更后的历史参与者查看权来自 `document_participant`，不得仅凭当前数据范围反推或由前端补齐。

### 2.3 W19 自身的权限边界

- 权限管理员能配置授权，不代表能查看被授权业务记录的正文或敏感字段。
- 审计人员能看到“某字段已变更”，不代表能看到该字段旧值、新值或业务附件。
- 角色、用户或范围查询由服务端先裁剪；前端不得下载全量配置后隐藏越权行。
- 权限变化后，服务端返回新的 `permissionVersion`；各工作面以该版本失效查询缓存并清除已经揭示的敏感值。
- 页面打开期间 W19 权限被收回时，立即清除配置草稿、审计详情和导出链接，切换为无权限态。
- 用户角色时间策略未配置时，只允许“立即紧急撤权”；其它用户角色分配或变更 fail-closed，不显示预约生效或到期编辑控件。
- 字段粒度策略未配置时，字段策略只读；页面不得假定 `fieldGroup` 是可写契约，也不得允许管理员输入任意字段名。
- 在线审计窗口与导出阈值完全采用服务端策略；策略缺失时只使用服务端返回的保守短窗口查询边界，所有导出禁用。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 角色权限 | 系统导航“权限与审计” | `/system/access-audit?view=roles`；页签身份为 `system:access-audit:{view}` | 浏览器后退恢复上一视图和筛选 |
| 用户授权 | 权限视图二级导航 | `view=users&q=...`，打开用户详情时写入 `subjectId` | 关闭详情恢复原行焦点 |
| 数据范围 | 角色/用户详情中的范围入口 | `view=scopes&subjectType&subjectId` | 返回原角色或用户详情 |
| 有效权限解释 | 任一角色/用户行“查看有效权限” | 在当前页签打开 detail Sheet；可复制带稳定主体 ID 的 URL | 关闭后回触发行 |
| 审计查询 | 系统导航或对象中心“审计” | `/system/access-audit?view=audit&objectType=...&objectId=...` | 返回时保留查询条件 |
| 审计事件详情 | 审计行 / 请求追踪号 | `eventId={auditEventId}` 打开只读 detail Sheet | 关闭恢复原审计行焦点 |

筛选、视图、主体和审计事件身份进入 URL；编辑 Sheet 的未保存表单不跨刷新恢复。编辑表单有脏状态时关闭必须确认。同一角色、用户或审计事件重复打开时聚焦已有上下文，不复制配置草稿。

Q1 尚未固化权限复核的 `work_item_type`、岗位分离和完成动作，因此 W01/W02 当前不把权限复核任务路由到 W19，W19 也不接收 `workItemId` 或提供领取、移动确认、完成任务入口。当前路由只支持权限对象级配置、解释和审计查询。

## 4. 页面布局

### 4.1 权限配置视图

```text
┌ PageHeader：权限与数据范围               权限配置水位 10:20
├ 二级导航：角色权限 | 用户授权 | 数据范围 | 字段策略 | 审计查询
├ PolicyBanner：时间策略 / 字段粒度策略 / 配置导出策略
├ ListToolbar：搜索 | 状态 | 组织/团队 | 权限风险 | 高级筛选 | 导出配置（按策略）
├ BusinessTableFrame
│ 主体身份 | 角色/组织 | 模块权限摘要 | 数据范围摘要 | 字段策略 | 状态 | 版本 | 操作
└ detail Sheet：有效权限解释 / 编辑配置 / 影响预览
```

### 4.2 审计查询视图

```text
┌ PageHeader：审计查询                         审计水位 10:19
├ PolicyBanner：在线查询窗口 / 审计导出阈值
├ ListToolbar：时间（服务端边界） | 操作者 | 动作 | 对象类型/编号 | 结果 | 请求追踪号
├ BusinessTableFrame
│ 时间 | 操作者 | 责任角色 | 动作 | 对象 | 结果 | 变更字段 | 请求追踪号 | 查看
└ detail Sheet
   事件身份 · 请求上下文 · 动作前后状态 · 变更字段名 · 安全摘要 · 关联对象
```

### 4.3 区域说明

| 区域 | 目的 | 主组件 | 是否固定 |
| --- | --- | --- | --- |
| 二级导航 | 在权限配置和审计之间切换，避免两个一级菜单割裂 | `Tabs` / 路由导航 | 页头下固定 |
| 策略提示 | 展示用户角色时间策略、字段粒度策略和审计查询/导出策略是否已配置，以及缺失时的安全降级 | `Alert` / `PolicyStatus` | 相关视图工具栏上方 |
| 配置列表 | 扫描主体、范围和风险摘要 | `DataTable` `BusinessTableFrame` | 身份列与操作列固定 |
| 有效权限解释 | 展开直接授权、角色继承、数据范围和字段策略来源 | `QuickPreviewSheet` `size="detail"` | 只读时半屏 |
| 配置编辑 | 修改单一主体的一类授权 | TanStack Form + 分组选择器 | Dialog/Sheet 内 |
| 影响预览 | 正式提交前冻结变化、受影响主体和风险 | `BatchImpactPreview` `BusinessDiffPanel` | 正式动作前必经 |
| 审计详情 | 查看追加式事件和请求链，不展示敏感正文 | `QuickPreviewSheet` | 只读 |
| 正式结果 | 返回配置版本、影响数量和审计事件号 | `FormalActionResult` | 提交后常驻 |

## 5. 展示内容与字段

### 5.1 权限配置

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 角色列表 | `roleCode/name/status` | 角色代码 / 名称 / 状态 | `role` 查询投影 | 代码稳定，名称可修订；状态文字 + tone | 仅返回可管理角色 |
| 角色列表 | `permissionSummary` | 模块与动作权限 | `permission` 聚合 | 按工作面和动作族汇总，不展示数据库表权限 | 有配置查看权可见 |
| 用户列表 | `userId/displayName/accountStatus` | 用户 / 账号状态 | 身份服务 + `user_role` 投影 | 不显示不必要联系方式 | 按组织管理范围裁剪 |
| 用户列表 | `activeRoles/effectiveFrom/effectiveTo` | 当前角色 / 已记录有效期间 | `user_role` | 已有事实按当前、未来、已过期分开只读展示；时间策略未配置时不据此开放预约或到期编辑 | 无权角色不返回 |
| 数据范围 | `scopeType/scopeTargets` | 范围类型 / 范围对象 | `data_scope` | 公司、组织、团队、本人负责、协作等固定策略 | 目标名称按当前范围显示 |
| 字段策略 | `policyTargetId/targetLabel/accessCapabilities` | 策略目标 / 访问能力 | 字段权限投影 + 字段粒度策略 | 隐藏、掩码、可见、短时揭示、可编辑、可导出为独立能力；目标粒度由服务端策略定义，不固定为 `fieldGroup` | 策略缺失时只读且不展示编辑入口；不展示策略实现表达式 |
| 有效解释 | `grantSources` | 权限来源 | 服务端解释查询 | 直接角色、继承角色、范围、历史参与者、显式阻塞分别列出 | 只解释当前用户有权管理的主体 |
| 有效解释 | `permissionVersion` | 权限版本 | 权限服务 | 单调稳定版本/ETag | 配置并发校验使用 |
| 阻塞 | `actionBlockers` | 当前不可操作原因 | 权限服务 | 结构化代码 + 业务文案 | 前端不自行计算 |

### 5.2 审计事件

| 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- |
| `auditEventId` | 审计事件号 | `audit_event` | 稳定身份，可复制 | 有审计查看权可见 |
| `recordedAt` | 发生时间 | 审计事件 | 服务端绝对时间 + 当前工作时区格式化 | 不以客户端时间改序 |
| `actorId/actorLabel` | 操作者 | 审计事件 + 身份快照 | 使用当时显示名快照和稳定 ID | 不显示无关账户资料 |
| `actorRole` | 责任角色 | 审计事件 | 动作发生时角色，不用当前角色覆盖 | 有权可见 |
| `actionType` | 动作 | 审计事件 | 固定代码映射业务文案 | 原始内部命令可在技术权限下查看代码 |
| `objectType/objectId/objectLabel` | 业务对象 | 审计事件 + 对象注册表 | 稳定类型、ID 和安全标题 | 打开对象时重新鉴权 |
| `requestId/traceId` | 请求追踪号 | 审计事件 | 精确复制，不当作业务单号 | 仅审计/运维权限 |
| `result` | 结果 | 审计事件 | 成功、拒绝、失败、结果未知后的最终结论 | 技术错误正文脱敏 |
| `changedFieldNames` | 变更字段 | 审计事件 | 只记录字段名和“已变更” | 不返回敏感旧值或新值 |
| `safeDigest` | 安全摘要 | 审计事件 | 必要时使用带密钥摘要或不可逆摘要引用 | 默认隐藏；不能据此离线枚举原值 |
| `sourceIp/deviceContext` | 来源上下文 | 安全审计投影 | 按安全政策裁剪、掩码 | 仅安全审计角色 |

审计列表不得把“没有记录”解释为“动作没有发生”；必须同时显示审计水位和查询覆盖期间。审计记录不可在 UI 删除或编辑。

## 6. 搜索、筛选、排序与默认视图

### 6.1 权限视图

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 配置视图 | 角色权限 | `view` | 切换视图保留各自最近筛选 |
| 搜索 | 空 | `q` | 搜角色代码/名称、用户稳定账号或显示名；服务端查询 |
| 状态 | 启用 | `status` | 可查停用和已有的未来生效配置；时间策略未配置时仅只读展示历史/存量事实，不开放预约或到期编辑 |
| 组织/团队 | 当前可管理范围 | `org/team` | 选择范围后服务端裁剪主体 |
| 风险筛选 | 全部 | `risk` | 高权限、空数据范围、即将过期、冲突策略等服务端标记 |
| 排序 | 名称升序 | `sort` | 支持最近变更、风险、名称；稳定分页 |

### 6.2 审计视图

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 时间范围 | 服务端策略默认；策略缺失时为服务端返回的保守短窗口 | `from/to` | 明确工作时区；客户端不硬编码 24 小时或最大窗口。超出在线上限仅在导出策略已配置且允许时转后台导出，否则阻断并说明策略缺失 |
| 操作者 | 全部有权主体 | `actorId` | 使用稳定用户 ID，不只按显示名 |
| 动作 | 全部 | `action` | 固定动作族多选 |
| 对象 | 全部 | `objectType/objectId` | 支持稳定单号解析为对象 ID |
| 结果 | 全部 | `result` | 区分拒绝、失败和结果不确定最终结论 |
| 请求追踪号 | 空 | `traceId` | 精确匹配，命中时仍执行数据范围裁剪 |
| 排序 | 发生时间倒序 | `sort=recordedAt:desc` | 服务端稳定游标或分页 |

1440×900 下列表至少露出 8 条 36px 数据行；身份、状态/结果和行级主动作固定。所有服务端标记可用的筛选与导出控件必须可操作；策略缺失而保留解释的禁用控件必须展示 blocker。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 新建角色 | 角色视图主动作 | `CREATE_ROLE`；代码唯一，权限集合合法 | 影响预览显示初始权限与适用范围 | 返回角色稳定 ID、配置版本和审计事件号 | 保留输入；同幂等键返回原结果 |
| 修改模块/动作权限 | 角色 detail | `UPDATE_ROLE_PERMISSIONS`；版本一致；服务端确认该动作当前允许对象级直接提交 | 必须显示新增、移除、受影响用户数和高风险提示 | 返回已确认的新权限版本和审计事件号 | 需要复核但 Q1 未固化时提交前阻塞；版本冲突打开 diff，不静默覆盖 |
| 分配/变更用户角色 | 用户 detail | `MANAGE_USER_ROLE`；主体与角色均在管理范围；用户角色时间策略已配置且服务端允许本次时点/期间 | 按策略展示允许的生效方式、影响工作面和角色组合；不得出现策略未允许的预约/到期字段 | 返回授权记录、策略版本和服务端确认的生效时间 | 时间策略缺失时以 `USER_ROLE_TIME_POLICY_MISSING` 阻断；结果不确定时查询原请求，不重复提交 |
| 立即紧急撤权 | 用户 detail 风险动作 | `MANAGE_USER_ROLE`；主体与授权记录在管理范围；撤销目的为立即止损 | 强确认将立即失效的角色、剩余权限和敏感会话缓存 | 立即撤销指定授权并返回权限版本、撤销时间和审计事件 | 不依赖用户角色时间策略；失败保持原事实并告警，结果不确定时按原幂等身份查询 |
| 修改数据范围 | 范围 detail | `MANAGE_DATA_SCOPE`；目标对象有效 | 显示扩大/缩小范围、影响对象族和用户数 | 返回范围版本；客户端收到权限版本变化 | 冲突时重新加载；已选目标不丢失 |
| 修改字段策略 | 字段策略视图 | `MANAGE_FIELD_POLICY`；字段粒度策略已配置；目标必须是服务端返回的 `policyTargetId` | 展示该策略目标的查看、揭示、编辑、导出能力变化 | 返回权限版本、字段粒度策略版本和审计事件 | 粒度策略缺失时以 `FIELD_POLICY_GRANULARITY_MISSING` 阻断并保持只读；禁止自由输入字段名或前端降级组合 |
| 停用角色 | 角色行更多 | `DISABLE_ROLE`；服务端无不可停用 blocker，且当前安全策略允许对象级直接提交 | 强确认影响用户、任务责任池和替代角色 | 角色停用结果常驻；不删除历史角色身份 | 需要复核但 Q1 未固化时保持启用并显示策略阻塞；其它 blocker 列出解决入口 |
| 查看有效权限 | 主体行 | 有主体管理/审计查看权 | 无 | 返回分层解释与计算水位 | 查询失败保留列表，允许重试 |
| 导出权限配置 | 工具栏 | 当前仍有配置导出和字段权限；服务端配置导出策略已配置且允许当前范围 | 显示服务端冻结的筛选、字段清单、遮罩规则、目标数和下载有效期 | 创建后台导出任务并返回服务端下载策略 | 策略缺失时禁用；下载时再鉴权，结果不确定查询任务 |
| 导出审计 | 审计工具栏 | `EXPORT_AUDIT`；服务端审计导出策略已配置，期间/数量在策略允许范围 | 显示期间、目标数、字段、用途和策略要求 | 后台任务号 + 下载审计 | 策略缺失时禁用；超阈值按服务端 blocker 处理，不在浏览器拆分或拼全量 |
| 打开关联对象 | 审计 detail | 当前有目标对象查看权 | 无 | 新开/聚焦对应 Wxx 对象页签 | 无权时只保留对象类型和事件，不泄露正文 |

任何配置变更都必须由服务端返回 `allowedActions`、`actionBlockers` 和复核要求。Q1 决策前，W19 只提交服务端明确允许直接生效的对象级动作；凡命中待固化复核条件的动作均以 `REVIEW_POLICY_UNCONFIGURED` 阻塞，不创建临时任务、不允许页面内“代确认”。Q2 决策并配置时间策略前，用户角色写动作只开放立即紧急撤权；Q3 决策并配置粒度策略前，字段策略只读；Q4 策略缺失时仅按服务端保守短窗口查询且不允许任何导出。若未来在正式模型中注册固定 `work_item_type`，相应处理器必须复用 W02 的 `CompleteWorkItemEnvelope`，业务决定与任务完成在同一事务提交；W19 不得另造任务状态、租约或完成接口。

## 8. 数据契约

### 8.1 查询

```ts
type AccessSubjectQuery = {
  view: "ROLES" | "USERS" | "SCOPES" | "FIELD_POLICIES"
  q?: string
  status?: string[]
  organizationId?: string
  teamId?: string
  riskFlags?: string[]
  page: number
  pageSize: number
  sort: string
}

type AccessGovernancePolicyView = {
  userRoleTimePolicy:
    | {
        state: "MISSING"
        allowedActions: ["EMERGENCY_REVOKE_USER_ROLE"]
        blockerCode: "USER_ROLE_TIME_POLICY_MISSING"
      }
    | {
        state: "CONFIGURED"
        policyVersion: string
        schedulingAllowed: boolean
        expirationAllowed: boolean
      }
  fieldPolicyGranularity:
    | {
        state: "MISSING"
        editable: false
        blockerCode: "FIELD_POLICY_GRANULARITY_MISSING"
      }
    | {
        state: "CONFIGURED"
        policyVersion: string
        editableTargets: Array<{ policyTargetId: string; label: string }>
      }
  auditAccessPolicy:
    | {
        state: "MISSING"
        fallbackFrom: string
        fallbackTo: string
        configurationExportAllowed: false
        auditExportAllowed: false
        blockerCode: "AUDIT_ACCESS_POLICY_MISSING"
      }
    | {
        state: "CONFIGURED"
        policyVersion: string
        defaultFrom: string
        defaultTo: string
        maxOnlineWindowSeconds: number
        configurationExportThreshold: { maxRows?: number }
        auditExportThreshold: { maxWindowSeconds?: number; maxRows?: number }
      }
}

type EffectiveAccessView = {
  subject: { type: "ROLE" | "USER"; id: string; label: string }
  moduleAndActionGrants: AccessGrantView[]
  dataScopes: DataScopeView[]
  fieldPolicies: FieldPolicyView[]
  historicalParticipantRules: AccessExplanationView[]
  deniedOrBlocked: AccessExplanationView[]
  permissionVersion: string
  calculatedAt: string
  governancePolicies: AccessGovernancePolicyView
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
}

type AuditQuery = {
  from: string
  to: string
  actorId?: string
  actions?: string[]
  objectType?: string
  objectId?: string
  result?: string[]
  traceId?: string
  sort: string
  cursor?: string
  pageSize: number
}
```

- 权限列表、有效解释、审计列表和详情均由 TanStack Query 管理。
- Query Key 至少包含当前用户、当前管理角色、`permissionVersion`、各治理策略版本/缺失态、视图、主体、筛选和审计水位。
- 有效权限解释由服务端计算；响应明确“来源”和“阻塞”，前端不合并集合后自行得出结论。
- 审计查询返回覆盖水位、下一游标和字段可见性；不得返回未授权事件后再在浏览器过滤。`from/to` 的默认值、最大在线窗口和导出阈值均来自 `auditAccessPolicy`，前端不得硬编码 24 小时。
- `auditAccessPolicy.state=MISSING` 时，`fallbackFrom/fallbackTo` 是服务端为本次查询签发的保守短窗口，前端只能原样采用，且权限配置与审计两类导出均为 false；不得自行扩大窗口或推测阈值。

### 8.2 提交

```ts
type AccessChangeCommandBase = {
  subjectId: string
  expectedPermissionVersion: string
  reasonCode: string
  comment?: string
  idempotencyKey: string
}

type GeneralAccessChangeCommand = AccessChangeCommandBase & {
  subjectType: "ROLE" | "DATA_SCOPE"
  action: string
  changeSet: Array<{
    targetReference: string
    operation: "ADD" | "REMOVE" | "REPLACE"
    valueReference?: string
  }>
}

type EmergencyRevokeUserRoleCommand = AccessChangeCommandBase & {
  subjectType: "USER"
  action: "EMERGENCY_REVOKE_USER_ROLE"
  roleAssignmentId: string
  timePolicyVersion?: never
  effectiveAt?: never
  expiresAt?: never
}

type GovernedUserRoleChangeCommand = AccessChangeCommandBase & {
  subjectType: "USER"
  action: "ASSIGN_USER_ROLE" | "CHANGE_USER_ROLE" | "REVOKE_USER_ROLE"
  roleId: string
  roleAssignmentId?: string
  timePolicyVersion: string
  effectiveAt?: string
  expiresAt?: string
}

type GovernedFieldPolicyChangeCommand = AccessChangeCommandBase & {
  subjectType: "FIELD_POLICY"
  action: "UPDATE_FIELD_POLICY"
  granularityPolicyVersion: string
  policyTargetId: string
  accessCapabilities: Array<
    "HIDDEN" | "MASKED" | "VISIBLE" | "TEMPORARY_REVEAL" | "EDITABLE" | "EXPORTABLE"
  >
}

type AccessChangeCommand =
  | GeneralAccessChangeCommand
  | EmergencyRevokeUserRoleCommand
  | GovernedUserRoleChangeCommand
  | GovernedFieldPolicyChangeCommand
```

- 表单统一使用 TanStack Form；服务器再次校验主体、目标、组合约束、岗位分离和管理范围。
- `EmergencyRevokeUserRoleCommand` 是时间策略缺失时唯一允许的用户角色写分支，只能立即撤销既有授权，且通过 `never` 禁止预约/到期字段。其它用户角色分配或变更必须携带当前 `timePolicyVersion`；策略缺失、版本变化或提交了策略不允许的时点字段时整体拒绝。
- 字段策略写入只能使用 `GovernedFieldPolicyChangeCommand` 并引用服务端返回的 `policyTargetId` 与当前 `granularityPolicyVersion`；粒度策略缺失时不构造命令，也不得把 `fieldGroup` 或任意字段路径当作可写契约。
- 当前对象级提交只返回 `CONFIRMED | REJECTED | UNKNOWN`、新权限版本、审计事件号和影响摘要；需要复核但策略尚未固化的动作在提交前以结构化 blocker 拒绝，不返回伪造的“待复核”。
- `UNKNOWN` 时不更新本地有效权限，不关闭结果区；按幂等请求查询最终结果。
- 配置版本冲突时展示服务端差异，用户基于最新版本重新提交；禁止把旧选择静默覆盖到新配置。
- 变更原因必填并记录审计，但原因文本不得包含密钥或敏感业务正文。
- `AccessChangeCommand` 是权限对象命令，不携带 `workItemId`、`claimToken`、`leaseVersion` 或页面自定义完成动作。用户角色时间策略与字段粒度策略字段只用于服务端并发校验，不能由前端补默认值。未来启用固定复核任务时，任务决定另由已注册处理器包入 W02 `CompleteWorkItemEnvelope`，不得向本命令追加私有任务字段。

### 8.3 前端边界

- 前端只格式化角色、权限、范围、字段策略和审计状态文案，不计算最终授权集合。
- “受影响用户数”“可见对象数量”“风险等级”由服务端预览返回；前端不能从当前页求和。
- 前端不得通过禁用按钮替代后端鉴权，也不得缓存完整敏感值以便权限恢复后重显。
- 审计字段差异只展示服务端允许的字段名、安全摘要和业务文案，不拼接请求/响应原文。
- 审计、权限和历史参与者记录均不可在浏览器删除或改写。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 二级导航、工具栏和表格同尺寸 Skeleton | 应用壳导航可用 | 查询完成原位替换 |
| 刷新 | 保留旧行，显示权限/审计水位 | 只读详情可用；写动作提交时重验版本 | 成功更新时间；失败保留缓存 |
| 无配置记录 | 明确“尚未配置角色/范围” | 有权时创建配置 | 创建后恢复 |
| 筛选无结果 | 显示筛选摘要 | 清除筛选 | 回全部有权记录 |
| 无数据范围 | 不显示 0 条配置/事件 | 查看管理范围或申请权限 | 权限更新后重查 |
| 字段级隐藏 | 标签和列保留，值掩码；若服务端允许导出则同步裁剪 | 其余有权动作 | 权限更新后重新查询 |
| 查询失败 | 有缓存保留并标记失败；无缓存失败态 | 重试 | 查询成功 |
| 数据陈旧 | 显示权限版本或审计水位已过期 | 刷新 | 获取最新版本 |
| 用户角色时间策略未配置 | 用户授权视图展示 `USER_ROLE_TIME_POLICY_MISSING`；已有有效期事实只读，隐藏预约/到期编辑 | 仅查看影响、执行立即紧急撤权 | 策略正式配置后重新查询，按服务端允许的生效方式编辑 |
| 字段粒度策略未配置 | 字段策略列表只读并展示 `FIELD_POLICY_GRANULARITY_MISSING`；不把字段组或字段名呈现为可写目标 | 查看有效字段权限和解释 | 策略正式配置且返回 `policyTargetId` 后重新查询 |
| 审计访问策略未配置 | 时间选择器锁定在服务端返回的保守短窗口；权限配置与审计导出均禁用并解释策略缺失 | 在该短窗口内查询、查看有权详情 | 服务端策略配置后重新查询窗口与导出能力 |
| 保存中 | 锁定当前正式动作，列表不乐观改权 | 取消未发出的编辑 | 返回结果后解锁 |
| 保存失败 | 保留选择和原因，主动作旁显示失败 | 重试或放弃 | 同幂等键重试 |
| 版本冲突 | `ConflictResolutionDialog` 展示配置 diff | 重新加载并审阅 | 基于最新版本提交 |
| 复核策略未固化 | 高风险动作禁用，固定显示 `REVIEW_POLICY_UNCONFIGURED` 和 Q1 所缺决策 | 查看对象与影响预览、返回列表；不能代确认 | 正式注册固定任务类型和处理规则后重新查询能力 |
| 正式动作成功 | `FormalActionResult` 固定展示配置/授权 ID、新权限版本、生效时间、审计事件和下一步 | 查看有效权限、返回列表 | 客户端按新权限版本重查 |
| 正式结果不确定 | 不改变有效权限，显示查询入口 | 查询最终结果、联系支持 | 得到最终结论 |
| 后台导出 | 仅在对应服务端导出策略已配置并允许当前范围时，`BackgroundJobProgress` 区分权限配置导出与审计导出，展示范围、遮罩、任务号和进度 | 查看任务、取消未开始项 | 完成后按策略下载；失败按原快照重试 |
| 权限收回 | 清除配置草稿、审计详情和下载链接 | 返回有权模块 | 权限恢复后重查 |

## 10. 响应式、键盘与无障碍

| 视口 | 布局变化 | 保留内容 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 侧栏展开；表格 + 768px detail；首屏至少 8 行 | 主体、权限/范围摘要、状态、版本、操作 | 无 |
| 1280×800 | 侧栏可折叠；detail 覆盖部分表格 | 主体、状态、风险和主动作 | 次要摘要列移入列设置 |
| 1024×768 | 侧栏图标模式；筛选换行；detail 覆盖式 | 权限层次、阻塞原因、审计结果 | 组织说明和审计上下文折叠 |
| 768×1024 | 导航抽屉；表格横向滚动，主体/操作固定；detail 上下分区 | 只读解释、审计查询、简单启停用确认 | 影响预览改分段卡片 |
| 375×812 | 仅保证自身/指定主体摘要、审计事件阅读和服务端明确允许的低风险对象级确认 | 主体、结论、变更字段名、结果；时间策略缺失时用户角色仅可立即紧急撤权 | 不做角色矩阵编辑、数据范围批量选择、字段策略编辑、复核任务或审计导出，提示转桌面 |

- Tab 顺序：二级导航 → 搜索/筛选 → 表格 → 行操作 → detail 子区 → 正式动作。
- 权限矩阵使用表头与行头关联；不能只靠勾选颜色表达允许/拒绝。
- 筛选和有效权限查询结果数量使用 `aria-live=polite`；保存结果使用固定结果区播报。
- Sheet/Dialog 关闭后焦点回触发行；版本冲突关闭后焦点回到变化摘要。
- 所有选择器和复选框有可读权限名称、范围和状态；触控目标不小于 44×44。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 任一对象中心审计入口 | W03–W14、W17–W30 | 对象类型、稳定 ID、时间范围 | 关闭审计详情回原对象页签 |
| 导入审计 | W18 | 批次 ID、后台任务 ID、下载事件 | 返回聚焦原批次 |
| 供应商连接审计 | W20 | 连接 ID、配置版本、健康检查请求 ID | 返回连接中心对应子区 |
| 供应商商品与供给审计 | W21 | 供应商商品、映射或供给版本 ID | 返回原映射队列上下文 |
| 接口错误与运维追踪 | W29 | 请求追踪号、集成尝试或错误任务 ID | 返回保留 W19 查询条件 |

跨工作面只传稳定身份与查询上下文。W19 重新查询当前授权，不能接受来源页传来的“可见”“可编辑”布尔值作为权限事实。

W01/W02 当前不直接打开 W19 执行权限复核或移动确认。若 Q1 后续确认并在正式模型中注册固定 `work_item_type`，再由统一任务注册表声明目标处理器；处理器必须遵守 W02 `CompleteWorkItemEnvelope`，不能以 W19 路由参数或 `AccessChangeCommand` 模拟任务完成。

## 12. 验收清单

### 12.1 权限模型

- [x] 页面明确分开模块/动作权限、数据范围、字段权限和对象状态 blocker。
- [x] 无模块权限、无数据范围、范围内无记录和字段掩码四种状态不会混淆。
- [x] 有效权限解释能指出授权或阻塞来源，不由前端合并权限集合。
- [ ] 历史参与者查看权按正式关系解释，不因当前负责人变化被静默抹去。
- [ ] 权限管理员不会因为能配置权限而自动看到业务敏感正文。
- [x] 用户角色时间、字段粒度和审计访问/导出策略均展示服务端配置态；缺失时分别执行规定的 fail-closed 行为，不由前端猜默认值。

### 12.2 配置动作

- [x] 所有授权变更提交前展示变化、影响主体和服务端风险摘要。
- [ ] 变更使用权限版本与幂等键；冲突不静默覆盖，结果未知不乐观生效。
- [ ] 角色停用前展示任务责任池和替代角色 blocker，不删除历史身份。
- [ ] 权限变化使其它工作面缓存按新版本失效，已揭示敏感值立即清除。
- [x] 正式结果包含配置版本、影响数量、审计事件号和下一步。
- [x] Q1 决策前，命中复核要求的动作失败关闭，W19 不创建、领取、移动确认或完成 `work_item`。
- [ ] `AccessChangeCommand` 仅是对象级命令；未来固定复核任务只能复用 W02 `CompleteWorkItemEnvelope`。
- [x] 用户角色时间策略未配置时，只有 `EmergencyRevokeUserRoleCommand` 可提交且立即生效；其它分配/变更阻断，页面没有预约/到期编辑控件。
- [x] 字段粒度策略未配置时字段策略只读；配置后只可提交服务端 `policyTargetId` 与策略版本，不以 `fieldGroup` 或任意字段路径充当写契约。

### 12.3 审计与安全

- [x] 审计可按操作者、角色、动作、对象、结果、请求追踪号和时间查询。
- [x] 敏感字段只显示字段名和“已变更”，不显示完整旧值或新值。
- [ ] 审计记录不可编辑或删除；对象钻取和导出均重新鉴权。
- [ ] 导出使用服务端冻结范围、字段清单和遮罩规则，短时下载并记录审计。
- [ ] 在线查询默认/最大窗口和导出阈值均来自服务端；策略缺失时只允许服务端保守短窗口查询且全部导出禁用，不硬编码 24 小时。
- [ ] 页面、审计详情、导出和错误信息均不出现密钥、卡密或禁止敏感值。

### 12.4 体验与响应式

- [x] 1440×900 首屏至少 8 行，主体身份和操作列固定。
- [x] URL 可恢复视图、筛选、主体和审计事件；返回恢复原行焦点。
- [ ] §9 全部状态与 1440/1280/1024/768/375 五档视口完成验证。
- [ ] 键盘可完成查询、打开解释、修改单一配置、查看影响和提交。
- [x] 页面文案不出现数据库表授权、前端路由守卫等实现术语。

## 13. 待确认事项

| ID | 问题 | 影响 | 建议决策人 | 当前建议 |
| --- | --- | --- | --- | --- |
| Q1 | 哪些高风险权限变更必须双人复核、复核人如何岗位分离，并注册哪个固定 `work_item_type` 与唯一完成动作？ | W01/W02 路由、处理器注册、动作状态和结果；决策前命中复核要求的动作失败关闭 | 安全负责人 + 内控 | 候选范围为扩大敏感字段、全公司数据范围和权限管理能力；确认后必须进入正式任务模型并复用 W02 `CompleteWorkItemEnvelope`，普通缩权才可按服务端策略直接生效 |
| Q2 | 权限变更是立即生效还是允许预约生效/到期？ | 表单字段、缓存失效和审计 | 安全负责人 + 人事/组织负责人 | 时间策略正式配置前，仅允许立即紧急撤权并强制失效敏感会话缓存；其它用户角色分配/变更 fail-closed，不开放预约或到期编辑 |
| Q3 | 字段权限采用字段级还是业务字段组级配置？ | 配置复杂度和可解释性 | 产品 + 安全 + 各领域负责人 | 粒度策略正式配置前字段策略只读；配置后由服务端下发稳定 `policyTargetId`，不预设 `fieldGroup` 为可写契约，也禁止任意字段名配置 |
| Q4 | 在线审计最大查询期间和导出审批阈值是多少？ | 查询性能、后台导出和复核 | 安全 + 运维 | 默认/最大在线窗口与导出阈值均由服务端策略返回；策略缺失时仅按服务端保守短窗口查询并禁用导出，不硬编码 24 小时 |
| Q5 | 普通用户是否提供“我的有效权限”只读页？ | 入口与支持成本 | 产品 + 安全 | 提供简化摘要和申请入口，不展示内部策略表达式或他人权限 |

确认后的安全策略必须写入 §2、§7 和 §8；不得仅保留在待确认表中由前端自行判断。

## 14. 业务依据

- `erp-phase-1.md` §11、§11.1：部门职责、销售/采购/运营/仓储/财务/管理员范围与配置化权限原则。
- `erp-phase-2.md` §16：第二期新增角色职责、员工信息边界、连接密钥和人工操作审计约束。
- `erp-data-model.md` §4.5–§4.6：追加式审计、敏感字段处理、固定状态机、配置化角色与数据范围。
- `erp-data-model.md` §5.1、§6.1：`role`、`permission`、`user_role`、`data_scope`、`audit_event`、`document_participant` 和正式动作审计。
- `erp-ui-design.md` §3.3–§3.5、§4.3、§11：权限四种表达、TaskTabs、响应式、M2 和通用状态契约。
- `erp-ui-flows.md` §1.1、§8–§12：对象中心、任务队列和分析下钻到审计时保留原工作上下文。
