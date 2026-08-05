# W17 · 商城同步与映射

> 状态：草稿
> 页面模式：M7 治理与导入（映射任务使用连续处理语言）
> 主要路由：`/governance/mall-sync`
> 主要角色：系统管理员、销售、运营；财务按任务与字段权限参与
> 最后更新：2026-08-04

## 1. 定位与目标

### 1.1 用户目标

W17 把第一期卡券销售单的商城同步运行状态、来源快照、业务映射任务和每日全量核对放在同一个治理工作面。

不同角色进入后应能回答：

- 系统管理员：同步是否按水位推进，是否漏单或积压，哪一次失败可以安全补拉或重试？
- 销售：来源客户、合同，以及仅在服务端明确分配给销售时的结算主体，应映射到哪个 ERP 正式对象？
- 运营：来源卡券类目是否能映射到 ERP 卡券销售项，唯一卡券明细是否符合规则？
- 财务：金额、税率、开票，以及仅在服务端明确分配给财务时的结算归属异常是否已由唯一责任人确认？
- 全部角色：差异解决后是否使用原来源身份重新归集，是否已经形成可验证的 ERP 销售版本和应收结果？

### 1.2 业务目标

- 第一阶段清晰呈现“商城开单商业事实 → ERP 正式销售单版本”的单向同步，不误写为双向编辑。
- 将同步技术故障和业务映射差异分开归责，同时在一个工作面中保留完整链路证据。
- 映射失败时阻止错误客户、应收、收入和经营归属，但不阻塞商城销售、制卡、绑定、激活和消费。
- 补拉、重试、映射解决和重新归集均沿用原来源身份、快照和幂等依据，不手工补建第二张销售单。
- 切换时点 T：商城停止创建 B2B 销售单，全部 B2B 销售单统一由 ERP 服务；T 起 W17 封存为只读历史态，保留历史证据，当前执行投影转 W23、通用接口错误与对账转 W29。

### 1.3 不在本工作面完成

- 不修改商城卡券销售单商业字段，不向商城回写第一期 ERP 侧修改。
- 不维护商城玩法、卡号、卡密、绑定手机号、卡实例秘密、支付或消费执行细节。
- 不允许系统管理员替销售、运营或财务确认业务映射。
- 不在差异页直接覆盖 ERP 销售版本、应收、回款、发票或经营事实。
- 不以“手工标记成功”“移动水位”或关闭任务代替可验证同步结果。
- 二期供应商商品映射进入 W21，销售执行投影进入 W23，T 切换后商城停单、ERP 全面服务，通用错误和对账进入 W29；W17 不复制这些工作面。

## 2. 用户、权限与数据范围

| 角色 | 默认子视图 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 系统管理员 | 运行总览 / 同步任务 | 授权来源商城的任务、水位、脱敏快照、技术错误和差异摘要 | 查看定时同步；直接执行立即增量或按单补拉，操作留审计；技术重试、指派业务任务、运行核对；不可确认业务归属 |
| 销售 | 我的映射任务 | 负责客户、合同，以及 `ownerRole=SALES` 时的结算主体差异及必要来源销售摘要 | 确认有权业务对象映射、在当前映射任务内追加来源修复说明与证据、查看重新归集结果 |
| 运营 | 我的映射任务 | 卡券类目、卡形态、唯一明细及授权销售摘要 | 确认卡券类目映射、处理运营语义差异；不可改商业字段 |
| 财务 | 我的映射任务 / 核对摘要 | 明确分派且授权的税率、开票、金额格式，以及 `ownerRole=FINANCE` 时的结算归属与结果摘要 | 确认财务语义或退回责任方；不可修正来源金额 |
| 销售经理/管理人员 | 风险摘要只读 | 授权客户范围的未归属数量和影响 | 督办和下钻，不执行技术操作 |
| 研发运维（二期） | 无 W17 默认入口 | 第一阶段历史技术证据；通用接口进入 W29 | 排障查询；不确认业务映射 |

### 2.1 分权规则

| 情况 | 界面行为 |
| --- | --- |
| 无 W17 模块权限 | 不展示入口；直接访问显示无权限页 |
| 有模块权限但无来源商城范围 | 展示“当前无来源系统数据范围”，不显示全公司 0 值 |
| 只有技术权限 | 可看任务、水位、错误码和脱敏来源摘要；业务候选和“确认映射”不可用 |
| 只有业务映射权限 | 只看指派任务与业务白名单字段；不可查看连接配置、原始报文或触发全量任务 |
| 无敏感字段权限 | 客户联系方式、税务、合同附件等值掩码；卡密等禁止字段任何角色均不可见 |
| 人工同步动作 | “立即增量”和“按单号补拉”直接可用，操作追加审计；服务端按阶段校验并拒绝封存后的人工动作。系统定时增量仍按调度契约运行 |
| T 后（封存） | W17 转为只读历史态：不再从商城拉取新商业事实，同步、核对及执行类写动作全部禁用，历史证据保留可查；当前动作引导至 W23/W29 |
| 权限运行中收回 | 清除来源快照和候选对象缓存，保留任务稳定身份与返回上下文 |

所有总数、差异数和任务列表均由服务端按来源系统、角色、客户数据范围与字段权限计算。前端不得下载全量快照后自行裁剪。

## 3. 入口、路由与任务页签

W17 子视图使用固定 `view`：`overview`、`jobs`、`snapshots`、`mapping`、`reconciliation`、`history`。

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 管理员默认进入 | 侧栏“商城同步与映射” | `/governance/mall-sync?view=overview&source=...` | 固定 W17 页签 |
| 业务人员从待办进入 | W01/W02 正式待办 | `view=mapping&workItemId={id}&mappingTaskId={id}&queueContextId={id}`；以 `workItemId` 领取/完成，以 `mappingTaskId` 查询领域证据 | 返回队列时保留原位置 |
| 查看同步批次 | 总览指标 / 任务表 | `view=jobs&jobId={id}`，detail 在当前页签 | 关闭 detail 回原行 |
| 查看来源快照 | 销售单协同提示 / 同步任务 | `view=snapshots&snapshotId={id}` | 返回原销售单或任务 |
| 查看核对差异 | 总览 / 告警 | `view=reconciliation&jobId={id}&differenceId={id}` | 后退恢复核对批次列表 |
| 打开 ERP 销售单 | 已应用快照 / 映射结果 | 新任务页签打开 W05 稳定销售单 | 关闭后恢复 W17 任务焦点 |
| T 切换后进入 | 历史链接 | 已封存为只读历史：不展示同步、核对或映射写动作，自动落 `view=history`，显示封存时间/水位 | 当前协同按对象导向 W23/W29 |

TaskTabs 身份：工作面为 `governance:mall-sync:{sourceSystemId}`；映射连续处理仍在该页签内，当前任务、队列位置和筛选写入 URL。重复打开同一来源商城聚焦已有页签。

刷新恢复来源、子视图、筛选、任务/快照/核对身份；不恢复临时确认框。浏览器后退按子视图和选中对象恢复。映射表单存在未提交说明时页签标脏，关闭必须确认。

