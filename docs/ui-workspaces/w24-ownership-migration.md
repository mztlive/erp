# W24 · 销售单主责迁移批次

> 状态：草稿<br>
> 页面模式：M7 治理与导入<br>
> 主要路由：`/governance/ownership-migrations`、`/governance/ownership-migrations/:batchId`、`/governance/ownership-migrations/cutover`<br>
> 主要角色：系统管理员、上线负责人；销售和财务分别确认清单<br>
> 最后更新：2026-08-01

## 1. 定位与目标

### 1.1 用户目标

W24 是第二期上线窗口的一次性治理工作面。不同角色应在同一事实链上完成：

1. 按客户查看正式存量卡券销售单迁移范围和阻塞原因；
2. 销售确认客户、销售单与商业版本清单，财务独立确认票款相关清单；
3. 在维护窗口内看到商城 B2B 建单关闭、商业字段只读和业务事实冻结状态；
4. 由上线负责人确认最后同步水位、最终权威基线、卡实例与初始余额基线；
5. 由系统管理员执行客户批次，并在失败后使用原批次幂等续跑；
6. 明确看到批次是全部提交还是全部未提交，不把逐项检查进度误认为部分迁移成功；
7. 全部批次和固定切换检查通过后，由上线负责人原子登记唯一消费回流启用时间 `T`；
8. 随时确认成功迁移不会换单号、复制销售单或改变应收、回款和发票。

### 1.2 业务目标

- 只把已生效及之后状态、且未作废的正式存量卡券销售单主责从 `MALL` 单向迁为 `ERP`。
- 商城草稿在关闭 B2B 建单入口后统一作废，不进入迁移清单、不在 ERP 补建。
- 每个批次只覆盖一个客户；批次内所有迁移项在一个原子事务中提交，任一项失败时全批回滚。
- 主责迁移只改 `sales_order.owner_system` 并追加迁移审计，不生成新单号、新销售单或新销售版本。
- 迁移成功后不把任何销售单回退为商城主责，不恢复一期轮询，不重开商城 B2B 建单入口。
- 以唯一且不可修改的 `T` 划分历史人工履约与 ERP 自动供应商履约。

### 1.3 不在本工作面完成

- 不修改客户、合同、卡券类目、销售单、票款或卡实例基线；阻塞项进入其主责工作面修复。
- 不提供“手工改主责”“逐行强制成功”“跳过失败项后提交”或“回退商城主责”。
- 不把 W24 做成通用导入器；主责迁移不能使用普通后台任务的部分成功语义。
- 不恢复商城草稿、不在 ERP 补建商城草稿。
- 不通过 W24 编辑商城玩法、卡号、卡密、绑定、余额或历史发卡过程。
- 不在 W24 直接修复接口和对账差异；进入 W17 或 W29。
- 不把固定切换检查扩展成另一套自动化阶段退出平台；只展示统一数据模型规定的证据链。

## 2. 用户、权限与数据范围

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 系统管理员 | 批次总览 | 指定来源商城与全部迁移客户 | 创建批次、发起检查、执行迁移、原批次续跑 |
| 上线负责人 | 切换总览 | 本次切换全部批次和固定证据 | 组织维护窗口、确认最终权威基线、登记唯一 `T` |
| 销售 | “我的客户清单” | 本人负责/协作客户及其迁移销售单 | 查看清单与阻塞；只有 Q2 已固化且服务端将本人路由为该客户唯一确认人时才可确认 |
| 销售领导 | 团队清单 | 授权团队客户 | 只读复核；只有 Q2 已固化且服务端明确路由为唯一确认人时才可确认，领导身份本身不放行动作 |
| 财务 | 待财务确认清单 | 授权客户及票款摘要 | 独立确认财务分面；不能替代销售或基线确认 |
| 运营 | 只读切换状态 | 获授权卡券类目与商城 | 查看执行投影基线和商城只读/冻结状态 |
| 运维 | 维护窗口与运行证据 | 技术运行范围 | 查看同步、接口和检查证据；不能替代业务确认或执行人 |
| 普通业务用户 | 全局维护提示 | 仅当前受影响范围摘要 | 只读了解为何暂时不能新建/变更 |

### 2.1 职责分离

| 责任 | 唯一责任角色 | 不可替代者 |
| --- | --- | --- |
| 销售清单确认 | Q2 固化后由服务端责任模式确定的唯一确认人；未配置时无人可确认 | 财务、管理员、上线负责人，以及未被路由的销售/销售领导 |
| 财务清单确认 | 确定的财务确认人 | 销售、管理员、上线负责人 |
| 最终权威基线确认 | 上线负责人 | 销售、财务、管理员、运维 |
| 客户批次执行 | 系统管理员 | 销售、财务、上线负责人 |
| 唯一 `T` 登记 | 上线负责人 | 系统管理员和运维不能代签 |

服务端必须分别返回每项动作的 `allowedActions` 和阻塞原因。界面不得用“管理员”这一角色名推断其可以替代业务确认。

