# W29 · 接口错误与对账中心

> 状态：草稿
> 页面模式：M7 治理与导入（内含连续处理区）
> 主要路由：`/governance/integration-errors`、`/governance/integration-errors/errors/:taskId`、`/governance/integration-errors/differences/:differenceId`
> 主要角色：系统管理员、研发运维；采购、运营、客服、财务按责任处理业务差异
> 最后更新：2026-08-01

## 1. 定位与目标

### 1.1 用户目标

- 管理员和运维在一个工作面看见发送失败、接收失败、结果未知、回调异常和周期对账差异，并按错误类别获得唯一正确的下一步。
- 结果未知时先查询原请求结果；只有明确无结果且服务端确认安全时才能重放，重放始终沿用原幂等键。
- 业务责任人能在同一详情查看左右证据、修复映射或形成补偿，不需要到日志平台猜业务状态。
- 处理完成后能看到可验证终态或正式纠错/补偿证据，而不是用通用“关闭”掩盖未闭环资金和履约。

### 1.2 业务目标

- 统一承载 `integration_error_task` 和 `reconciliation_difference` 的处理入口，但保持消息错误与业务差异的身份、证据和处理链独立。
- 落实消息幂等与业务事实幂等两层治理，重复投递、回填重叠和人工重放都不产生重复正式事实。
- 对账只产生差异和任务；任何修复通过对应业务变更、纠错、重新归集或原消息重放入口完成。
- 所有人工查询、重放、补偿、确认和关闭动作追加审计，不覆盖原消息、尝试、差异或正式事实。

### 1.3 不在本工作面完成

- 不直接编辑销售单、商城消费、供应商订单、成本、应收、应付、退款或余额事实。
- 不展示或编辑连接密钥、签名密钥、完整请求/响应、完整手机号和地址。
- 不用“标记成功”替代真实供应商/商城终态；不允许把结果未知手工改成已完成。
- 不在错误中心重新实现商品映射、订单售后、结算和回填业务；通过稳定上下文进入 W21、W26、W27、W30。
- 不允许对账任务自动覆盖任一侧正式事实。

## 2. 用户、权限与数据范围

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 系统管理员 | 我的接口任务 / 结果未知 | 授权系统、商城、连接和环境 | 领取、查询原结果、按规则重放、转交、关联终态证据 |
| 研发运维 | 技术故障 | 授权连接和环境 | 排查鉴权/签名/限流/临时故障，查看脱敏尝试摘要 |
| 采购 | 供应商与商品差异 | 负责供应商、外部商品和订单 | 修复业务映射、确认供应商业务拒绝、提供人工证据 |
| 运营 | 商品发布 / 销售投影 / 商城事实差异 | 负责商城和类目范围 | 处理发布、投影和消费归集差异 |
| 客服 | 售后闭环差异 | 服务范围内订单 | 协同取消、退款和余额恢复，不执行技术重放 |
| 财务 | 成本、结算、退款和余额对账 | 财务授权供应商、客户和期间 | 确认财务差异、进入正式纠错、验证闭环 |

权限规则：

- 无 W29 模块权限时不展示治理菜单；业务角色仍可从其对象中心看到指派给自己的脱敏差异任务。
- 服务端按系统、环境、商城、供应商、业务对象和责任角色限制数据范围。
- 技术摘要、业务字段、成本金额和敏感履约信息使用独立字段权限；列表永不展示原始载荷。
- 任务领取使用租约；只有领取人和有效租约可执行正式处理，查看权限不等于处理权限。
- 安全类故障（鉴权、签名）只向授权运维展示必要摘要，立即停止自动重试并告警。
- 权限变化后清除脱敏前临时揭示内容、下载链接和本地缓存；TaskTab 保留对象身份但内容切无权限态。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 处理错误队列 | 侧栏“接口错误与对账” | `/governance/integration-errors?view=mine` | 恢复视图、筛选、位置 |
| 从待办进入 | W01 / W02 中 `destinationWorkspaceId=W29` 的 `INTEGRATION_RESULT_UNKNOWN` / `BUSINESS_EXCEPTION` | 仅当注册 handler 的业务对象为 `integration_error_task` / `reconciliation_difference` 时，先打开 `/governance/integration-errors?resolveWorkItemId=:workItemId&queueContextId=...`；服务端解析后以 replace 导向领域详情，URL 不携带 `claimToken` | 完成/暂挂后回原队列 |
| 从业务对象进入 | W17/W20–W27 | 带 `objectType/objectId`及稳定错误任务或对账差异 ID；目标页按对象类型选择明确详情路由 | 返回原对象异常子区 |
| 打开错误任务详情 | 错误列表、全局搜索或待办解析结果 | `/governance/integration-errors/errors/:taskId`；`:taskId` 固定为 `integration_error_task` 稳定 ID，相同错误任务聚焦 `integration-error:{taskId}` | 返回原队列位置 |
| 打开对账差异详情 | 对账列表、全局搜索或业务对象 | `/governance/integration-errors/differences/:differenceId`；相同差异聚焦 `reconciliation-difference:{differenceId}` | 返回原对账筛选和行位置 |
| 连续处理 | 错误列表主操作 | 打开 M3 任务页签，URL 保存筛选、当前位置、任务 ID | 完成后自动下一项 |
| 深入业务修复 | 处理面“去修复” | 新开/聚焦目标 Wxx，W29 队列页签保留 | 修复后返回当前任务并重验 |
| 查看对账批次 | 对账视图 | `mode=reconciliation&jobId={id}` | 返回差异队列筛选 |