## 4. 页面布局

### 4.1 桌面布局

```text
┌ PageHeader：商城同步与映射    来源：福利商城生产   [数据水位] [立即增量]
├ OwnershipBanner
│ 当前方向：商城 → ERP 商业事实  · 商城开单 · ERP 只读接收
│ 或：T 后已封存（商城停单、ERP 全面服务，只读历史） · [前往投影/错误中心]
├ MetricStrip：同步延迟 | 失败任务 | 待映射 | 核对差异 | 待重新归集
├ 子视图：运行总览 | 同步任务 | 来源快照 | 映射任务 | 每日核对 | 历史
├──────────────────────────────────┬──────────────────────────────────┐
│ 当前子视图列表 / 趋势            │ 选中对象 detail / 映射处理区       │
│ 状态、范围、水位、责任人、影响     │ 来源白名单事实 ↔ ERP 候选/当前事实 │
│                                   │ 历史、操作、正式结果               │
└──────────────────────────────────┴──────────────────────────────────┘
```

### 4.2 区域说明

| 区域 | 目的 | 主组件 | 是否固定 |
| --- | --- | --- | --- |
| 页头 | 选择来源商城、刷新和管理员主动作 | `PageHeader`、`DataFreshness` | 固定 |
| 来源边界 | 明确方向、来源（商城开单）和封存状态 | `MaintenanceBanner` / `Alert` | 始终在指标上方 |
| 指标区 | 识别延迟、失败、映射积压和核对差异 | `MetricStrip` | 指标可点击过滤 |
| 子视图导航 | 在同一链路切换任务、快照、映射和核对 | Tabs | sticky |
| 列表区 | 高密度查询和选择待处理对象 | `BusinessTableFrame`、`DataTable` | 身份/状态/操作列固定 |
| detail | 读完整任务证据和处理历史 | `QuickPreviewSheet size="detail"` 或右侧处理区 | 覆盖/分栏 |
| 映射差异 | 比较来源白名单字段、ERP 候选和当前正式结果 | `BusinessDiffPanel`、`SequentialProcessBar` | 处理时固定动作区 |
| 后台进度 | 展示同步、补拉、重新归集和核对任务 | `BackgroundJobProgress` | 任务运行时持续可见 |

### 4.3 子视图内容

| 子视图 | 核心内容 | 主要角色 |
| --- | --- | --- |
| 运行总览 | 当前阶段/方向、水位、延迟趋势、最近任务、差异分布、来源健康摘要 | 系统管理员、管理只读 |
| 同步任务 | 期初基线、增量拉取、单号补拉的范围、分页、数量、错误和水位变化 | 系统管理员 |
| 来源快照 | 来源单号、更新时间、观察时间、指纹、映射状态、应用版本和白名单内容 | 系统管理员；业务按任务范围 |
| 映射任务 | 客户、合同、结算主体、卡券类目、唯一明细和格式差异的连续处理 | 销售、运营、财务、管理员指派 |
| 每日核对 | 商城清单与 ERP 当前状态/完整内容指纹差异、补拉和终态证据 | 系统管理员；业务按责任下钻 |
| 历史 | 已封存轮询、最终水位、历史版本和处理证据 | 授权只读角色 |

## 5. 展示内容与字段

### 5.1 来源边界与运行指标

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 边界 | `ownershipStage` | 商城开单（T 前）/ T 后 ERP 全面服务（只读历史） | 销售单来源 | T 后仅历史只读，不回写商城；不用“原生/影子单”等旧称 | 全部有权用户 |
| 边界 | `syncDirection` | 商城 → ERP 商业事实 / 已封存（不再拉取） | 服务端阶段状态 | 第一阶段只读拉取；T 起停止从商城拉取新商业事实，不再捕获或应用一期快照 | 全部有权用户 |
| 边界 | `sealedAt/finalWatermark` | 封存时间 / 最终水位 | T 切换与同步封存事实 | 绝对时间和证据引用 | 有历史权限 |
| 指标 | `syncLagSeconds` | 同步延迟 | 安全水位与来源安全时间 | 服务端计算；显示最近成功时间 | 管理员；业务只看等级 |
| 指标 | `failedJobCount` | 失败任务 | `mall_sales_sync_job` | 当前未恢复失败/部分失败数 | 管理员 |
| 指标 | `pendingMappingCount` | 待映射 | `master_mapping_task` | 当前角色/范围内进行中任务 | 按任务角色过滤 |
| 指标 | `pendingReapplyCount` | 待重新归集 | 快照处理投影 | 映射已解决但未取得正式应用终态 | 对应责任角色 |
| 指标 | `reconciliationDifferenceCount` | 核对差异 | 最近有效核对批次 | 待处理、补拉中（按单补拉与人工排查） | 按范围过滤 |

### 5.2 同步任务与水位

| 字段 | 用户文案 / 表现 | 数据来源 | 说明 |
| --- | --- | --- | --- |
| `jobNo/jobType` | 任务号 · 基线/增量/单号补拉 | `mall_sales_sync_job` | 每次执行稳定身份 |
| `rangeStart/rangeEnd` | 查询范围 | 任务事实 | 管理员可见；业务不必展示技术边界 |
| `status` | 运行中/成功/部分失败/失败 | 任务状态 | 文字 + tone |
| `page/item/errorCount` | 页数/快照数/错误数 | 任务统计 | 成功数不等于形成销售版本数 |
| `cursorBefore/After` | 来源捕获水位变化 | 水位审计投影 | 只有来源读取、白名单规范化或快照安全持久化失败才阻断推进；后续异步映射失败不回退已安全捕获水位 |
| `startedAt/finishedAt` | 开始/完成 | 任务事实 | 业务时区绝对时间 |
| `lastError` | 失败分类和业务影响 | 脱敏错误摘要 | 不展示堆栈、连接信息或密钥 |
| `triggeredBy` | 系统调度 / 操作人 | 审计 | 人工触发含理由 |

页面禁止提供直接编辑高水位、同刻游标或重叠窗口的控件。增量任务的范围和稳定分页规则由服务端契约决定。水位只证明来源区间内白名单快照已安全捕获和持久化，不证明映射、销售版本、应收或经营投影已经成功；这些后续状态必须独立展示。

### 5.3 来源快照

| 字段 | 用户文案 / 表现 | 数据来源 | 说明 |
| --- | --- | --- | --- |
| `externalOrderNo` | 商城销售单号 | 来源原值 | 保留大小写和原字节语义；可复制 |
| `sourceUpdatedAt` | 商城更新时间 | 来源快照 | 用于迟到判定，不等于 ERP 观察时间 |
| `observedAt` | ERP 观察时间 | 快照事实 | 判断同步延迟 |
| `sourceStatusLabel` | 商城状态 | 来源状态字典 | 未知代码显示未知并进差异，不默认启用/完成 |
| `contentHashShort` | 商业内容指纹 | `content_hash` | 默认缩略；完整值仅技术复制，不用于业务判断 |
| `mappingStatus` | 待映射/已应用/差异/无变化 | 快照处理状态 | A→B→A 第三次 A 仍为新观察 |
| `appliedRevision` | ERP 已形成版本 | 应用结果 | 链到 W05；无结果显示原因而非 0 |
| `syncJobNo` | 来源同步任务 | 快照来源 | 可回溯任务 |