### 2.2 权限与数据范围

| 情况 | 页面行为 |
| --- | --- |
| 无 W24 管理权限 | 不展示技术批次页；业务用户仍可从维护 Banner 查看授权摘要 |
| 业务用户无客户范围 | “我的客户清单”显示无数据范围，不显示全局批次数量 |
| 无票款字段权限 | 保留财务确认状态；金额和票款明细掩码 |
| 非上线窗口 | 页面可只读准备与检查；执行类动作由服务端禁用并说明 |
| 权限在页面打开期间变化 | 立即清除敏感清单、确认按钮和证据引用，重新获取当前动作 |

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 查看迁移总览 | 系统管理侧栏 | `/governance/ownership-migrations` | 返回恢复商城、客户、状态和阻塞筛选 |
| 查看我的客户是否在清单 | 全局维护 Banner / W03 | `?view=my-customers&customer={customerId}` | 返回客户中心聚焦原客户 |
| 查看客户批次 | 总览行、确认待办 | `/governance/ownership-migrations/:batchId?stage=scope` | 返回总览保留筛选 |
| 销售 / 财务确认 | W01 / W02 待办 | 打开同一批次并定位 `stage=confirmations&workItemId={workItemId}`；URL 只保存任务身份，`claimToken` 留在当前会话内存 | 处理后回待办或留在批次 |
| 基线确认 | 维护窗口待办 | 同一批次 `stage=baseline` | 返回批次总览 |
| 执行 / 失败续跑 | 管理员待办 | 同一批次 `stage=execution` | 保留批次身份和操作结果 |
| 查看商城级切换 | 总览页头 | `/governance/ownership-migrations/cutover?mall=:mallId` | 返回总览不丢筛选 |
| 刷新浏览器 | 任一阶段 | 恢复批次、阶段、筛选；不恢复确认 Dialog | 当前稳定对象 |

批次页签身份为 `ownership-migration:{batchId}`，标题为 `迁移 · {batchNo}`；商城切换页签身份为 `ownership-cutover:{mallId}`。同一批次重复打开只聚焦已有页签。确认内容没有本地可编辑草稿，阶段切换不产生新页签。

### 3.1 全局维护 Banner

冻结生效期间，所有受影响工作面顶部显示 `MaintenanceBanner`：

- 维护开始时间、来源商城、受影响业务范围；
- 当前阶段和责任角色；
- 被冻结的动作：本次范围内的卡券销售新建/变更、制卡、卡实例与初始余额登记、支付、取消、退款、完成和余额恢复；
- “查看进度”进入 W24 的只读摘要；
- 不展示“忽略”“暂时关闭”或绕过冻结入口。

Banner 只描述服务端冻结事实，不由浏览器时间或本地开关推断。

## 4. 页面布局

### 4.1 总览页（1440×900）

```text
┌ MaintenanceBanner（冻结期间全局显示）
├ PageHeader：主责迁移                    来源商城 [切换检查] [刷新]
├ MetricStrip：[待准备客户] [待销售确认] [待财务确认] [待基线确认]
│               [可执行批次] [执行失败·仍冻结] [已完成]
├ 状态总览：一期同步水位 | 冻结状态 | 已迁移/总客户 | 已迁移/总销售单 | T 状态
├ ListToolbar：商城 | 客户 | 批次状态 | 阻塞类型 | 确认状态 | 搜索
├ BusinessTableFrame
│ 批次号（固定） | 客户 | 销售单数 | 销售确认 | 财务确认 | 冻结
│ 基线确认 | 批次状态 | 阻塞/失败 | 最后更新 | 操作（固定）
└ 分页 / 商城级切换检查摘要
```

### 4.2 批次向导（1440×900）

```text
┌ PageHeader object-chrome：主责迁移 › 批次号                [返回总览] ─┐
├ DocumentHeader compact：客户名 [批次状态] · 批次号 · 商城 · 冻结提示
├ ImportStageIndicator：范围清单 → 双确认 → 冻结与最后同步 → 最终基线 → 执行 → 完成
├ 固定摘要：scopeHash · 当前销售版本摘要 · 票款摘要 · 卡实例/余额基线 · 最后水位
├ 当前阶段主区
│  ├ 范围清单：合格项 / 阻塞项 / 不在范围（商城草稿、已作废）
│  ├ 双确认：销售与财务独立卡片、对象摘要、确认人和失效原因
│  ├ 基线：最终全量核对、迁移执行基线、卡实例与初始余额基线
│  └ 执行：整体进度、原子提交说明、失败原因与原批次续跑
├ 迁移项表：销售单 | 当前版本 | 唯一明细 | 映射 | 票款 | 基线 | 结果
└ FormalActionResult / AuditTimeline
```

### 4.3 商城级切换页