连续队列 TaskTab 身份为 `integration-resolution-queue:{queueContextId}`；单个差异对象页签身份为 `integration-error:{taskId}` 或 `reconciliation-difference:{differenceId}`。`:taskId` 与 `workItemId` 是不同身份：前者定位 `integration_error_task`，后者只定位统一待办；解析入口必须由服务端同时校验 handler 的 `destinationWorkspaceId=W29`、业务对象类型和二者关系，再 replace 到领域详情，前端不得把两个 ID 互换。其它 `BUSINESS_EXCEPTION`（例如 W21 的 `STOPPED`）严格按注册 handler 进入自己的工作面，W29 不按任务类型抢路由。刷新恢复队列筛选、当前项和自动下一项偏好；租约身份从服务端重取，`claimToken` 只留在当前会话内存。重复打开同一任务聚焦已有页签。未提交处理说明为脏状态；关闭按全局契约确认。

## 4. 页面布局

### 4.1 桌面布局

```text
┌ PageHeader：接口错误与对账中心              [数据水位] [运行监控]
├ MetricStrip：结果未知 | 待人工 | 安全故障 | 未解决差异 | 最长滞留
├ SavedView / FilterBar：我的任务 | 结果未知 | 映射 | 对账差异 | 已解决
├─────────────────────────────┬──────────────────────────────────────────┐
│ 任务/差异队列 38%           │ 当前处理详情 62%                         │
│ 类型、对象、错误、滞留、责任│ SequentialProcessBar                    │
│ [处理]                       │ 业务影响 + 左右证据 + 原动作/尝试历史    │
│                              │ InterfaceErrorResolutionPanel           │
│                              │ [查询原结果] [去修复] [重放] [转人工]   │
└─────────────────────────────┴──────────────────────────────────────────┘
```

窄列表模式可先以 M2 表格浏览；打开“处理”后进入上述 M3 双栏，不再从普通详情另跳处理页。

### 4.2 区域说明

| 区域 | 目的 | 主组件 | 是否固定 |
| --- | --- | --- | --- |
| 指标与视图 | 判断异常水位和快速聚焦 | `MetricStrip` `SavedViewPicker` | 顶部固定 |
| 任务队列 | 连续选择和扫读 | `WorkTaskItem` / `DataTable` | 左栏独立滚动 |
| 连续处理条 | 显示位置、租约和上一/下一项 | `SequentialProcessBar` | 详情顶部固定 |
| 影响摘要 | 用业务语言解释影响对象和风险 | `Alert` `DocumentSummary` | 当前项顶部 |
| 证据区 | 展示消息、尝试、左右事实和时间水位 | `BusinessDiffPanel` `AuditTimeline` | 独立滚动 |
| 处理区 | 按错误类别只开放安全动作 | `InterfaceErrorResolutionPanel` `GuardedBusinessAction` | 详情底部固定 |
| 正式结果 | 固定展示查询、重放、补偿或解决证据 | `FormalActionResult` | 动作后保持可见 |

## 5. 展示内容与字段

### 5.1 列表和任务身份

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 身份 | `taskNo` / `differenceNo` | 异常任务 / 差异编号 | 错误任务或差异处理投影 | 稳定编号 | 有任务权限可见 |
| 类型 | `errorClass` / `reconciliationType` | 结果未知、映射错误、鉴权失败等 | 固定错误/对账枚举 | 固定业务文案 | 技术细分类按权限 |
| 对象 | `objectType/objectId/title` | 影响对象 | 稳定业务对象引用 | 标题为已授权投影 | 无对象权限时只给脱敏类型 |
| 方向 | `source/target` | 商城 → ERP / ERP → 供应商等 | 消息/对账类型 | 环境和系统明确 | 生产/验证环境不得只靠颜色 |
| 状态 | `status` | 待处理 / 自动重试中 / 待人工 / 已解决 / 已关闭 | 错误任务或处理链 | 与原消息状态分开 | 按角色 |
| 滞留 | `createdAt/dueAt` | 发生 / 已滞留 / SLA | 任务时间 | 服务端计算风险级别 | 全员有权任务可见 |
| 责任 | `ownerRole/user` / `lease` | 责任角色 / 领取人 | `work_item` / 错误任务 | 租约剩余时间 | 领取人可处理 |
| 重试 | `attemptCount/nextAttemptAt` | 尝试次数 / 下次重试 | `integration_attempt` | 不等于业务完成次数 | 运维可见 |

### 5.2 错误类别与处理提示

| 错误类别 | 页面必须展示 | 默认动作 | 明确禁止 |
| --- | --- | --- | --- |
| 能力不足 | 缺失能力、受影响商品/售后、人工替代路径 | 转业务人工处理 | 重复调用不支持接口 |
| 参数 / 映射错误 | 字段/映射差异、责任工作面 | 去 W21/W17 修复后重新归集/原消息重放 | 自动重试相同错误参数 |
| 供应商业务拒绝 | 拒绝代码、业务原因、商城支付已发生提示 | 进入 W26 售后/补偿 | 把拒绝当临时故障重试 |
| 临时故障 | 最近尝试、退避计划、服务水位 | 等待自动重试或授权提前重试 | 生成新幂等键 |
| 结果未知 | 原动作、幂等键摘要、查询能力、最后尝试 | **查询原结果** | 直接重放下单/取消/退款 |
| 鉴权 / 签名失败 | 环境、连接、发生时间、告警状态 | 停止重试、转运维修复 | 展示密钥或继续自动重试 |
| 限流 | 限流窗口、退避时间 | 按策略等待/重试 | 高频人工点击重试 |
| 回调重复 | 已处理的消息/事实证据 | 确认重复并关联原结果 | 创建第二份业务事实 |
| 回调乱序 | 当前正式状态、回调版本/时间 | 保留证据、拒绝状态倒退 | 用晚到旧回调覆盖终态 |
| 对账差异 | 左右事实、边界、水位、差异摘要 | 进入对应纠错/补偿/确认无误 | 对账任务直接改数 |