detail 只展示安全白名单商业字段：状态、客户、合同、结算主体、项目名称/业务备注、卡券类目、销售单级履约期限、恰好一条卡券明细的面额/数量/成交金额/卡形态、税率、开票要求和金额合计。

玩法规则、卡号、卡密、绑定手机号、卡实例秘密、支付数据、数据库连接和接口密钥既不展示，也不允许进入快照或指纹。原始报文若存在加密引用，仅供受控技术调查，不在普通页面直接下载。

### 5.4 映射任务

| 字段 | 用户文案 / 表现 | 数据来源 | 说明 |
| --- | --- | --- | --- |
| `mappingTaskId/workItemId` | 映射差异 / 正式待办 | `master_mapping_task` + `work_item` | 前者保存领域差异与解决状态，后者保存领取、责任和完成动作；不得合并成一个私有任务状态 |
| `mappingType` | 客户/合同/结算主体/卡券类目/唯一明细/金额格式 | `master_mapping_task` | 类型决定候选对象类型；结算主体必须由服务端配置唯一 `ownerRole` |
| `sourceEvidence` | 来源字段和值 | 白名单快照 | 只显示完成判断必需内容 |
| `candidateTargets` | ERP 候选 | 规范对象查询 | 相似只作候选，绝不自动确认/合并 |
| `currentTargets` | 当前来源身份谱系 | `external_identity_map/target` | 显示有效期、关系角色和历史 |
| `mappingTaskStatus` | 待处理/已解决/无法处理/已关闭 | `master_mapping_task` | 关闭必须有替代任务或证据；不复用来源快照或重新归集状态 |
| `ownerRoutingState/workItemStatus/owner` | 待责任配置，或待领取/待处理/处理中/已完成/已转交/已关闭 · 责任角色/人员 | 责任路由配置 + `work_item` | 结算主体责任未配置时不创建可执行确认待办；配置后只创建一个正式待办。领取和转交服从 W02 统一契约，管理员可指派但不能替代业务确认 |
| `reapplyOperationStatus` | 未开始/排队/运行/成功/失败/结果未知 | 重新归集强类型操作 | 映射已解决后独立推进；结果未知不得把 `mappingTaskStatus` 回滚为待处理 |
| `impactSummary` | 业务影响 | 服务端 | 如“未形成应收和经营归属”，不写技术堆栈 |
| `resolutionHistory` | 处理结论、依据、结果 | 不可变处理审计 | 包含重新归集正式结果引用 |

映射任务的处理区左右对照：左侧来源白名单事实，右侧 ERP 候选/当前对象，中部明确“确认的是身份关系，不是修改来源销售单”。每个已完成责任路由、需要人工处理的有效 `master_mapping_task` 必须关联一个有效 `work_item`；转交直接更新责任人并记录审计，两者仍关联同一映射任务，页面不维护第二套领取字段。结算主体责任未配置时，领域差异可保存但不进入业务确认队列，待配置生效后原子创建唯一正式待办。

责任路由固定遵守唯一责任人语义：客户映射进入销售、合同映射进入销售、卡券类目/唯一明细进入运营、税率和金额口径进入财务；结算主体由服务端 `ownerRole` 配置在销售与财务中二选一。结算主体未配置唯一 `ownerRole` 时，任务显示“待责任配置”，`CONFIRM_TARGET` 禁用；不得同时向销售和财务生成两个可完成待办，也不得由前端按当前登录人选择责任。

### 5.5 每日核对

核对批次展示来源清单边界/摘要、来源和 ERP 数量、差异数量、状态、开始/完成时间。差异项展示来源单号、来源状态/更新时间/完整内容指纹、ERP 销售单/当前版本/指纹、差异类型、补拉任务和处理证据。

核对必须比较完整商业内容指纹，不能只比状态和金额。核对只生成差异及任务，不直接覆盖任何一侧事实。

## 6. 搜索、筛选、排序与默认视图

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 来源商城 | 当前唯一生产商城 | `source` | 生产/验证来源身份隔离；不可跨环境合并 |
| 子视图 | 按角色默认 | `view` | 管理员 overview，业务人员 mapping |
| 搜索 | 空 | `q` | 商城销售单号、ERP 销售单号、任务号；来源单号按协议精确语义 |
| 时间范围 | 最近 24 小时 / 当前批次 | `from/to` | 按子视图作用于任务或观察时间，标签明确 |
| 状态 | 未完成优先 | `status` | 每个子视图使用自己的固定状态字典 |
| 任务类型 | 全部 | `jobType` | 基线、增量、单号补拉；核对使用独立子视图 |
| 映射类型 | 当前角色全部 | `mappingType` | 客户、合同、结算主体、类目、明细、格式 |
| 责任范围 | 我的任务 | `owner=mine` | 管理员可切角色/全部，业务人员受范围限制 |
| 差异类型 | 全部未解决 | `differenceType` | 商城缺失、ERP 缺失、状态、指纹、重复身份 |
| 排序 | 风险/滞留优先 | `sort` | 映射按超期、优先级、创建时间；任务按开始时间倒序 |

指标点击切换相应子视图和筛选，具有 `aria-pressed`、筛选摘要和结果数。1440×900 下列表至少展示 6–8 行；身份列、状态列和主动作固定。

一次性运营迁移完成后，W17 转只读历史态：不再从商城拉取新商业事实，普通增量触发、业务新建/变更和执行写入均不可用；默认进入历史视图，不再显示第一阶段活动筛选，也不把历史任务算作当前积压。

## 7. 操作契约

### 7.1 技术运行操作

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 立即执行增量 | 页头 / 总览 | 管理员；阶段严格为 `FIRST_PHASE_MALL_OWNED` 且轮询启用；同来源无有效推进任务 | 展示当前安全水位、系统计算范围和“不修改来源” | 创建后台任务并记录审计；完成后分别展示来源捕获水位和后续映射积压 | 失败使用原任务查询/重试，水位规则不变 |
| 按单号补拉 | 差异 / 快照 / 管理员动作 | 管理员；有效来源单号；阶段严格为 `FIRST_PHASE_MALL_OWNED` | 确认来源、单号、影响和原身份 | 使用原来源身份创建单号补拉任务并记录审计 | 失败保留差异并沿原幂等身份重试；迁移时点后入口禁用且服务端拒绝 |
| 重试失败任务 | 任务 detail | 管理员；阶段严格为 `FIRST_PHASE_MALL_OWNED`；原任务属于普通一期且错误类别允许重试 | 展示原范围、原水位和错误分类 | 新增重试尝试/任务并关联原任务 | 明确失败；不手工改成功或推进水位；迁移时点后重试入口不可用 |
| 执行每日核对 | 核对子视图 | 管理员；阶段严格为 `FIRST_PHASE_MALL_OWNED`；无同边界运行任务 | 展示清单边界与比较内容 | 后台产生核对批次和逐单差异 | 同边界使用 `rerunNo` 重跑，旧证据不覆盖；迁移时点后每日核对停止，W17 转只读历史态 |
| 指派映射任务 | 映射 detail | 管理员；目标角色有业务权限 | 确认责任类型和截止 | 追加指派审计并进入责任人待办 | 失败保留原责任；管理员不能代确认 |