```text
┌ PageHeader：消费回流启用 · {商城}             T：尚未登记 / 2026-…
├ 必要前提：目标批次覆盖 | 一期轮询封存 | B2B入口关闭 | 商业字段只读
├ 固定检查链（每个 check_code）：当前链尾状态 | 对象摘要 | 证据 | 检查人/时间
├ 范围摘要：全部目标销售单、已完成客户批次、migrationScopeDigest
└ [登记唯一启用时间 T]（仅全部条件通过时） / 固定正式结果
```

### 4.4 区域说明

| 区域 | 目的 | 主组件 | 是否固定 |
| --- | --- | --- | --- |
| 维护 Banner | 让所有受影响用户理解冻结 | `MaintenanceBanner` | 全局固定 |
| 阶段指示 | 明确当前可做和不可做的阶段 | `ImportStageIndicator` | 批次顶部固定 |
| 范围摘要 | 防止确认后范围静默变化 | `DocumentSummary` / 指纹摘要 | 阶段切换时固定可见 |
| 独立确认卡 | 保持销售、财务、基线职责分离 | `ApprovalDecisionPanel` 只读确认形态 | 否 |
| 阻塞与差异 | 按责任对象分组修复 | `ImportIssueTable` `BusinessDiffPanel` | 否 |
| 执行进度 | 呈现后台执行而不声称部分提交 | `BackgroundJobProgress` | 执行阶段固定 |
| 正式结果 | 固定记录确认、执行或 `T` 结果 | `FormalActionResult` | 动作后主区顶部 |

## 5. 展示内容与字段

### 5.1 批次身份与范围

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 身份 | `batchNo` | 迁移批次 | 迁移批次 | 永久编号 | 有批次查看权限可见 |
| 范围 | `sourceMallName` | 来源商城 | `source_mall_id` | 单一来源商城 | 按商城范围 |
| 范围 | `customerId` / `customerName` | 客户 | 批次唯一客户 | 一批只含一位客户 | 按客户数据范围 |
| 范围 | `eligibleCount` / `blockedCount` | 可迁移 / 阻塞 | 服务端预检 | 商城草稿不计入可迁移 | 业务用户仅看本人客户 |
| 指纹 | `scopeHash` | 当前范围摘要 | 服务端规范化摘要 | 短摘要 + 可复制；变化使相关确认失效 | 受权确认角色可见 |
| 状态 | `status` | 批次状态 | 迁移批次 | 准备、已冻结、基线已确认、执行中、完成、失败 | 可见 |
| 冻结 | `freezeStartedAt` / `freezeActive` | 维护冻结 | 服务端维护状态 | 失败时仍明确“冻结中” | 全体受影响用户可见摘要 |
| 水位 | `lastSyncWatermark` | 最后一期同步水位 | 最终同步结果 | 原始游标按权限展示，业务页显示时间/批次摘要 | 技术详情限管理员/上线负责人 |

### 5.2 三类确认

| 确认 | 展示字段 | 对象范围 | 数据来源 | 口径 / 新鲜度 | 权限规则 | 失效条件 |
| --- | --- | --- | --- | --- | --- | --- |
| 销售清单确认 | 确认人、时间、`salesSubjectHash`、客户/销售单/当前版本摘要 | 客户、正式销售单、当前商业版本和映射 | W05/W17 正式身份、版本与映射快照 | 使用批次固定 `scopeAsOf`，不读取确认后的新快照 | 销售确认人仅见授权客户；管理员见状态不见无权商业值 | 销售单范围、版本、客户/合同/类目映射变化 |
| 财务清单确认 | 确认人、时间、`financeSubjectHash`、应收/回款/发票摘要 | 同批销售单及票款分面 | W11 正式票款事实快照 | 金额明确含税，使用 `financeAsOf` 与同批范围 | 财务确认人可见授权金额；其他角色只见确认状态 | 票款事实、销售范围或财务摘要变化 |
| 最终权威基线 | 确认人、时间、`baselineSubjectHash`、最后水位 | 最终 ERP 当前版本、迁移执行基线、卡实例和初始余额基线 | W17 最后同步、全量核对与服务端基线登记 | 展示唯一来源水位和 `baselineAsOf` | 仅上线负责人可确认；管理员只能执行同步/核对并保存技术证据，不能代签 | 冻结后基线、同步水位、映射或事实摘要变化 |

确认失效时保留原确认审计，但状态明确显示“范围已变化，需重新确认”；不得静默把旧确认应用到新 `scopeHash`。

### 5.3 迁移项