### 5.3 证据与审计

详情必须包含：

- 原消息事件 ID 摘要、对外幂等键摘要、业务事实键摘要及 Schema 版本；完整值按安全策略显示或复制，不进入列表。
- 来源发送、ERP 接收、每次尝试和业务发生时间；区分发生时间与记录时间。
- 脱敏请求/响应摘要、HTTP/协议分类、外部请求号；不得展示密钥和完整敏感原文。
- 对账左右不可变证据引用、数据边界、水位、差异类型和摘要。
- 所有领取、查询、重放、转交、补偿、解决或关闭动作的追加式记录。
- 替代任务、正式纠错单、供应商订单、退款、应付或其他可验证终态的稳定引用。

## 6. 搜索、筛选、排序与默认视图

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| Saved View | `mine` | `view=mine` | 我的待处理；可切结果未知、安全故障、自动重试、对账差异、已解决 |
| 模式 | 错误 + 差异统一待办 | `mode=all/errors/reconciliation` | 身份和操作仍按各自类型表达 |
| 系统方向 | 全部有权 | `direction=` | 商城→ERP、ERP→商城、ERP→供应商、供应商→ERP |
| 环境 | 生产 | `environment=` | 生产/验证有文字标签和独立权限 |
| 错误类别 | 全部待处理 | `errorClass=` | 固定枚举多选 |
| 业务对象 | 全部 | `objectType=` / `objectId=` | 可由其他 Wxx 预置 |
| 责任人 | 我 / 角色池 | `owner=` | 区分已领取、角色池和他人领取 |
| 滞留 | 全部 | `age=` | 超 SLA、24h、7d 等服务端口径 |
| 搜索 | 空 | `q=` | 精确匹配任务号、业务单号、事件 ID、外部请求号 |
| 排序 | 安全故障和结果未知优先，其次 SLA | `sort=` | 服务端稳定排序；创建时间和 ID 为尾键 |

队列处理时筛选摘要、当前项 ID、自动下一项偏好和位置写入 URL/可恢复状态。指标点击采用按钮语义并与筛选同步。刷新保留旧队列，不能因查询失败把已领取项从屏幕清空。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 领取任务 | 当前项 / 队列 | 责任角色匹配、无有效他人租约 | 无 | 返回租约令牌摘要和有效期 | 被他人领取时转只读并显示领取人 |
| 查询原结果 | 结果未知主动作 | 有查询权限/能力、原动作可定位 | 无破坏性确认 | 用 `WorkItemActionEnvelope` 保存查询证据；无论得到终态、明确无结果或仍未知，任务仍为正式 `PENDING/IN_PROGRESS` | 网络失败保留当前项和原状态，可用本次任务动作幂等键重试 |
| 重放原动作 | 查询结果后的次动作 | **仅明确无结果且服务端判定安全**；租约有效 | 展示对象、原动作、影响和服务端锁定的原键摘要 | 用任务动作信封发起；服务端自行取原记录的 `originalActionIdempotencyKey` 重放并保存结果，任务仍为 `PENDING/IN_PROGRESS` | 再次未知时保持非终结状态并回到查询原结果；客户端不能传入或替换原键 |
| 修复后重新归集 | 映射/归集错误 | 目标 Wxx 已形成正式修复，当前事实仍待归集 | 展示原事实和修复证据 | 用任务动作信封和原业务事实键重新归集；保存证据但任务仍为 `PENDING/IN_PROGRESS` | 失败保留原事实和最新错误 |
| 转人工 / 转交 | 能力不足或责任错误 | 有转交权限、目标责任明确 | 原因与业务影响必填 | 用 `TransferWorkItemEnvelope` 原子转交原任务、失效原租约并创建 `UNCLAIMED/PENDING` 后继任务 | 失败保留原领取和输入 |
| 关联正式补偿 | 处理区 | 对应业务允许且补偿对象已创建或本动作可幂等创建 | `FormalActionConfirmDialog` 展示影响 | 用任务动作信封创建/关联纠错、退款、冲正或补偿对象证据；当前任务仍为 `PENDING/IN_PROGRESS` | 结果未知查询本次任务动作，不重复创建 |
| 补充终态证据 | 处理区 | 有证据追加权；证据类型属于服务端白名单 | 无正式终结确认 | 用 `WorkItemActionEnvelope` 追加强类型受控证据，任务保持 `PENDING/IN_PROGRESS` | 证据无效时保留当前任务并说明缺失项 |
| 标记已解决 | 处理区 | 当前错误类型与资金影响已命中有效证据策略，强类型证据齐全，岗位分离通过，且已取得可验证终态或已完成补偿并对账通过 | 展示策略版本、证据清单、复核人与终态 | 用 `CompleteWorkItemEnvelope` 追加“已解决”记录并完成任务 | 策略缺失、证据失效、岗位冲突或版本变化时阻断，只允许补证、暂挂或转交 |
| 关闭重复/误派 | 更多 | 仅重复或误派，任务类型允许关闭，且有替代任务/终态证据 | 结构化原因和受控证据必填；重复项必须给出替代任务 | 用 `CloseWorkItemEnvelope` 追加关闭记录并返回 `CLOSED`，不影响正式事实 | 证据不足不允许关闭；失败保持原任务有效 |
| 确认直接对账结论 | 无 `work_item` 的差异详情 | 对账原因注册表已配置且当前原因规则、差异版本、内容指纹和强制证据均有效 | 展示“确认无误”或“确认有效差异”、受控原因规则和全部证据 | 只追加 `reconciliation_difference_resolution` 并进入对应差异终态；不完成或关闭任务 | 原因注册表缺失、原因不匹配或证据不足时 fail-closed，差异保持非终态 |
| 暂挂 / 跳过当前项 | 连续处理条 | 当前任务仍有效，`DEFER` / `SKIP` 可用 | 暂挂原因按规则 | 用任务动作信封追加记录；任务仍为 `PENDING/IN_PROGRESS`，租约是否释放以结果为准，焦点可进入下一项 | 明确播报当前项仍未完成及恢复方法 |
| 下一项 | 连续处理条 | 当前快照内有下一有效项 | 无 | 只切队列焦点，不写任务状态 | 目标失效时定位下一有效项 |
| 续租 | 连续处理条 | 当前领取人、租约仍有效 | 无 | 更新租约版本和到期时间，不提供未定义的独立“放弃领取”动作 | 续租失败或租约丢失时保留输入但禁用正式动作；需要离开时使用上方受控暂挂 |