“立即增量”不能让用户输入或移动高水位；服务端按当前水位、重叠窗口、`safeNow` 和稳定分页形成范围，每次人工动作追加审计。来源不可用时商城继续运行，ERP 水位保持不变。按单补拉、普通失败重试和普通每日核对均是 `FIRST_PHASE_MALL_OWNED` 专属动作：迁移时点后普通增量与核对停止，即使用户保留旧页面或重放旧请求，服务端也必须拒绝；W17 转只读历史态，仅保留历史查询与证据追溯。

### 7.2 业务映射操作

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 确认映射 | 映射处理区 | 当前 `work_item` 领取人、对象版本均有效；对应唯一 `ownerRole`；目标对象类型正确且当前可用；阶段为 `FIRST_PHASE_MALL_OWNED` | 展示来源身份、ERP 目标、关系角色、依据和影响 | 同一事务追加可审计映射目标、把 `mappingTaskStatus` 置为已解决并完成当前正式待办；不立即伪称已形成销售版本 | 冲突时保留选择并刷新当前谱系，不静默覆盖；结算主体未配置责任，或阶段已封存时 fail-closed |
| 请求来源修复 | 映射处理区 | 来源事实确有缺失/矛盾；当前正式待办已领取 | 必填内部说明、所需修复内容和证据引用；明确当前不创建跨系统协同对象 | 使用 W02 非终结动作只向当前映射任务追加内部证据记录；`mappingTaskStatus` 保持 `PENDING`、任务保持 `IN_PROGRESS`，不终结、不自动下一项 | 新来源快照沿原身份到达后继续当前任务，不编辑旧快照；提交失败保留当前项并重试 |
| 转交映射责任 | 任务更多菜单 | 当前责任路由已配置但实际责任人/角色需变更；有转交权限 | 展示目标责任和原因 | 使用 W02 `TRANSFER` 动作直接更新责任人并记录审计，任务状态不变；`mappingTaskStatus` 保持待处理 | 失败保留原任务和输入；不得用来源修复动作顺带改责任人 |
| 拒绝错误候选 | 候选行 | 有映射权限 | 无正式业务动作确认；需记录依据 | 只更新本任务候选判断，不停用 ERP 对象 | 保留其它候选和输入 |
| 重新归集 | 已解决任务固定下一步 | `mappingTaskStatus=RESOLVED`；原快照仍有效；阶段为 `FIRST_PHASE_MALL_OWNED` | 展示将重用原快照和幂等身份 | 独立重新归集操作后台应用快照；成功固定展示 ERP 销售单/版本和应收结果 | 结果未知只把 `reapplyOperationStatus` 标为 `UNKNOWN` 并停留当前项；映射仍保持已解决，不得自动下一项 |
| 暂挂当前映射 | 连续处理条 | 当前正式待办已领取；输入已保存 | 必选结构化原因，可填备注；展示本轮 `queueContextId` | 使用 `DeferMappingTaskCommand` 追加非终结动作；`mappingTaskStatus` 不变，任务回到待领取状态，不写 `paused`、不完成任务。界面只按返回值移动本轮队列游标 | 提交失败停原项并保留输入；结果未知不移动游标，查询最终结果 |
| 浏览上一项 / 下一项 | 连续处理条 | 当前无脏输入；目标仍属于本轮队列快照 | 无 | 只更新 URL 和本轮队列游标，不提交任务动作、不改变待办或映射状态 | 目标失效时按服务端快照定位下一有效项；失败停原项 |

只有“确认映射”使用 W02 统一动作命令：它携带 `mappingTaskId`、`workItemId`、`expectedSubjectVersion`、映射任务版本、来源快照 ID 和当前映射谱系版本，领域映射变化与正式待办完成在同一事务提交。“请求来源修复”和“暂挂当前映射”是非终结任务动作，使用 W02 非终结动作；前者只追加当前映射任务内部证据/说明，不创建外部协同对象、外部责任人或外部状态，也不引入 `WAITING_SOURCE_FIX` 正式状态；改变责任人只使用 W02 `TRANSFER` 动作。重新归集使用独立 operation ID 和状态。映射提交结果不确定时不得在前端标记已解决；重新归集结果不确定时不回滚已经服务端确认的映射结论，且不得形成前端应收或自动跳下一项。

### 7.3 禁止动作

- 无“编辑来源快照”“修改内容指纹”“直接绑定并生成应收”“手工标记同步成功”。
- 无“将商城差异复制成 ERP 新销售单”。
- 无“T 后恢复第一期轮询”或把 T 前商城开单改由商城再次服务的入口。
- 无通用“关闭”绕过结果未知、映射未完成或未形成终态证据。

## 8. 数据契约

### 8.1 查询