| 字段 | 用户文案 | 数据来源 | 展示规则 |
| --- | --- | --- | --- |
| `salesOrderNo` / `sourceOrderNo` | ERP 销售单 / 来源单号 | 销售单、来源身份 | 两者并列；迁移不换号 |
| `salesOrderStatus` | 当前状态 | 销售单 | 只包含已生效及之后、未作废正式单 |
| `beforeOwnerSystem` / `afterOwnerSystem` | 主责变化 | 迁移项 | 固定 `福利商城 → ERP` |
| `baselineSalesOrderRevisionId` | 最终 ERP 版本 | 迁移项 | 基线确认不产生新版本 |
| `baselineProjectionRevisionId` | 迁移执行基线 | 投影修订 | 是第一份执行投影修订，不是新销售版本 |
| `singleVoucherLineCheck` | 唯一卡券明细 | 预检 | 必须恰好一条；失败阻断全批 |
| `mappingChecks` | 客户 / 合同 / 类目映射 | 预检 | 任一未完成即阻断 |
| `cardBaselineCheck` | 卡实例 / 初始余额基线 | 预检 | 所有会被事实引用的卡实例均需覆盖 |
| `itemStatus` | 项目结果 | 迁移项 | 仅在全批事务提交后显示“已迁移”；检查通过不等于已提交 |
| `errorSummary` | 失败原因 | 脱敏错误 | 不展示原始报文和内部堆栈 |

### 5.4 商城级 `T` 与固定检查

页面显示统一模型规定的固定 `checkCode`：

```text
PRODUCT_PUBLICATION · SALES_PROJECTION · MALL_FACT_INTAKE
SUPPLIER_ORDER · SUPPLIER_REJECTION · AFTER_SALES_CANCEL
MALL_REFUND · CARD_BALANCE_RESTORATION · SUPPLIER_REFUND
COST_FINALIZATION · SUPPLIER_SETTLEMENT · PAYABLE_LINKAGE
MANUAL_EXCEPTION · RECONCILIATION · BACKFILL_CAPABILITY
PHASE1_POLLING_STOPPED · MALL_B2B_ENTRY_CLOSED · MALL_COMMERCIAL_FIELDS_READONLY
```

每项展示当前链尾的通过/不通过状态、`subjectHash`、证据引用、检查人、检查时间和所替代的上一检查。旧通过、失败或过期证据不删除、不覆盖；只有未被后继引用的链尾代表当前证据。

`T` 展示 `enabledAt`、`enabledBy`、`migrationScopeDigest` 和 `confirmationDigest`。一经登记不可修改或删除。

## 6. 搜索、筛选、排序与默认视图

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 来源商城 | 本次目标商城 | `mall` | 切换商城时清除不适用批次筛选 |
| 客户 | 全部有权客户 | `customer` | 支持客户名、客户编号精确/模糊查询 |
| 批次状态 | 未完成与失败 | `status` | 已完成默认折叠，可显式查看 |
| 确认状态 | 全部 | `confirmation` | 待销售、待财务、待基线、确认失效 |
| 阻塞类型 | 全部 | `blocker` | 映射、唯一明细、票款、卡实例/余额、同步水位等 |
| 我的客户 | 业务角色默认开启 | `view=my-customers` | 服务端按客户负责人/协作关系过滤 |
| 排序 | 阶段风险优先 | `sort=risk.desc,customer.asc` | 失败仍冻结 → 可执行 → 确认失效 → 待确认 → 完成 |

- 总览指标和列表使用同一数据范围版本，不用当前页求总数。
- 商城草稿和已作废销售单可以在“排除说明”中显示数量与原因，但不进入迁移选择、批次项目或正式迁移统计。
- W24 不提供普通跨客户批量选择；创建批次必须明确唯一客户。一个客户需要拆多个批次时，各批有独立编号和范围摘要。
- 迁移项表支持按检查结果筛选，但不允许勾选部分项目执行。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 创建客户批次 | 总览页头 | `CREATE_BATCH`；唯一客户、正式范围已预检 | `BatchImpactPreview` 展示纳入/排除/阻塞 | 创建 `PREPARING` 批次和固定范围摘要 | 重复请求返回既有批次；不重复纳入销售单 |
| 重新预检范围 | 批次范围阶段 | `RECHECK_SCOPE`；尚未执行 | 无正式确认 | 更新检查和 `scopeHash`；自动使不匹配确认失效 | 保留旧确认审计和变化说明 |
| 销售确认清单 | 确认卡 | `CONFIRM_SALES`；Q2 的销售确认责任模式已配置；当前用户是该客户唯一确认人；销售分面无阻塞；持有 `OWNERSHIP_MIGRATION_SALES_CONFIRMATION` 正式任务的有效租约 | 展示责任模式版本、客户、销售单数、版本、对象摘要与任务指纹 | 同一事务记录确认人、时间、责任模式版本和销售对象指纹，并完成对应 `work_item` | 责任模式未配置时不生成销售确认任务并返回 `SALES_CONFIRMATION_ROUTING_UNCONFIGURED`；结果未知查询同一操作，范围变化或租约失效需刷新 |
| 财务确认清单 | 确认卡 | `CONFIRM_FINANCE`；财务分面无阻塞；持有 `OWNERSHIP_MIGRATION_FINANCE_CONFIRMATION` 正式任务的有效租约 | 展示票款摘要、对象指纹与岗位分离 | 同一事务独立记录财务确认并完成对应 `work_item` | 不影响销售确认；结果未知查询同一确认操作，不能另行标记任务完成 |
| 启动维护冻结 | 商城切换 / 批次阶段 | 服务端 `START_FREEZE`；责任与窗口已确认 | 高风险确认列出全部冻结动作 | 写服务端冻结事实，全局 Banner 生效 | 失败不得显示已冻结；成功后无本地绕过 |
| 执行最后一期同步 | 基线阶段 | 管理员权限、冻结已生效 | 展示来源水位 | 记录最后水位与全量核对任务 | 失败保持冻结，进入 W17/W29 修复后原任务续跑 |
| 确认最终权威基线 | 基线阶段 | 上线负责人；冻结、最后同步、全量核对、映射和卡实例基线完成 | 展示完整对象摘要与责任声明 | 记录基线确认；为每项关联迁移执行基线，不产生销售版本 | 结果未知查询确认记录；摘要变化使确认失效 |
| 执行迁移批次 | 执行阶段 | 系统管理员；三类确认有效且对象摘要仍一致 | 高风险确认：本批全部提交或全部不提交 | 后台执行后原子更新全批主责并写审计 | 任一项失败全批未提交，保持冻结 |
| 原批次续跑 | 失败结果区 | `RESUME_BATCH`；失败已修复、原批次仍冻结 | 展示原批次、原范围和重新预检结果 | 使用原批次幂等身份再次原子执行 | 仍失败继续冻结；不得新建替代批次规避审计 |
| 登记唯一 `T` | 商城级切换页 | 上线负责人；全部批次完成、一期轮询封存、固定检查链尾全部通过且摘要一致 | 展示不可回退影响、唯一时间点和范围摘要 | 同一事务写唯一 `enabledAt`、操作者与确认摘要 | 结果未知查询切换记录；不得再次登记或修改时间 |
| 查看阻塞对象 | 迁移项 / 差异 | 有目标 W 权限 | 无 | 打开主责工作面并携带稳定对象 | 返回后重新预检，不自动确认或执行 |