结果未知、资金未闭环、补偿未完成、仍有未解决差异的任务不得使用通用“关闭”。`RESOLVE` 的证据策略按错误类型与资金影响注册；策略未配置时服务端必须从 `allowedActions` 排除 `RESOLVE`，页面只允许补证、暂挂或转交。对账“确认无误”与“确认有效差异”必须命中服务端原因注册表并强制提交受控证据，注册表未配置时不得形成终态。

凡当前项关联正式 `work_item`，不能因为页面已携带 `workItemId` 就省略领取令牌、租约版本、任务版本或内容指纹。`QUERY_ORIGINAL_RESULT`、`REPLAY_ORIGINAL`、`REATTRIBUTE`、`LINK_COMPENSATION`、`ADD_EVIDENCE`、`SKIP`、`DEFER` 使用 W02 `WorkItemActionEnvelope`；`RESOLVE` 使用 `CompleteWorkItemEnvelope`；`CLOSE_DUPLICATE/CLOSE_MISROUTED` 使用 `CloseWorkItemEnvelope`；`TRANSFER` 只使用 `TransferWorkItemEnvelope`。只有未创建 `work_item` 的直接对账差异处理，才使用差异对象命令；该命令不得顺带完成、转交或关闭任何任务。

## 8. 数据契约

### 8.1 查询

```ts
type ResolutionEvidencePolicyId = string & {
  readonly __type: "ResolutionEvidencePolicyId"
}

type ReconciliationReasonRegistryId = string & {
  readonly __type: "ReconciliationReasonRegistryId"
}

type RegisteredReconciliationReasonId = string & {
  readonly __type: "RegisteredReconciliationReasonId"
}

type RegisteredIntegrationErrorType = string & {
  readonly __type: "RegisteredIntegrationErrorType"
}

type ControlledEvidenceRecordId = string & {
  readonly __type: "ControlledEvidenceRecordId"
}

type ResolutionPolicyKey = {
  errorType: RegisteredIntegrationErrorType
  fundsImpact: "NONE" | "POTENTIAL" | "POSTED"
}

type ControlledTerminalEvidenceRef =
  | { kind: "EXTERNAL_CASE_RESULT"; externalCaseRecordId: ControlledEvidenceRecordId }
  | { kind: "BUSINESS_OBJECT_VERIFICATION"; verificationRecordId: ControlledEvidenceRecordId }
  | { kind: "FINANCIAL_RECONCILIATION"; reconciliationRecordId: ControlledEvidenceRecordId }
  | { kind: "COMPENSATION_RESULT"; compensationResultId: ControlledEvidenceRecordId }
  | { kind: "DISTINCT_REVIEW"; reviewRecordId: ControlledEvidenceRecordId }

type ResolutionEvidencePolicyView = {
  evidencePolicyId: ResolutionEvidencePolicyId
  evidencePolicyVersion: number
  key: ResolutionPolicyKey
  requiredEvidenceKinds: ControlledTerminalEvidenceRef["kind"][]
  reviewerSeparation: "NONE" | "DISTINCT_REVIEWER" | "DISTINCT_FINANCE_REVIEWER"
}

type ReconciliationTerminalReason =
  | {
      conclusion: "CONFIRM_NO_ERROR"
      registeredReasonId: RegisteredReconciliationReasonId
      registeredReasonVersion: number
    }
  | {
      conclusion: "CONFIRM_VALID_DIFFERENCE"
      registeredReasonId: RegisteredReconciliationReasonId
      registeredReasonVersion: number
    }

type ReconciliationReasonRegistryView = {
  reasonRegistryId: ReconciliationReasonRegistryId
  reasonRegistryVersion: number
  registeredReasons: ReconciliationTerminalReason[]
}

type IntegrationResolutionQuery = {
  view: "mine" | "result_unknown" | "security" | "auto_retry" | "reconciliation" | "resolved"
  mode?: "all" | "errors" | "reconciliation"
  directions?: string[]
  environment?: "production" | "verification"
  errorClasses?: string[]
  objectType?: string
  objectId?: string
  owner?: "me" | "role_pool" | "claimed"
  age?: string
  q?: string
  queueContextId?: string
  resolveWorkItemId?: string
  sort: string
  cursor?: string
}

type IntegrationResolutionItemView = {
  identity: {
    itemType: "ERROR_TASK" | "RECONCILIATION_DIFFERENCE"
    id: string
    number: string
    subjectHash: string
  }
  workItem?: {
    workItemId: string
    workItemType: "INTEGRATION_RESULT_UNKNOWN" | "BUSINESS_EXCEPTION"
    workItemVersion: string
    status: WorkItemStatus // 直接复用 W02 的唯一正式任务状态契约
    subjectVersion?: string
    subjectHash: string
    completionAction: string
    lease?: LeaseView
  }
  businessObject: AuthorizedObjectRef
  classification: { code: string; label: string; severity: string }
  message?: AuthorizedMessageSummary
  originalAction?: {
    originalActionId: string
    originalActionIdempotencyKeySummary: string
    originalActionIdempotencyKeyLocked: true
  }
  difference?: AuthorizedDifferenceEvidence
  resolutionEvidencePolicy?: ResolutionEvidencePolicyView
  reconciliationReasonRegistry?: ReconciliationReasonRegistryView
  attempts: IntegrationAttemptSummary[]
  objectVersion: string
  allowedActions: string[]
  actionBlockers: ActionBlocker[]
  freshness: { updatedAt: string; sourceWatermark?: string }
}
```