```ts
type MallSyncViewName =
  | "overview"
  | "jobs"
  | "snapshots"
  | "mapping"
  | "reconciliation"
  | "history"

type MallSyncQuery = {
  sourceSystemId: string
  view: MallSyncViewName
  q?: string
  from?: string
  to?: string
  status?: string[]
  jobType?:
    | "BASELINE"
    | "INCREMENTAL"
    | "SINGLE_ORDER"
  mappingType?: string[]
  owner?: "mine" | string
  differenceType?: string[]
  jobId?: string
  snapshotId?: string
  mappingTaskId?: string
  workItemId?: string
  queueContextId?: string
  reconciliationJobId?: string
  differenceId?: string
  sort: string
  cursor?: string
  pageSize: number
}

type FirstPhaseMallSyncExecutionContext = {
  executionStage: "FIRST_PHASE_MALL_OWNED"
}

type MallSyncContext = {
  sourceSystem: { id: string; code: string; name: string; environmentLabel: string }
  ownership: {
    businessType: "VOUCHER"
    stage: "FIRST_PHASE_MALL_OWNED" | "ARCHIVED"
    originSystemSummary: "MALL" | "ERP"
    mallOwnedOrderCount?: number
    erpOwnedOrderCount?: number
    syncDirection:
      | "MALL_TO_ERP_COMMERCIAL_FACT"
      | "SEALED_HISTORY"
    firstPhasePollingEnabled: boolean
    sealedAt?: string
    finalWatermark?: string
  }
  freshness: {
    currentWatermark?: string
    latestSuccessfulJobAt?: string
    sourceSafeTime?: string
    syncLagSeconds?: number
    viewProjectedAt: string
  }
  metrics: Array<{
    key: string
    label: string
    count?: number
    value?: string
    visible: boolean
    targetView: MallSyncViewName
    targetFilter: Record<string, string>
  }>
}

type MallSyncJobRow = {
  jobId: string
  jobNo: string
  jobType: string
  rangeStart?: string
  rangeEnd?: string
  status: string
  pageCount: number
  itemCount: number
  errorCount: number
  cursorBefore?: string
  cursorAfter?: string
  startedAt: string
  finishedAt?: string
  errorClass?: string
  impactSummary?: string
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
}

type MallSnapshotRow = {
  snapshotId: string
  externalOrderNo: string
  sourceUpdatedAt: string
  observedAt: string
  sourceStatusCode: string
  sourceStatusLabel: string
  contentHashShort: string
  mappingStatus: string
  appliedSalesOrderId?: string
  appliedRevisionId?: string
  syncJobId: string
  conflictFlags: string[]
}

type MappingTaskWorkItemView = {
  workItemId: string
  workItemType: "BUSINESS_EXCEPTION"
  businessObjectType: "MASTER_MAPPING_TASK"
  businessObjectId: string
  subjectVersion: string
  status: WorkItemStatus // 直接复用 W02 的唯一正式任务状态契约
  completionAction: string
  claimedBy?: string
}

type MappingTaskViewBase = {
  mappingTaskId: string
  sourceSnapshotId: string
  externalIdentityMapId?: string
  mappingType: string
  mappingTaskStatus: "PENDING" | "RESOLVED" | "UNRESOLVABLE" | "CLOSED"
  reapplyOperation?: {
    operationId: string
    status: "QUEUED" | "RUNNING" | "SUCCEEDED" | "FAILED" | "UNKNOWN"
    lastUpdatedAt: string
  }
  sourceEvidence: Array<{ field: string; label: string; value: string; sensitive?: boolean }>
  candidateTargets: Array<{
    objectType: string
    objectId: string
    stableNo: string
    label: string
    currentRevisionId: string
    eligibility: string
    reason: string
  }>
  currentTargets: Array<{
    objectType: string
    objectId: string
    relationRole: string
    validFrom: string
    validTo?: string
    status: string
  }>
  impactSummary: string
  resolutionHistory: Array<{
    action: string
    result: string
    handledBy: string
    handledAt: string
    evidenceReference?: string
  }>
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
  lockVersion: number
}

type MappingTaskView =
  | (MappingTaskViewBase & {
      ownerRoutingState: "MISSING"
      ownerRole?: never
      ownerUserId?: never
      workItem?: never
    })
  | (MappingTaskViewBase & {
      ownerRoutingState: "CONFIGURED"
      ownerRole: "SALES" | "OPERATIONS" | "FINANCE"
      ownerUserId?: string
      workItem: MappingTaskWorkItemView
    })
```

- 所有视图由 TanStack Query 管理；Query Key 包含来源系统、阶段、权限/范围版本、子视图、筛选、`queueContextId` 以及 `mappingTaskId` / `workItemId` 等对象身份。
- 同步任务、水位、来源快照、映射任务和核对使用各自强类型查询；前端不把通用日志拼成业务状态。
- 运行中后台任务按服务器建议间隔轮询或订阅；离开页面不取消已提交任务。
- 来源快照 detail 仅返回当前角色允许的白名单字段；禁止字段不得先返回再在组件隐藏。
- 管理员的人工立即增量与按单补拉直接可用并在 `allowedActions` 返回，每次操作追加审计；系统定时增量不依赖任何人工配置。
- `MappingTaskView` 以 `ownerRoutingState` 强判别：`MISSING` 分支禁止 `ownerRole` 和 `workItem`，`CONFIGURED` 分支强制返回唯一 `ownerRole` 与正式 `workItem`；前端不得把可选字段补成可执行责任路由。
- 封存（`ARCHIVED`）后 `allowedActions` 不再返回任何同步、核对或映射写动作；历史查询与证据追溯仍可用。T 切换由服务端运营行为完成，不属于 W17 可执行工作面，契约不提供任何切换执行上下文。

### 8.2 提交

```ts
type MallSyncCommandBase = {
  sourceSystemId: string
  idempotencyKey: string
}

type ScheduledIncrementalMallSyncCommand =
  MallSyncCommandBase &
  FirstPhaseMallSyncExecutionContext &
  {
    mode: "INCREMENTAL"
    triggerSource: "SCHEDULED"
    baseCursorVersion?: number
    externalOrderNo?: never
    failedJobId?: never
    reconciliationBoundary?: never
  }

type ManualIncrementalMallSyncCommand =
  MallSyncCommandBase &
  FirstPhaseMallSyncExecutionContext & {
    mode: "INCREMENTAL"
    triggerSource: "MANUAL"
    baseCursorVersion?: number
    externalOrderNo?: never
    failedJobId?: never
    reconciliationBoundary?: never
  }

type ManualSingleOrderMallSyncCommand =
  MallSyncCommandBase &
  FirstPhaseMallSyncExecutionContext & {
    mode: "SINGLE_ORDER"
    triggerSource: "MANUAL"
    externalOrderNo: string
    failedJobId?: never
    reconciliationBoundary?: never
    baseCursorVersion?: never
  }

type FirstPhaseOperationalMallSyncCommand =
  MallSyncCommandBase &
  FirstPhaseMallSyncExecutionContext &
  (
    | {
        mode: "RETRY_FAILED_JOB"
        reason: string
        failedJobId: string
        externalOrderNo?: never
        reconciliationBoundary?: never
        baseCursorVersion?: number
      }
    | {
        mode: "RECONCILIATION"
        reason: string
        reconciliationBoundary: { asOf: string; sourceDigest?: string }
        externalOrderNo?: never
        failedJobId?: never
        baseCursorVersion?: never
      }
  )

type FirstPhaseTriggerMallSyncCommand =
  | ScheduledIncrementalMallSyncCommand
  | ManualIncrementalMallSyncCommand
  | ManualSingleOrderMallSyncCommand
  | FirstPhaseOperationalMallSyncCommand

type TriggerMallSyncCommand = FirstPhaseTriggerMallSyncCommand

type MallMappingExecutionContext = FirstPhaseMallSyncExecutionContext

type ConfirmMappingDecisionPayload = {
  mappingTaskId: string
  sourceSnapshotId: string
  externalIdentityMapId?: string
  expectedMappingTaskVersion: number
  mappingOperationId: string
  resolution: {
    type: "CONFIRM_TARGET"
    objectType: string
    objectId: string
    relationRole: string
  }
  evidenceNote: string
}

type ConfirmMappingDecision =
  ConfirmMappingDecisionPayload & MallMappingExecutionContext

type ConfirmMappingCommand = WorkItemActionCommand<ConfirmMappingDecision>

type RequestSourceFixAction = {
  type: "REQUEST_SOURCE_FIX"
  mappingTaskId: string
  sourceSnapshotId: string
  expectedMappingTaskVersion: number
  requestOperationId: string
  reasonCode: string
  reasonText: string
  requestedEvidence: string[]
}

type RequestSourceFixCommand = WorkItemActionCommand<RequestSourceFixAction>

type DeferMappingTaskAction = {
  type: "DEFER_MAPPING_TASK"
  mappingTaskId: string
  reasonCode: string
  note?: string
  queueContextId: string
  currentQueueCursor?: string
  filterDigest: string
}

type DeferMappingTaskCommand =
  WorkItemActionCommand<DeferMappingTaskAction>

type MappingWorkItemTransfer = {
  mappingTaskId: string
  targetOwnerRole: string
  targetOwnerUserId?: string
  reasonCode: string
  reasonText: string
}

type TransferMappingWorkItemCommand =
  WorkItemActionCommand<MappingWorkItemTransfer>

type ConfirmMappingBusinessResultPayload = {
  mappingTaskId: string
  mappingTaskStatus: "RESOLVED"
  externalIdentityMapId: string
  mappingTargetId: string
  recordedAt: string
}

type ConfirmMappingBusinessResult =
  ConfirmMappingBusinessResultPayload & MallMappingExecutionContext

type ConfirmMappingResult =
  WorkItemActionResult<ConfirmMappingBusinessResult>

type RequestSourceFixResult = WorkItemActionResult<{
  mappingTaskId: string
  mappingTaskStatus: "PENDING"
  mappingEvidenceEntryId: string
  recordedAt: string
}>

type DeferMappingTaskResult = WorkItemActionResult<{
  mappingTaskId: string
  queueContextId: string
  nextQueueCursor?: string
  recordedAt: string
}>

type ReapplyMallSnapshotPayload = {
  mappingTaskId: string
  sourceSnapshotId: string
  expectedMappingVersion: number
  operationId: string
  idempotencyKey: string
}

type ReapplyMallSnapshotCommand =
  ReapplyMallSnapshotPayload & MallMappingExecutionContext

type GovernanceActionResult = {
  actionId: string
  status: "ACCEPTED" | "SUCCEEDED" | "FAILED" | "UNKNOWN"
  backgroundJobId?: string
  reapplyOperationStatus?: string
  salesOrderId?: string
  salesOrderRevisionId?: string
  receivableResultReference?: string
  recordedAt: string
  nextActions: string[]
}
```