### 7.1 不可逆与失败契约

- 单个批次中即使 99 项检查通过、1 项失败，也不能显示“已迁移 99 项”；全批事务提交前所有项均是未迁移。
- 批次执行失败后，页面固定显示“本批未提交，维护冻结仍有效”，并提供原批次续跑。
- 其它已完成客户批次不因本批失败而回退。
- 页面不存在“恢复商城主责”“重开商城 B2B 建单”“恢复一期轮询”或“修改 `T`”动作。
- 确认、执行和 `T` 登记均携带唯一请求 ID；请求超时先查询最终记录，不重复创建批次或第二个切换。
- `CONFIRM_SALES`、`CONFIRM_FINANCE` 不接受仅含 `batchId` 的直接对象命令；无论从 W01/W02 还是批次页进入，都必须先定位、领取对应正式任务，并使用 W02 完整任务信封提交。
- 普通 `background_job` 只登记总体运行进度和安全结果，不赋予主责迁移部分成功语义。

## 8. 数据契约

### 8.1 总览查询

```ts
type OwnershipMigrationListQuery = {
  mallId: string
  customerIds?: string[]
  statuses?: string[]
  confirmationStates?: string[]
  blockerCodes?: string[]
  view?: "all" | "my_customers"
  q?: string
  sort: string[]
  page: number
  pageSize: number
}

type OwnershipMigrationBatchRow = {
  batchId: string
  batchNo: string
  sourceMallId: string
  sourceMallName: string
  customerId: string
  customerName: string
  scopeHash: string
  status: string
  freezeActive: boolean
  eligibleCount: number
  blockedCount: number
  migratedCount: number
  salesConfirmation: ConfirmationSummary
  financeConfirmation: ConfirmationSummary
  baselineConfirmation: ConfirmationSummary
  lastSyncWatermark?: string
  errorSummary?: string
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
  updatedAt: string
}
```

`migratedCount` 只有批次完成后才可等于批次项目数；执行中或失败批次不得展示暂存事务内计数作为正式成功数。

### 8.2 批次详情查询

```ts
type OwnershipMigrationBatchView = {
  identity: {
    batchId: string
    batchNo: string
    sourceMallId: string
    customerId: string
  }
  status: string
  scopeHash: string
  freeze: { active: boolean; startedAt?: string; scopeLabel: string }
  counts: { total: number; eligible: number; blocked: number; migrated: number }
  confirmations: {
    sales: ConfirmationSummary
    finance: ConfirmationSummary
    baseline: ConfirmationSummary & { lastSyncWatermark?: string }
  }
  checks: Array<{
    code: string
    status: "PASSED" | "BLOCKED" | "NOT_RUN"
    subjectHash: string
    summary: string
    destinationWorkspaceId?: string
    objectId?: string
  }>
  items: Array<{
    itemId: string
    salesOrderId: string
    salesOrderNo: string
    sourceIdentityId: string
    sourceOrderNo: string
    salesOrderStatus: string
    beforeOwnerSystem: "MALL"
    afterOwnerSystem: "ERP"
    baselineSalesOrderRevisionId?: string
    baselineProjectionRevisionId?: string
    checkResults: Record<string, string>
    itemStatus: string
    errorSummary?: string
  }>
  backgroundOperation?: {
    operationId: string
    status: string
    progressLabel: string
    startedAt?: string
    lastProgressAt?: string
  }
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
  objectVersion: string
  queriedAt: string
}

type ConfirmationSummary = {
  state: "MISSING" | "VALID" | "INVALIDATED"
  confirmedBy?: string
  confirmedAt?: string
  subjectHash?: string
  invalidatedReason?: string
}
```