队列使用游标或稳定分页，必须保证同一筛选下切换下一项不会重复/漏掉已处理项。Query Key 包含用户、角色、权限、数据范围、队列筛选、`queueContextId`、可选 `resolveWorkItemId`、任务版本和租约版本；解析成功后清除 `resolveWorkItemId`，领域详情只保留 `:taskId`。

### 8.2 提交与结果

```ts
type IntegrationNonTerminalTaskAction = {
  itemType: "ERROR_TASK" | "RECONCILIATION_DIFFERENCE"
  itemId: string
  kind:
    | "QUERY_ORIGINAL_RESULT"
    | "REPLAY_ORIGINAL"
    | "REATTRIBUTE"
    | "LINK_COMPENSATION"
    | "ADD_EVIDENCE"
    | "SKIP"
    | "DEFER"
  operationId: string
  reasonCode?: string
  comment?: string
  evidenceRefs?: ControlledTerminalEvidenceRef[]
}

type IntegrationTaskActionCommand =
  WorkItemActionEnvelope<IntegrationNonTerminalTaskAction> & {
    expectedWorkItemVersion: string
  }

type IntegrationTaskCompletionDecision = {
  itemType: "ERROR_TASK" | "RECONCILIATION_DIFFERENCE"
  itemId: string
  kind: "RESOLVE"
  operationId: string
  evidencePolicyId: ResolutionEvidencePolicyId
  evidencePolicyVersion: number
  policyKey: ResolutionPolicyKey
  evidenceRefs: [ControlledTerminalEvidenceRef, ...ControlledTerminalEvidenceRef[]]
  comment?: string
}

type IntegrationTaskCompletionCommand =
  CompleteWorkItemEnvelope<IntegrationTaskCompletionDecision> & {
    expectedWorkItemVersion: string
  }

type IntegrationTaskCloseDecision =
  | {
      kind: "CLOSE_DUPLICATE"
      itemType: "ERROR_TASK" | "RECONCILIATION_DIFFERENCE"
      itemId: string
      operationId: string
      reasonCode: string
      replacementWorkItemId: string
      closureEvidenceReference: string
      comment?: string
    }
  | {
      kind: "CLOSE_MISROUTED"
      itemType: "ERROR_TASK" | "RECONCILIATION_DIFFERENCE"
      itemId: string
      operationId: string
      reasonCode: string
      replacementWorkItemId?: string
      closureEvidenceReference: string
      comment?: string
    }

type IntegrationTaskCloseCommand =
  CloseWorkItemEnvelope<IntegrationTaskCloseDecision> & {
    expectedWorkItemVersion: string
  }

type IntegrationTaskTransfer = {
  itemType: "ERROR_TASK" | "RECONCILIATION_DIFFERENCE"
  itemId: string
  operationId: string
  targetRole: string
  targetUserId?: string
  reasonCode: string
  comment?: string
}

type IntegrationTaskTransferCommand =
  TransferWorkItemEnvelope<IntegrationTaskTransfer> & {
    expectedWorkItemVersion: string
  }

type DirectReconciliationDecision =
  | {
      kind: "NON_TERMINAL_ACTION"
      action:
        | "QUERY_ORIGINAL_RESULT"
        | "REPLAY_ORIGINAL"
        | "REATTRIBUTE"
        | "LINK_COMPENSATION"
        | "ADD_EVIDENCE"
      evidenceRefs?: ControlledTerminalEvidenceRef[]
      comment?: string
    }
  | {
      kind: "TERMINAL_CONCLUSION"
      reasonRegistryId: ReconciliationReasonRegistryId
      reasonRegistryVersion: number
      reason: ReconciliationTerminalReason
      evidenceRefs: [ControlledTerminalEvidenceRef, ...ControlledTerminalEvidenceRef[]]
      comment?: string
    }

type DirectReconciliationCommand = {
  differenceId: string
  expectedDifferenceVersion: string
  expectedSubjectHash: string
  decision: DirectReconciliationDecision
  operationId: string
  idempotencyKey: string
}

type IntegrationTaskActionEvidence = {
  operationId: string
  outcome:
    | "TERMINAL_EVIDENCE_FOUND"
    | "NO_RESULT_CONFIRMED"
    | "RESULT_UNKNOWN"
    | "REPLAY_ACCEPTED"
    | "REATTRIBUTED"
    | "EVIDENCE_LINKED"
    | "EVIDENCE_ADDED"
    | "DEFERRED"
    | "SKIPPED"
  businessResultReference?: string
  evidenceReference?: string
}

type IntegrationTaskActionResult =
  WorkItemActionResult<IntegrationTaskActionEvidence> & {
    nextAllowedActions: string[]
  }

type IntegrationTaskCompletionResult =
  CompleteWorkItemResult<{
    operationId: string
    resolutionRecordId: string
    terminalEvidenceReference: string
  }>

type IntegrationTaskCloseResult =
  CloseWorkItemResult & {
    operationId: string
    resolutionRecordId: string
  }

type IntegrationTaskTransferResult =
  TransferWorkItemResult & {
    operationId: string
  }

type DirectReconciliationResult = {
  differenceId: string
  operationId: string
  resolutionRecordId: string
  resultingStatus:
    | "OPEN"
    | "EVIDENCE_PENDING"
    | "CONFIRMED_NO_ERROR"
    | "CONFIRMED_VALID_DIFFERENCE"
  isTerminal: boolean
  outcome:
    | IntegrationTaskActionEvidence["outcome"]
    | "CONFIRMED_NO_ERROR"
    | "CONFIRMED_VALID_DIFFERENCE"
  businessResultReference?: string
}
```