W17 直接复用 W02 的统一动作命令（`WorkItemActionCommand`）及对应结果类型，不重定义同名或私有任务命令。`completionAction` 由服务端当前 `work_item` 元数据约束；客户端不能另传任意完成动作。

- `TriggerMallSyncCommand` 以 `executionStage` 强判别：`FIRST_PHASE_MALL_OWNED` 只能使用普通增量、按单补拉、普通失败重试或普通核对；阶段已封存时同步、核对及一切执行类写命令整体拒绝。
- 定时增量只走 `ScheduledIncrementalMallSyncCommand`，不携带人工字段。人工立即增量与按单补拉只能走对应 manual 分支，每次提交追加审计；服务端按阶段校验，封存后人工动作整体拒绝。
- `executionStage` 是并发校验值，不是客户端可选择的放行开关。所有同步、确认映射与重新归集提交都先由服务端重读当前阶段；实际已封存却提交普通一期分支时必须整体拒绝，未知阶段值也按契约错误拒绝而不是忽略。
- `INCREMENTAL` 的实际查询范围由服务端根据安全水位、重叠窗口和 `safeNow` 生成；客户端不能传任意范围覆盖水位。
- 普通按单补拉必须使用协议规范化后的原来源单号，不创建新的来源身份；已封存后不存在单号补拉、普通失败重试或普通每日核对分支，旧页面请求也必须被服务端拒绝。
- 已封存（`ARCHIVED`）后服务端不再接受任何增量、补拉、重试、核对、确认映射或重新归集写命令；旧页面或重放请求一律整体拒绝，只保留历史证据查询与追溯。
- 映射目标类型、稳定身份、有效性和关系角色由服务端校验；相似候选不构成确认。
- `ConfirmMappingCommand` 同时校验 `work_item` 当前领取人和任务对象版本；追加映射结论、更新 `master_mapping_task` 与完成当前 `work_item` 必须处于同一事务。完成待办本身不能脱离该强类型事务修改业务事实。若 `executionStage=ARCHIVED`，确认映射整体不提交。
- `RequestSourceFixCommand` 只向当前 `master_mapping_task` 追加内部说明和证据，成功返回 `mappingEvidenceEntryId`，并保持任务 `IN_PROGRESS`、`mappingTaskStatus=PENDING`；不得创建外部协同对象、外部 `assignee/status`，不得写 `WAITING_SOURCE_FIX`，也不得完成、关闭、转交任务或自动下一项。新快照沿原来源身份到达后继续同一映射任务。
- `DeferMappingTaskCommand` 只追加结构化暂挂原因、备注和本轮 `queueContextId`。成功必须返回 `DeferMappingTaskResult`，任务回到待领取（`UNCLAIMED`），不写也不改变 `mappingTaskStatus`，且没有 `paused` 状态；`nextQueueCursor` 只能移动同一 `queueContextId` 内的浏览位置，不能重排正式队列或暗示任务完成。
- 若需改变责任人，必须另交 `TransferMappingWorkItemCommand`；责任人与转交审计更新服从 W02 原子语义，不写映射结论、不改变 `mappingTaskStatus`。
- 重新归集复用原快照、原销售来源身份和业务幂等约束；不得建立第二张销售单。已封存后不再接受重新归集命令；历史证据经 `view=history` 只读追溯。
- 确认映射提交 `UNKNOWN` 时保留查询前状态；来源修复请求或转交结果未知时同样不乐观改变任务/责任并停留当前项。重新归集 `UNKNOWN` 时映射仍保持 `RESOLVED`。各动作按自身操作身份查询最终结果；只有明确无结果且服务端确认安全时才重试。

### 8.3 前端边界