### 8.3 商城切换查询

```ts
type ConsumptionCutoverView = {
  cutoverId: string
  mallId: string
  status: "PREPARING" | "ENABLED"
  migrationScopeDigest: string
  coveredBatchCount: number
  coveredSalesOrderCount: number
  enabledAt?: string
  enabledBy?: string
  confirmationDigest?: string
  checks: Array<{
    checkCode: string
    checkNo: number
    checkStatus: "PASSED" | "FAILED"
    subjectHash: string
    evidenceReference: string
    supersedesCheckId?: string
    checkedBy: string
    checkedAt: string
    isCurrentTail: boolean
  }>
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
  objectVersion: string
}
```

### 8.4 提交

```ts
type OwnershipMigrationConfirmationDecision = {
  batchId: string
  action: "CONFIRM_SALES" | "CONFIRM_FINANCE"
  expectedScopeHash: string
  comment?: string
}

// 直接复用 W02 CompleteWorkItemEnvelope；expectedSubjectVersion 对应批次版本，
// expectedSubjectHash 对应当前销售/财务确认分面的任务内容指纹。
type OwnershipMigrationConfirmationCommand =
  CompleteWorkItemEnvelope<OwnershipMigrationConfirmationDecision>

type MigrationFormalCommand = {
  batchId?: string
  cutoverId?: string
  action:
    | "START_FREEZE"
    | "CONFIRM_BASELINE"
    | "EXECUTE_BATCH"
    | "RESUME_BATCH"
    | "ENABLE_CUTOVER"
  expectedObjectVersion: string
  expectedScopeHash?: string
  expectedSubjectHash?: string
  requestId: string
}

type MigrationFormalResult = {
  operationId: string
  batchId?: string
  cutoverId?: string
  status: "COMMITTED" | "NOT_COMMITTED" | "RUNNING" | "RESULT_UNKNOWN"
  batchStatus?: string
  enabledAt?: string
  committedAt?: string
  nextAction: string
}
```

- `EXECUTE_BATCH` 与 `RESUME_BATCH` 必须锁定批次并在一个原子事务中重新校验所有项、更新主责和写审计。
- `ENABLE_CUTOVER` 必须锁定商城切换记录，重算全部批次覆盖、轮询封存、检查链尾和摘要后一次性登记 `T`。
- `OwnershipMigrationConfirmationCommand` 必须完整携带 W02 的 `workItemId`、`claimToken`、`leaseVersion`、`expectedSubjectVersion`、`expectedSubjectHash` 和 `idempotencyKey`。服务端在同一事务重读批次与确认分面、校验任务租约/版本/指纹、追加确认事实与 `workflow_action` 并完成该 `work_item`；成功响应同时返回确认结果和任务完成结果，前端不得补发“标记完成”。
- 任一校验失败返回 `NOT_COMMITTED`，不能返回项目级部分提交。
- 确认命令同时提交当前对象摘要；服务端重新计算不一致时拒绝。
- 所有表单/确认使用 TanStack Form 或正式确认组件；TanStack Query 管理查询与失效。

### 8.5 前端边界