- 非终结、完成、关闭、转交四类任务命令都校验领取人、`claimToken`、`leaseVersion`、`expectedWorkItemVersion`、业务对象版本与 `expectedSubjectHash`，但终态语义不得混用。
- `IntegrationTaskActionCommand` 只追加查询、重放、重新归集、补偿关联、跳过或暂挂的证据。成功必须返回服务端正式 `workItemStatus: "PENDING" | "IN_PROGRESS"`；即使查询或重放取得可验证终态，也必须再以当前版本执行 `RESOLVE` 才能完成任务。仍为 `RESULT_UNKNOWN` 时保留当前任务、输入和查询入口，不得自动下一项。
- `IntegrationTaskCompletionCommand` 只接受 `RESOLVE`；命令必须引用当前错误类型与资金影响所命中的 `evidencePolicyId/evidencePolicyVersion`，并提交非空强类型 `evidenceRefs`。服务端重验策略版本、必需证据种类和岗位分离后，才把可验证终态处理记录与任务 `COMPLETED` 在同一事务提交；策略缺失或不匹配时拒绝。
- `IntegrationTaskCloseCommand` 只接受 `CLOSE_DUPLICATE` / `CLOSE_MISROUTED`，完整复用 W02 `CloseWorkItemEnvelope`；结构化原因与受控关闭证据必填，重复项还必须引用替代任务。服务端确认任务类型允许关闭后，关闭记录和任务 `CLOSED` 在同一事务提交，且不写业务解决结论。
- `IntegrationTaskTransferCommand` 的处理记录、原任务 `TRANSFERRED`、原租约失效和 `UNCLAIMED/PENDING` 后继任务创建同一事务提交；转交不写解决结论。
- `DirectReconciliationCommand` 只允许操作无 `work_item` 的 `reconciliation_difference`，以差异版本和内容指纹做并发保护。终结分支只能是 `ReconciliationTerminalReason` 的“确认无误”或“确认有效差异”，并强制引用有效原因注册表版本和非空受控证据；注册表缺失时只允许非终结补证/查询等动作。该命令不接受 `CLOSE_DUPLICATE`、不返回任务 `CLOSED`；若存在 `work_item`，重复/误派关闭必须改用 `IntegrationTaskCloseCommand` 的 `CloseWorkItemEnvelope`。
- `DirectReconciliationResult.resultingStatus` 由最后一条追加式差异处理记录派生并使用服务端固定枚举；前端只读取 `isTerminal`，不得复用 `work_item` 状态或另造差异状态机。
- `WorkItemActionEnvelope.idempotencyKey`（或直接差异命令的 `idempotencyKey`）只标识**本次任务/差异动作**。原对外动作的 `originalActionIdempotencyKey` 存在服务端不可变原动作记录中，查询视图只给摘要和锁定标识；重放命令不接受该键，服务端必须自行读取并沿用，客户端无权生成、覆盖或替换。
- `QUERY_ORIGINAL_RESULT` 保存查询证据，但不自行推进正式业务状态。
- `REATTRIBUTE` 复用原业务事实键，结果引用原事实；不得复制 `mall_order_fact` 或消费事实。
- `RESOLVE` 要求已配置证据策略、强类型终态证据和岗位分离，并只通过完成信封返回 `COMPLETED`；`CLOSE_DUPLICATE/CLOSE_MISROUTED` 要求关闭原因及替代任务/受控证据并只通过关闭信封返回 `CLOSED`。
- mutation 返回 `operationId`、处理记录、业务终态或关闭证据引用，以及适用时的新租约、任务完成、关闭或转交链结果。非终结动作只更新当前项；完成、关闭或转交得到确定结果后才可推进队列。

### 8.3 前端边界