- 前端只格式化时间、延迟、数量、状态文案、指纹缩略和安全掩码。
- 前端不得计算/移动水位、判断迟到顺序、生成商业内容指纹、选择冲突快照胜者或推导正式销售版本；也不得因映射/重新归集失败回退已经安全持久化的来源捕获水位。
- 映射候选排序可按服务端相关性展示，但前端不能自动选择或合并。
- 来源状态映射、唯一卡券明细校验、金额/税率解析、来源系统、`allowedActions` 和正式应用结果必须采用服务端返回。
- 同步成功不等于映射成功，映射成功不等于重新归集成功，重新归集成功必须有销售版本及适用应收结果证据；来源快照 `mappingStatus`、`mappingTaskStatus`、`workItem.status` 和 `reapplyOperation.status` 分段展示，不合并成一个绿色状态，也不互相回滚。
- 第一阶段允许不一致的商城玩法和执行字段不进入差异比较；不允许不一致的商业白名单事实由服务端指纹覆盖。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 阶段 Banner、指标、子视图和 8 行表格 Skeleton | 应用壳可用 | 查询完成原位替换 |
| 刷新 | 保留旧水位和列表，标刷新中 | 已有对象可查看；提交前重新校验 | 成功更新时间，失败标旧数据 |
| 当前无任务 | 总览水位仍可见；子视图显示“当前没有待处理项” | 切换历史/其它子视图 | 新任务到达或刷新 |
| 筛选无结果 | 显示筛选摘要和清除入口 | 清除筛选 | 恢复默认视图 |
| 无数据范围 | 不展示来源总量和 0 指标 | 查看范围 / 申请权限 | 范围更新后重查 |
| 查询失败且无缓存 | `BusinessFailureState`，区分权限、网络、来源和投影故障 | 重试、返回其它模块 | 查询成功 |
| 查询失败但有缓存 | 保留旧内容，水位标陈旧/刷新失败 | 允许只读查看；动作服务端重校验 | 重试成功 |
| 人工同步动作 | 页头与差异区提供“立即增量”“按单号补拉”入口，操作追加审计 | 查看定时同步与历史水位；触发人工动作 | 服务端按阶段校验，封存后拒绝 |
| 来源商城不可用 | 明确“商城继续运行，ERP 水位未推进”及最近成功水位 | 正常一期可安全重试；已封存后不重试、不推进；业务映射旧快照仍按阶段条件处理 | 来源恢复后按对应阶段的原水位补齐 |
| 已封存后收到普通一期动作 | 入口隐藏或禁用；旧页面请求显示“W17 已封存为只读历史”，不创建后台任务 | 仅查看历史证据；不可按单补拉、普通重试或普通每日核对 | 历史证据经 `view=history` 只读追溯，当前动作转 W23/W29 |
| 后台任务运行 | 常驻进度、范围和已处理数量 | 可离开页面；禁止重复触发同来源推进任务 | 任务终态刷新 |
| 来源捕获部分失败 | 列出失败分页/对象；来源读取、白名单规范化或快照持久化未完成时明确“水位未推进” | 正常一期可安全重试/按单补拉；已封存后不续跑 | 原身份按对应阶段合法命令续跑并安全持久化成功 |
| 映射/应用失败 | 来源快照与已推进捕获水位保持不变，独立显示 `mappingTaskStatus`、责任待办、重新归集状态和业务影响 | 领取正式待办、修复映射、按原快照重新归集 | 映射与重新归集分别取得终态 |
| 映射冲突 | 左右差异和当前谱系并列，提交禁用 | 刷新候选、请求责任人确认 | 形成明确映射或来源修复 |
| 来源修复说明已记录 | 显示当前映射任务内的证据记录号、说明和证据；不显示外部责任人或外部状态；`mappingTaskStatus=PENDING`、待办保持 `IN_PROGRESS` | 继续补充内部证据；如需换当前待办责任另行转交 | 新快照沿原身份到达后继续当前映射任务 |
| 当前映射已暂挂 | 显示结构化原因和动作时间；不出现 `paused` 业务状态，`mappingTaskStatus` 不变且任务回到待领取 | 只按 `nextQueueCursor` 浏览本轮下一项 | 重新定位该任务并按统一领取规则继续 |
| 映射待办已转交 | 固定展示原任务、目标责任和后继任务；不显示映射已解决 | 返回队列、打开有权后继任务 | 后继任务被领取并继续处理 |
| 确认映射 / 重新归集成功 | `FormalActionResult` 固定展示映射/归集结果、销售单或应收引用、时间和下一步 | 打开 W05/W11、下一项 | 等待正式查询水位刷新 |
| 重新归集结果不确定 | 固定结果区，`reapplyOperationStatus=UNKNOWN`；映射结论和已完成映射待办不回滚，不自动下一项 | 按 operation ID 查询最终结果、原幂等重试（服务端允许时） | 取得销售版本/应收终态 |
| 字段级隐藏 | 来源字段标签保留、值掩码；候选按权限过滤 | 其余授权处理 | 权限更新后重查 |
| T 后（封存） | Banner 显示封存时间/最终水位，活动写动作隐藏 | 查看历史、前往 W23/W29 | 不恢复第一期写模式 |
| 权限收回 | 清除快照、候选和历史敏感值，切权限态 | 返回有权模块 | 权限恢复后重查 |

## 10. 响应式与键盘

| 视口 | 布局变化 | 保留内容 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 侧栏展开；列表与 detail 分栏；至少 6–8 行 | 阶段方向、水位、延迟、任务身份、状态、责任和主动作 | 无 |
| 1280×800 | 子视图可横滚；detail 覆盖部分列表 | 来源单号、更新时间、映射/任务状态、操作列 | 次要统计移入 detail |
| 1024×768 | 侧栏图标；映射左右 diff 保持双栏但可调宽 | 阶段 Banner、来源事实、候选身份、影响与动作 | 工具栏换行，技术摘要折叠 |
| 768×1024 | 导航抽屉；列表横滚；diff 改上下分区 | 水位、任务身份、来源/目标、状态和主动作 | 筛选收进面板，历史记录折叠 |
| 375×812 | 单列，只保证任务阅读、指派查看和简单业务确认 | 来源边界、任务身份、来源关键字段、目标身份、结果反馈 | 不提供手工触发同步、复杂 diff、多候选合并、核对大表和技术诊断 |

键盘与无障碍：

- `/` 聚焦当前子视图搜索；方向键移动表格，Enter 打开 detail。
- 映射队列支持上一项/下一项；切换后焦点落新任务标题，并播报“第 N/M 项、映射类型、来源单号”。
- 左右 diff 字段以相同标题关联，增删改有文字，不只用红绿背景。
- 正式结果、后台进度和筛选数量使用 `aria-live=polite`；错误与结果未知使用明确警示语义。
- 关闭 detail/确认框后焦点回触发源；从 W05 返回聚焦原快照或映射任务。
- 触控目标至少 44px；移动端不把复杂技术动作缩成难以辨认的图标按钮。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 今日工作台 / 待办 | W01 / W02 | `workItemId`、`mappingTaskId`、`queueContextId` | 处理后返回原队列位置或自动下一项 |
| 客户 / 结算主体 | W03 | 客户或结算主体候选/目标稳定 ID；不传相似结论 | 返回 W17 重新查询候选和唯一 `ownerRole` |
| 合同中心 | W04 | 合同候选/目标稳定 ID 与允许的当前修订 | 返回 W17 重新校验合同有效性 |
| 销售单 | W05 | 来源销售单身份、ERP `salesOrderId/revisionId`、差异任务 | 返回聚焦原快照；W05 明示未归属/来源状态 |
| 卡券票款复核 | W13 | 已形成销售单、复核状态 | 映射未完整时不进入错误应收复核 |
| 基础资料 | W14 | 卡券类目 SKU 候选稳定 ID/版本 | 返回后重新校验类目目标有效性 |
| 导入与期初 | W18 | 期初基线批次、来源身份 | 基线完成后持续同步沿用同一来源身份 |
| 执行投影 | W23 | T 后 ERP 销售版本和商城接收状态 | W17 只保留第一阶段历史，不处理当前投影 |
| 接口错误与对账 | W29 | 二期通用消息/错误/对账身份 | 返回 W17 时不混入第一阶段专用核对状态 |
| 权限与审计 | W19 | 来源、任务、快照、映射或处理动作身份 | 返回原 detail |