- 前端不计算迁移资格、确认有效性、`scopeHash`、检查链尾、批次覆盖或 `T`。
- 进度百分比只表示后台操作进度，不表示迁移项已正式提交。
- 前端可格式化水位、时间和数量，不把缺失检查显示成失败通过，也不把旧通过证据当当前通过。
- 维护 Banner、冻结动作和可执行条件完全采用服务端返回。
- 页面不能缓存完整票款、卡实例或地址数据；W24 不需要卡号、卡密和绑定手机号。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 总览、向导或切换页按成稿显示 Skeleton | 应用壳导航可用 | 查询完成原位替换 |
| 刷新 | 保留批次与冻结提示，显示数据水位 | 只读；正式动作前服务端重验 | 成功更新时间，失败保留旧数据 |
| 无批次 | 展示“尚未建立迁移批次”和准备条件 | 有权角色创建客户批次 | 创建后进入向导 |
| 筛选无结果 | 展示筛选摘要 | 清除筛选 | 恢复未完成视图 |
| 无数据范围 | 业务用户显示无客户范围；不泄露全局迁移量 | 查看角色/申请范围 | 权限更新后重查 |
| 查询失败 | 保留全局冻结 Banner；内容失败态可重试 | 返回其它模块、重试 | 查询恢复 |
| 数据陈旧 | 明确最后水位；冻结状态以独立高优先级查询为准 | 刷新 | 状态追平 |
| 字段级隐藏 | 票款、技术水位或证据按权限掩码 | 对应有权确认仍按服务端决定 | 权限更新后重查 |
| 准备中有阻塞 | 向导停在范围阶段，按责任分组问题 | 打开主责工作面、重新预检 | 阻塞修复后更新 `scopeHash` |
| 确认失效 | 保留旧确认审计，显著标记失效原因 | 对应角色重新确认 | 新对象摘要确认成功 |
| 三类确认结果不确定 | 销售、财务或基线确认不本地标已确认，固定展示操作号和对应指纹 | 查询原确认记录 | 明确确认已写入或未提交 |
| 已冻结 | 全局 Banner 生效；所有受影响写动作阻断 | 最后同步、基线核对 | 不允许浏览器绕过 |
| 基线待确认 | 展示最后水位和未通过检查 | 处理差异、由上线负责人确认 | 全部条件满足 |
| 执行中 | 总体后台进度；项目不显示正式“已迁移” | 查询操作结果 | 全批提交或全批未提交 |
| 执行失败且仍冻结 | 固定结果“本批未提交”，展示脱敏原因 | 修复、原批次续跑 | 原批次成功提交 |
| 批次完成 | 全部项目显示已迁移，展示审计与不可回退说明 | 查看切换总览 | 等待其它客户批次 |
| 切换检查未通过 | 列出当前链尾及责任方 | 进入对应 W 修复/复检 | 新链尾全部通过 |
| `T` 登记结果未知 | 不开放自动履约，不允许再次登记 | 查询切换记录 | 明确已启用或未提交 |
| `T` 已登记 | 固定展示唯一时间、操作者与范围摘要 | 只读查看 | 不可修改/删除 |
| 权限收回 | 清除清单、票款和证据详情；保留通用维护 Banner | 返回有权模块 | 恢复后重查 |

## 10. 响应式、键盘与无障碍

| 视口 | 布局变化 | 保留内容 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 侧栏展开；阶段向导、摘要和迁移项同屏 | 冻结、客户、三确认、基线、批次结果、T | 无 |
| 1280×800 | 侧栏可折叠；表格次要检查列收起 | 批次身份、阶段、确认、阻塞、操作 | 审计摘要折叠 |
| 1024×768 | 图标侧栏；向导纵向；详情覆盖 | 维护 Banner、阶段、对象摘要、正式结果 | 迁移项细节进展开行 |
| 768×1024 | 导航抽屉；步骤改纵向时间线；表格横滚 | 批次/客户、冻结、三确认、阻塞、执行结果 | 技术水位和证据引用折叠 |
| 375×812 | 只读维护摘要和“我的客户清单” | 冻结原因、当前阶段、客户是否在范围、结果 | 不提供创建批次、确认基线、执行迁移或登记 T |

- 阶段指示必须使用有序列表/步骤语义，当前、完成、阻塞和未开始均有文字。
- Tab 顺序：维护说明 → 阶段 → 范围摘要 → 当前阶段动作 → 问题表 → 审计。
- 高风险确认打开时焦点落在标题，关闭后返回触发按钮；正式结果通过 `aria-live=polite` 播报一次。
- 执行状态更新不自动把焦点从当前阅读位置移走；全批明确结果后焦点落到结果标题。
- 迁移项表支持键盘展开问题，身份列与结果列固定；色彩不是确认或失败的唯一表达。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 今日工作台 / 待办 | W01 / W02 | `workItemId`、`batchId`、确认角色 | 返回聚焦原确认任务 |
| 客户中心 | W03 | `customerId`、迁移批次 ID | 返回保留客户与批次筛选 |
| 销售单中心 | W05 | `salesOrderId`、基线版本 | 只修复基础资料/版本问题；返回后重新预检 |
| 客户往来 | W11 | 客户、应收/回款/发票摘要 | 财务修复后回批次重新确认 |
| 商城同步与映射 | W17 | 最后同步任务、水位、映射差异 | 处理后回 W24，不自动推进阶段 |
| 权限与审计 | W19 | 操作者、角色、审计记录 | 返回原批次 |
| 执行投影 | W23 | `baselineProjectionRevisionId`、销售版本 | 只读核对迁移执行基线 |
| 商城消费订单 | W25 | 唯一 `cutoverId` / `T`、履约链说明 | `T` 只作为服务端事实，不由 W25 改写 |
| 接口错误与对账 | W29 | 同步/检查差异、错误任务 | 修复后 W24 重新检查当前链尾 |
| 历史消费回填 | W30 | `cutoverId`、固定范围 `[rangeStart,T)` | `T` 后才能正式启动回填 |

跨工作面只传稳定身份和检查上下文；修复完成不会自动替用户做销售、财务或基线确认。

## 12. 验收清单

### 12.1 范围与职责