- 前端不判断某 HTTP 响应是否代表业务成功，不把自动重试次数当业务状态。
- 安全重放、重试退避、能力支持、差异是否闭环、终态证据是否充分由服务端判断。
- 前端只格式化错误文案、时间、环境、证据差异和状态；不计算业务事实键或幂等键。
- 原始载荷只能通过受控、加密、短时且审计的安全查看入口；普通页面和日志只接收脱敏摘要。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 队列、详情和处理区等尺寸 Skeleton | 应用壳导航 | 原位替换 |
| 刷新 | 保留队列和当前项，显示水位 | 当前输入保留；正式动作重验 | 成功更新；失败保留旧值 |
| 队列为空 | “当前筛选项已处理完” | 返回工作台、清除筛选 | 新任务到达后刷新 |
| 筛选无结果 | 显示筛选摘要 | 清除筛选 | 返回默认视图 |
| 无数据范围 | 专用无范围空态 | 查看角色 / 申请权限 | 范围更新后重查 |
| 查询失败无缓存 | `BusinessFailureState` | 重试、返回来源 | 查询恢复 |
| 局部证据失败 | 影响摘要与身份保留，证据区失败 | 重试证据查询 | 局部恢复 |
| 解决证据策略未配置 | 显示错误类型、资金影响和“解决证据规则尚未配置”，`RESOLVE` 禁用 | 补证、暂挂、转交 | 注册匹配策略并刷新后重验 |
| 对账原因注册表未配置 | 直接差异保持非终态，“确认无误/有效差异”均禁用 | 补证、暂挂或转为正式任务 | 注册受控原因与证据要求后重验 |
| 数据陈旧 | 显示消息/事实/对账各自水位 | 刷新；正式动作阻断或重验 | 追平后解除 |
| 字段级隐藏 | 敏感值和技术摘要掩码 | 其余授权动作 | 权限更新后重查 |
| 领取中 / 租约有效 | 显示领取人和剩余时间 | 允许对应处理 | 续租或完成 |
| 租约丢失 | 输入保留，正式动作禁用 | 重新领取、复制非敏感说明 | 取得新租约并重验版本 |
| 内容指纹 / 对象版本冲突 | 展示任务 `subjectHash` 针对事实与当前事实的结构化差异，禁用旧决定 | 刷新证据、进入替代任务 | 基于当前版本重新领取处理 |
| 自动重试中 | 显示退避时间和最近尝试 | 查看、必要时转人工 | 自动终态或超限转人工 |
| 结果未知 | 固定警示；主动作只为“查询原结果” | 查询、转人工 | 得到终态/明确无结果 |
| 明确无结果 | 展示查询证据与安全判断 | 允许时由服务端沿锁定的原幂等键重放 | 固定展示重放结果，任务仍为 `PENDING/IN_PROGRESS` |
| 保存/提交失败 | 保留原因、证据和说明 | 重试同一操作 | 成功后写处理记录 |
| 非终结动作成功 | 固定展示查询、重放、归集、关联、跳过或暂挂证据；任务明确仍为 `PENDING/IN_PROGRESS` | 继续查询、验证后解决、暂挂或转交 | 不自动下一项；按返回的新租约重验 |
| 非终结动作仍结果未知 | 固定警示和本次动作证据；任务保持 `PENDING/IN_PROGRESS` | 查询本次动作最终结果、沿同一任务动作幂等键重试 | 得到确定动作结果后仍需显式解决 |
| 正式完成成功 | 固定结果 + 终态/补偿引用；任务为 `COMPLETED` 后再进入下一项 | 打开结果、下一项 | 不依赖 toast |
| 正式关闭成功 | 固定显示 `CLOSED`、结构化原因、替代任务/受控证据和关闭记录；不显示业务已解决 | 打开替代任务/证据、下一项 | 关闭结果可审计，不依赖 toast |
| 转交成功 | 固定展示原任务、转交记录和 `UNCLAIMED/PENDING` 后继任务 | 打开后继、下一项 | 不把转交显示为解决或完成 |
| 正式动作结果不确定 | 不跳下一项、不改本地终态 | 查询最终结果 | 得到可验证结果 |
| 权限收回 | 清除证据、摘要和下载链接 | 返回有权模块 | 权限恢复后重查 |

## 10. 响应式与键盘

| 视口 | 布局变化 | 保留内容 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 38/62 双栏连续队列 | 队列位置、影响、证据、主动作和结果 | 无 |
| 1280×800 | 左栏收窄；证据区标签化 | 对象身份、错误类别、滞留和动作 | 尝试历史默认折叠 |
| 1024×768 | 图标侧栏；队列可折叠为顶部选择条 | 当前项、位置、结果未知规则 | 左右证据改单列 |
| 768×1024 | 导航抽屉；队列在上、详情在下 | 处理条、业务影响、证据摘要、主动作 | 技术细节折叠；表格横向滚动 |
| 375×812 | 单列只读任务卡；允许领取、查询结果、暂挂 | 任务身份、影响、结果未知提示和查询结果 | 不提供重放、补偿、差异修复、原文查看；提示桌面处理 |

键盘顺序：页头筛选 → 队列项 → 连续处理条 → 影响摘要 → 证据 → 处理动作。j/k 或方向键移动当前队列；Enter 打开；处理后焦点落新对象标题并播报“第 x/y 项”。Dialog/Sheet 关闭回触发源。结果未知时读屏器必须先读警示和“查询原结果”，不能把重放按钮排在其前。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 今日工作台 / 待办 | W01 / W02 | 任务 ID、队列上下文、责任范围 | 处理后刷新原任务 |
| 商城同步与映射 | W17 | 同步任务、来源快照、映射类型 | 修复后回当前项重新归集 |
| API 连接 | W20 | 连接、环境、能力、健康结果 | 返回保留异常队列 |
| 外部商品映射 | W21 | 外部商品、SKU、映射差异 | 映射生效后回当前项 |
| 商品发布 / 执行投影 | W22 / W23 | 发布/投影版本、投递消息 | 返回查询原投递结果 |
| 主责迁移 | W24 | 迁移批次、阻塞项、基线证据 | 返回原差异，不自动续跑 |
| 商城消费 / 供应商订单 | W25 / W26 | 事实、订单、售后请求、原动作 | 正式结果后回 W29 验证闭环 |
| API 结算 | W27 | 账单消息、结算单、差异 | 处理后重新试算/对账 |
| 卡券分析 / 历史回填 | W28 / W30 | 未归集原因、回填任务、事实键 | 解决后等待投影刷新 |