跨工作面只传稳定身份、任务/队列上下文和证据引用。来源字段、状态、权限及正式结果由目标页重新查询。

## 12. 验收清单

### 12.1 来源与同步正确性

- [x] 页面始终明确来源（商城开单）、同步方向及 ERP/商城各自可写边界。
- [x] 第一阶段只允许商城 → ERP 商业事实同步，ERP 不向商城回写商业修改。
- [ ] 来源读取、规范化或快照安全持久化失败时捕获水位不推进；异步映射/重新归集失败保留差异但不回退已安全捕获水位，也不阻塞商城执行。
- [x] 同步失败或差异处理不存在“在 ERP 手工补建销售单”入口。
- [ ] 同一来源快照重复处理幂等；迟到、同刻冲突和 A→B→A 均按服务端证据展示。
- [ ] 按单补拉、普通失败重试和普通每日核对只在 `FIRST_PHASE_MALL_OWNED` 可用；T 起普通增量与核对停止，W17 转只读历史态，旧页面或重放请求也不能恢复。
- [ ] 管理员可直接执行“立即增量”和“按单号补拉”，操作留审计；定时增量不受影响；封存后人工动作整体拒绝。
- [ ] `TriggerMallSyncCommand` 以阶段强判别：`FIRST_PHASE_MALL_OWNED` 分支只允许普通增量、按单补拉、失败重试与核对；已封存后全部执行类命令整体拒绝，不允许以旧页面或重放绕过。
- [ ] T 切换完成后 W17 进入只读历史态（封存）：不再从商城拉取新商业事实，历史证据保留可查；当前动作正确导向 W23（执行投影）与 W29（通用错误对账）。

### 12.2 映射与核对

- [x] 系统管理员只能补拉、重试、指派和排障，不能替业务角色确认映射。
- [ ] 每个已完成责任路由、需要人工处理的映射差异同时具有 `master_mapping_task` 与正式 `work_item` 双身份；路由缺失时只保留领域差异且不可执行。配置后统一领取生效，映射结论与待办完成在同一事务，转交可追溯。
- [ ] 只有确认映射使用 W02 统一动作命令；请求来源修复只以非终结动作追加当前映射任务内部说明/证据，保持任务 `IN_PROGRESS`，不创建外部协同对象、外部责任或 `WAITING_SOURCE_FIX`；改变责任人只使用 W02 `TRANSFER` 动作。
- [ ] 暂挂使用 `DeferMappingTaskCommand`，只记录结构化原因/备注/队列上下文；不改 `mappingTaskStatus`、不写 `paused`、不完成任务，本轮队列游标严格采用服务端结果。
- [ ] 结算主体只有一个服务端配置的 `ownerRole`；未配置时确认动作阻断，不会向销售和财务同时生成可完成待办。
- [x] `MappingTaskView` 按 `ownerRoutingState` 强判别：`MISSING` 不含 `ownerRole/workItem`，`CONFIGURED` 必含唯一 `ownerRole/workItem`。
- [x] 映射处理清楚展示来源事实、ERP 候选、当前谱系、业务影响和确认依据。
- [ ] 差异解决后使用原快照和原来源身份重新归集，不创建重复销售单。
- [ ] 已封存后确认映射与重新归集不可执行，普通一期写命令整体 fail-closed，不从历史态恢复任何写动作。
- [ ] 未完成映射不得形成错误应收、收入或经营归属。
- [ ] 每日核对比较完整内容指纹而非仅状态/金额，并只产生差异和任务。
- [ ] 映射状态与重新归集操作状态独立；重新归集成功有 ERP 销售单、版本及适用应收结果，结果未知不回滚已解决映射、不自动完成/下一项。

### 12.3 安全、状态与响应式

- [x] 页面和接口不返回玩法、卡号、卡密、绑定手机号、连接信息或接口密钥。
- [ ] 来源快照、候选对象、指标、导出和历史均按角色、数据范围和字段权限过滤。
- [x] §9 全部状态通过组件或浏览器验证，尤其覆盖来源不可用、部分失败、冲突、结果未知和封存。
- [ ] 1440、1280、1024、768、375 五档视口符合 §10；移动端只保留安全的简单确认。
- [ ] 键盘和读屏可完成筛选、比较差异、确认、自动下一项和返回焦点恢复。

## 13. 待确认事项

| ID | 问题 | 影响 | 建议决策人 | 当前建议 |
| --- | --- | --- | --- | --- |
| Q1 | 业务映射任务的默认 SLA 和超期升级路径如何按类型区分？ | 工作台预警、排序和管理指标 | 销售/运营/财务负责人 | 客户合同类 1 个工作日，类目/唯一明细类 4 小时；最终按实际运营量校准 |
| Q2 | 结算主体映射的唯一 `ownerRole` 固定为销售还是财务？ | 责任路由、正式待办和 `allowedActions` | 销售负责人 + 财务负责人 | 倾向销售确认业务关系；正式决定写入服务端配置前，结算主体确认保持阻断。税率/金额口径固定由财务确认，技术解析失败由管理员排障 |
| Q3 | 人工“立即增量”和单号补拉是否需要在服务端二次确认？ | 管理员操作效率和风险 | 系统负责人 + 审计负责人 | 管理员直接可执行，操作追加审计；禁止手工水位编辑 |
| Q4 | 来源修复请求是否需要在 ERP 内形成对商城责任人的外部协同状态？ | 映射任务状态和跨系统沟通 | 运营负责人 + 系统负责人 | 决策前只允许在当前映射任务内追加说明和证据，不创建外部协同对象、外部责任/状态或 `WAITING_SOURCE_FIX`；新快照到达后继续原任务 |
| Q5 | 第一阶段封存后 W17 历史数据默认保留多长时间、哪些角色可导出？ | 存储、审计和权限 | 审计/合规负责人 + 系统负责人 | 同步任务、快照谱系、映射和核对证据长期可查；原始受控报文按独立安全策略，不开放普通导出 |

确认结论应写回运行规则、权限配置和本文件对应章节，并从本表移除。

## 14. 业务依据

- `erp-phase-1.md` §3–§4、§8：商城开单与同步、ERP 主动轮询完整商业快照、水位/幂等、映射失败和每日全量核对；§11：系统管理员与业务部门职责。
- `erp-phase-2.md` §3.1、§8.5、§13：T 切换后 ERP 全面服务、第一阶段轮询封存、消息幂等和通用对账边界。
- `erp-data-model.md` §6.1、§6.13、§8.4：来源身份谱系、同步任务/水位/快照、映射任务、核对及事务不变量；§6.21 为二期通用错误/对账（W29）边界。
- `erp-mall-data-mapping.md` §1–§3、§9–§11：兼容层隔离、来源身份、白名单快照、转换、两期方向、运行验收和 T 切换封存。
- `erp-ui-design.md` §3.3–§3.5、§4.4、§4.8、§5.7、§10–§11：权限、TaskTabs、M3/M7、同步映射路径、键盘和状态契约。
- `erp-ui-flows.md` §9–§10：跨角色映射协同，以及 T 切换后执行投影和错误治理的工作面分工。