- [x] 迁移范围只包含已生效及之后、未作废的正式存量卡券销售单。
- [x] 商城草稿明确显示为不迁移、不补建，不会进入批次项目或统计。
- [x] 每个批次只含一个客户；同批所有项目属于该客户。
- [x] 销售、财务和最终基线三类确认由各自责任角色独立完成，管理员不能代签。
- [ ] 销售与财务确认均通过 W02 完整任务信封提交；确认事实、`workflow_action` 和对应任务完成在同一事务落地。
- [x] `scopeHash` 或相应分面变化会使旧确认失效，并保留审计。

### 12.2 冻结、基线与执行

- [x] 冻结期间所有受影响工作面显示不可忽略的维护 Banner。
- [x] 最终基线确认只在冻结、最后同步和全量核对后可用。
- [x] 基线登记不生成新销售版本；迁移执行基线明确标注为第一份投影修订。
- [x] 迁移成功只改变主责标记，不换单号、不复制销售单、不改变应收、回款和发票。
- [x] 批次任一项失败时全批未提交，界面不出现部分成功语义。
- [x] 失败保持冻结并使用原批次续跑；其它完成批次不回退。

### 12.3 唯一 T 与不可回退

- [x] 全部目标客户批次完成、一期轮询封存和固定检查链尾全部通过前，无法登记 `T`。
- [x] 旧、失败、过期或已被替代的检查证据不能被当成当前通过。
- [x] `T` 以商城为粒度原子登记，结果未知时先查询，不创建第二个切换。
- [x] `T` 一经登记不可修改或删除。
- [x] 页面不存在恢复商城主责、重开商城 B2B 建单或恢复一期轮询的动作。
- [ ] `T` 前支付只回填台账，`T` 及以后支付才进入自动供应商履约；界面不按接收时间判断。

### 12.4 状态、权限与响应式

- [x] 无模块权限、无客户范围、无批次、筛选无结果、确认失效和字段掩码可区分。
- [x] 执行进度不会被表达为正式项目成功数。
- [ ] 权限收回后不残留票款、卡实例、技术证据或完整来源身份。
- [ ] §9 所有状态均完成组件或浏览器验收。
- [ ] 1440、1280、1024、768、375 五档视口符合 §10，手机不提供高风险执行动作。

## 13. 待确认事项

| ID | 问题 | 影响 | 建议决策人 | 当前建议 |
| --- | --- | --- | --- | --- |
| Q1 | 谁拥有“启动维护冻结”和最终解除维护展示的系统动作权限？ | 全局 Banner、责任审计和上线手册 | 上线负责人 + 系统负责人 | 上线负责人发起、系统管理员执行技术动作；服务端保存双角色审计 |
| Q2 | 销售清单确认由每位负责销售完成，还是按客户由销售领导统一确认？ | 待办数量、确认主体和对象摘要 | 销售负责人 | 责任模式未配置时不生成销售确认任务、无人可确认；确认后每个客户只路由一名正式确认人，协作销售或领导身份不能自行覆盖 |
| Q3 | 单个大客户需要拆成多个批次时的容量上限和拆分规则是什么？ | 原子事务时长、批次编号和范围摘要 | DBA / 后端负责人 + 上线负责人 | 压测后固定上限；每批仍是同一客户的独立不可变范围 |
| Q4 | 维护窗口在任何批次尚未执行前能否整体取消，取消需要哪些审批？ | 预执行恢复路径和审计 | 上线负责人 + 业务负责人 | 只允许执行前按正式应急方案取消；一旦任何批次完成即不可回退 |
| Q5 | 固定切换检查的证据引用保留期限和可查看角色如何配置？ | 审计、合规和字段权限 | 安全负责人 + 运维负责人 | 证据长期留摘要；原始文件受控保留并短时授权查看 |

## 14. 业务依据

- `erp-phase-2.md` §8.5：迁移范围、商城权限、切换准备、最终权威基线和失败续跑。
- `erp-phase-2.md` §13.1、§13.3、§13.4：迁移幂等、主责对账与监控。
- `erp-phase-2.md` §15 P2-P05/P2-P06、§16、§17.2：W24 页面、角色职责和验收场景。
- `erp-data-model.md` §6.16：客户批次、三类对象摘要、原子迁移、执行基线与不可回退约束。
- `erp-data-model.md` §6.17 `mall_consumption_cutover`：固定检查链、唯一不可变 `T` 和原子启用。
- `erp-data-model.md` §7.8、§8.4、§10：迁移状态机、切换事务不变量和能力启用矩阵。
- `erp-mall-data-mapping.md` §10.2：冻结、最终同步、按客户迁移、停止轮询和唯一 `T`。
- `erp-ui-design.md` §3.1、§3.4–§3.5、§4.8、§6、§9–§11、§15：维护 Banner、TaskTabs、M7、后台任务和状态契约。
- `erp-ui-flows.md` §10.3：清单 → 基线确认 → 执行 → 失败续跑，以及业务用户只读查询。