## 12. 验收清单

### 12.1 安全处理与幂等

- [ ] 结果未知时不能直接重放，下单/取消/退款均先查询原结果。
- [ ] 只有明确无结果且服务端确认安全时开放重放；本次任务动作幂等键与服务端锁定的 `originalActionIdempotencyKey` 分离，客户端不能传入或替换原键。
- [ ] 业务明确拒绝、参数/映射错误和鉴权/签名失败不会进入无意义自动重试。
- [ ] 实时与回填重复事实按业务事实键只形成一份正式记录。
- [ ] 重复、乱序回调不会创建第二份事实或使业务状态倒退。
- [ ] 原消息、尝试、差异、正式事实和处理记录均不可被页面覆盖。

### 12.2 闭环与审计

- [ ] 错误详情内能完成查询、重放、转交、关联补偿和终态验证，不需去日志平台猜结果。
- [ ] 结果未知、资金未闭环或补偿未完成的任务不能通用关闭。
- [ ] 解决必须引用与错误类型、资金影响匹配的 `evidencePolicyId/evidencePolicyVersion`、非空强类型证据并通过岗位分离；策略未配置时只能补证、暂挂或转交。
- [ ] 无任务直接对账的“确认无误/有效差异”必须引用已配置原因注册表中的强类型原因和非空受控证据；注册表缺失时保持非终态，不接受自由字符串原因或可选证据。
- [ ] 关闭重复/误派必须引用替代任务或受控证据，并始终使用 `CloseWorkItemEnvelope`；直接对账命令不伪造 `CLOSED`。
- [ ] 对账只生成差异，修改业务必须进入正式变更、纠错、重新归集或重放入口。
- [ ] 任务领取、租约、转交和连续处理位置均可恢复并有审计。
- [ ] `QUERY/REPLAY/REATTRIBUTE/LINK/ADD_EVIDENCE/SKIP/DEFER` 使用 `WorkItemActionEnvelope`，成功或结果未知时任务均保持正式 `PENDING/IN_PROGRESS`，不会自动下一项。
- [ ] 只有 `RESOLVE` 使用 `CompleteWorkItemEnvelope` 并返回 `COMPLETED`；重复/误派关闭使用 `CloseWorkItemEnvelope` 并强制原因、替代任务/受控证据后返回 `CLOSED`；`TRANSFER` 使用转交信封并原子创建 `UNCLAIMED/PENDING` 后继任务。
- [ ] 所有 task-bound 写动作均携带对应 W02 信封和 `expectedWorkItemVersion`；完成、关闭或转交只按各自明确定义改变任务状态。
- [ ] 无任务的直接对账命令只追加差异处理记录，不会隐式完成、转交或关闭 `work_item`。

### 12.3 权限、状态与体验

- [ ] 普通业务页面和导出不出现密钥、完整请求/响应、完整手机号和地址。
- [ ] 环境、严重度和状态不只靠颜色表达。
- [ ] §9 全部状态和 §10 五档视口完成验收。
- [ ] 正式动作结果固定展示；结果不确定时停留当前项且不自动下一条。
- [ ] 键盘与读屏可完成队列浏览、查询原结果、查看证据和返回来源。

## 13. 待确认事项

| ID | 问题 | 影响 | 建议决策人 | 当前建议 |
| --- | --- | --- | --- | --- |
| Q1 | 各错误类别自动重试上限和退避策略由谁维护？ | 自动/人工边界和 SLA | 运维负责人 + 各连接负责人 | 由服务端按连接策略维护，UI 只显示不可编辑摘要 |
| Q2 | 哪些角色有权查看完整事件 ID 和外部请求号？ | 排障效率与信息暴露 | 安全负责人 + 运维负责人 | 管理员/运维可复制完整稳定标识；业务角色只看摘要 |
| Q3 | 结果未知且供应商无查询能力时，哪些证据足以形成可验证终态？ | 任务能否解决 | 业务部门 + 运维 + 财务 | 按错误类型与资金影响注册证据规则和岗位分离；未配置匹配的 `evidencePolicyId/version` 前禁止 `RESOLVE`，只允许补证、暂挂或转交 |
| Q4 | 安全故障是否需要独立强制值班升级时限？ | 排序、通知和告警 | 安全负责人 + 运维负责人 | 生产鉴权/签名失败立即告警并置顶，不等待普通 SLA |
| Q5 | 对账差异“确认无误”与“确认有效差异”的受控原因枚举有哪些？ | 防止把真实差异以自由文案终结 | 各业务负责人 | 原因注册表未配置前两类终结动作均 fail-closed；配置后必须引用强类型注册原因、版本和非空受控证据，任务重复关闭仍单独使用 `CloseWorkItemEnvelope` |

## 14. 业务依据

- `erp-phase-2.md` §6.3、§13：统一错误分类、两层幂等、结果未知先查询、原幂等键重放和周期对账。
- `erp-phase-2.md` §17.7：日志脱敏、回调校验、人工重放和对账不修改正式事实。
- `erp-data-model.md` §6.1 `work_item`：领取租约、转交、关闭限制和终态证据。
- `erp-data-model.md` §6.21：outbox/inbox、错误任务、对账差异和追加式处理记录。
- `erp-data-model.md` §7.7、§9.4：集成投递状态机和结果未知不得盲目重试。
- `erp-ui-design.md` §4.4、§4.8、§11：M3/M7、错误详情内闭环、结果不确定和队列恢复。
- `erp-ui-flows.md` §9、§11.2：映射协同与供应商异常补偿的工作面路径。
