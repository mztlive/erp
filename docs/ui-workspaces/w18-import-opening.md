# W18 · 导入与期初

> 状态：已确认
> 页面模式：M7 治理与导入
> 主要路由：`/governance/imports`、`/governance/imports/:batchId`
> 主要角色：系统管理员；各迁移对象责任部门的确认人
> 最后更新：2026-08-01

## 1. 定位与目标

### 1.1 用户目标

- 系统管理员能创建导入批次、上传合规白名单包、观察校验与后台应用进度，并准确定位失败行。
- 销售、采购、运营、仓储和财务确认人只核对自己负责的对象、口径和试算影响，不需要接触技术文件或其它部门数据。
- 所有人都能明确区分“文件已接收”“校验通过”“业务已确认”和“正式数据已形成”，避免把上传成功误认为导入完成。

### 1.2 业务目标

- 以 `legacy_import_batch`、`legacy_import_row` 和 `background_job` 记录可追溯、可续跑的导入过程。
- 期初迁移先在验证环境完成校验和业务确认，再进入生产环境；生产应用由后台任务逐项执行。
- 成功对象、失败对象、规则版本、manifest、结果和来源谱系可审计，同时严格执行文件隔离与保留策略。
- 导入只按正式业务命令创建对象或期初事实，不直接覆盖已存在的正式单据、库存余额、票款或历史修订。

### 1.3 不在本工作面完成

- 不在浏览器内解析原始 SQL、数据库连接头或含禁止字段的商城导出；这类文件只能在受控临时区清洗。
- 不用“重新上传一份 Excel”覆盖已经形成的正式事实；冲突进入映射、基础资料修订、库存调整或财务纠错流程。
- 不在 W18 逐单补录卡券期初已收、已开票金额；这些值初始化为 0，逐单复核进入 W13。
- 不迁移历史实物与服务销售单、采购单、履约明细、期初应收应付或实体卡库存。
- 不替代 W17 的商城持续同步、T 切换（商城停单、ERP 全面服务）和 W30 的历史消费回填。
- W18 仅承载治理型批量导入与期初迁移；正式业务主路径必须在对应对象工作面完成，禁止以导入替代正常业务流程。供应商日常商品 Excel 等非治理型导入不在本工作面范围。
- 正常导入业务确认/退回必须使用唯一固定 `work_item_type=IMPORT_BUSINESS_CONFIRMATION`，
  `business_object_type=LEGACY_IMPORT_BATCH`、`handlerKey=import_business_confirmation`、
  `destinationWorkspaceId=W18`、唯一领域命令 `CompleteImportBusinessConfirmation`。
  `BUSINESS_EXCEPTION` 只承载异常，禁止伪装为正常必经确认，禁止上线页面私有类型。
- 每个 `batchId × confirmationScope` 最多一个有效确认任务；
  `confirmationScope` 与 `ownerRole` 共同决定责任范围。服务端必须保持真实批次为
  `businessObjectId`，并把已注册 `confirmationScope` 冻结为 `responsibilityKey`；不得伪造
  批次身份。后端 handler、W01/W02 展示映射和 W18 处理器未完成接线前，运行时确认入口必须 fail-closed（禁用）。

## 2. 用户、权限与数据范围

### 2.1 角色与责任

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 系统管理员 | 导入批次列表 | 被授权环境内全部批次；敏感列仍按字段权限掩码 | 创建批次、上传、启动校验、提交应用、查看进度、下载合规结果 |
| 销售确认人 | W01/W02 分派的 `IMPORT_BUSINESS_CONFIRMATION` 进入指定批次 | 客户、合同及销售负责的卡券销售单试算 | 确认或退回责任范围结果；handler 未接线前入口必须禁用 |
| 采购确认人 | 待办进入指定批次 | 供应商、能力、资质、公司商品池（公司 SKU 集合） | 确认或退回责任范围结果 |
| 运营确认人 | 待办进入指定批次 | 卡券类目及对应映射结果 | 确认或退回责任范围结果 |
| 仓储确认人 | 待办进入指定批次 | 仓库和期初库存实盘试算 | 确认基准日、仓库、SKU 和数量 |
| 财务确认人 | 待办进入指定批次 | 卡券期初应收派生试算和票款初始化口径 | 确认应收派生；不能在此把历史票款改为已收/已开 |
| 审计人员 | W19 链入只读批次 | 授权审计范围 | 查看批次动作、规则版本、确认人和下载记录 |

表中销售、采购、运营、仓储和财务的“待办进入”都使用同一固定类型；每项任务以
`confirmationScope + ownerRole` 分责，且服务端固定 `responsibilityKey=confirmationScope`；
禁止客户端提供责任维度，禁止因某个角色已有页面权限而跳过任务或版本校验。
各对象集合必须由对应责任部门分别确认；生产环境的期初库存确认与期初应收确认必须保留独立责任人，禁止同一人兼任库存确认与应收确认。
handler 与展示映射未完成接线前，所有正常业务确认入口必须统一禁用。

生产环境提交应用必须采用发起人与复核人分离；验证环境允许单人执行校验与业务确认。系统管理员只负责技术编排，禁止替代责任部门作业务确认。

### 2.2 权限表达

| 情况 | W18 行为 |
| --- | --- |
| 无模块权限 | 不展示侧栏与命令入口；直接访问显示无权限页 |
| 有模块权限但无环境数据范围 | 展示“当前角色无可管理环境”，不显示虚假的空批次 |
| 只负责业务确认 | 只打开被指派批次的“试算与确认”只读视图，不显示上传、应用和文件下载动作 |
| 无敏感字段权限 | 保留列和字段标签，值掩码；下载结果按当前字段权限重新生成或拒绝 |
| 批次进行中权限被收回 | 清除已加载诊断明细与短时下载链接；后台任务不因前端断开而取消，后续执行仍逐项重验系统授权 |
| 批次状态不允许动作 | 动作保留并禁用，展示服务端 `actionBlockers`，不得由前端猜测状态流转 |

生产应用必须同时满足模块权限、环境权限、批次版本、全部必要确认、发起/复核分离（仅生产）和文件安全检查。验证环境确认结果仅当规则版本、manifest 与数据更新时间均未变化且仍在批准窗口内时，方可作为生产应用前置；任一变化或超过批准窗口，必须重新校验与业务确认，禁止沿用过期验证结论。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 查看批次 | 系统导航“导入与期初” | `/governance/imports?environment=validation&status=active`；列表页签身份为当前用户 + 环境 | 返回保留筛选、页码和滚动位置 |
| 创建批次 | 列表主动作 | 在同一列表打开创建 Dialog；创建成功后打开批次任务页签 | 关闭批次页签返回原列表 |
| 打开批次 | 行、全局搜索或待办 | `/governance/imports/:batchId?section=overview`；身份为 `import-batch:{batchId}` | 聚焦已存在页签，不创建副本 |
| 处理业务确认 | W01/W02 的 `IMPORT_BUSINESS_CONFIRMATION` 待办 | 打开同一批次页签并定位 `section=confirm&confirmationScope={scope}&workItemId={workItemId}&queueContextId={queueContextId}`；URL 只保存任务/队列身份；handler 未接线时返回实施 blocker | 完成后回原待办或留在批次结果 |
| 查看问题行 | 批次问题统计 | URL 写入 `issueCode`、`objectType`、`page`，刷新可恢复 | 后退回试算摘要 |
| 查看审计 | 批次“审计”入口 | 新开 W19 页签并携带批次稳定 ID | 返回聚焦 W18 批次页签 |

批次阶段、当前子区、问题筛选和页码进入 URL；任务入口额外保存 `confirmationScope`、
`workItemId` 与 `queueContextId`。上传进度、临时浮层和本地文件选择不跨刷新恢复。尚未提交的批次说明表单属于脏状态，关闭页签必须确认；文件一旦安全接收成功，其服务端资产身份不依赖浏览器页签。业务确认即使从批次页直接打开，也必须定位对应 `work_item`；`DIRECT` 任务直接处理，`POOL` 任务先执行“开始处理”。页面不得绕过导入确认强类型命令直接写确认事实。

## 4. 页面布局

### 4.1 批次列表

```text
┌ PageHeader：导入与期初        环境：验证环境      [新建导入批次]
├ MetricStrip：待校验 | 待业务确认 | 执行中 | 失败/部分失败
├ ListToolbar：对象集 | 状态 | 发起人 | 基准日 | 时间 | 搜索批次号
├ BusinessTableFrame
│ 批次号 | 环境 | 对象集合 | 基准日 | 阶段 | 进度 | 责任确认 | 发起人 | 更新时间 | 操作
└ 分页
```

### 4.2 批次中心

```text
┌ PageHeader object-chrome：导入与期初 › 批次号              [返回列表] ─┐
├ DocumentHeader compact：来源系统名 [批次状态]                           │
│  批次号 · 版本 · 验证/生产 · 基准日 · 当前阶段 · 规则版本               │
│ 数据更新时间 / 后台任务状态                       [阶段主动作]
├ ImportStageIndicator
│ 安全接收 → 结构校验 → 业务校验与试算 → 责任确认 → 后台应用 → 结果
├ 锚点：概览 | 文件与规则 | 试算与问题 | 责任确认 | 执行进度 | 结果 | 审计
├ 当前阶段主区
│ 左：汇总、依赖、影响预览或问题表
│ 右：阶段说明、确认人、阻塞原因和下一步
└ FormalActionResult / BackgroundJobProgress（固定结果区）
```

### 4.3 区域说明

| 区域 | 目的 | 主组件 | 是否固定 |
| --- | --- | --- | --- |
| 批次身份头 | 防止在错误环境或错误基准日操作 | `PageHeader object-chrome` + `DocumentHeader density="compact"` `BusinessStatusBadge` | 顶部固定 |
| 阶段条 | 明确“已上传”不等于“已入账” | `ImportStageIndicator` | 视口内固定 |
| 试算摘要 | 展示对象数量、冲突、业务影响和依赖 | `MetricStrip` `BusinessDiffPanel` | 当前阶段首屏 |
| 问题明细 | 按错误码、对象和行列定位合规诊断 | `ImportIssueTable` | 服务端分页 |
| 责任确认 | 按部门分离确认结论 | `ApprovalDecisionPanel` / 确认卡 | 仅待确认阶段可写 |
| 执行进度 | 观察后台逐项结果，不模拟同步完成 | `BackgroundJobProgress` | 执行阶段常驻 |
| 结果区 | 固定呈现正式结果、失败范围和后续入口 | `FormalActionResult` | 完成后常驻 |

## 5. 展示内容与字段

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 批次头 | `batchNo` | 导入批次号 | `legacy_import_batch` | 稳定编号，可全局搜索 | 有批次查看权可见 |
| 批次头 | `environment` | 验证环境 / 生产环境 | 批次执行上下文 | 使用显著文字和 tone，生产不可仅靠颜色 | 按环境数据范围 |
| 批次头 | `sourceObjectSet` | 本批对象集合 | 批次 | 固定对象代码映射业务文案 | 只返回有权对象族 |
| 批次头 | `baselineDate` | 期初基准日 | 批次 | 公司已确认业务日期 | 确认人可见，不可前端改写 |
| 文件 | `fileName`、`byteSize` | 合规输入包 | `file_asset` | 只展示白名单包元数据 | 原始存储键、签名 URL 不展示 |
| 文件 | `securityScanStatus` | 安全检查 | `file_asset` | 待扫描、通过、拒绝、隔离 | 拒绝原因脱敏 |
| 文件 | `contentHmac` | 文件审计指纹 | `file_asset` | 默认仅展示短指纹和 HMAC 密钥版本 | 无安全审计字段权时隐藏值 |
| 规则 | `importRuleVersion` | 导入规则版本 | 批次 | 固定版本号，可查看变更说明 | 不允许临时切换已确认批次规则 |
| 试算 | `totalRows/successRows/failedRows` | 总数 / 可应用 / 问题 | 批次和后台任务 | 服务端聚合，不按当前页求和 | 按对象范围裁剪 |
| 问题 | `sourceRowNo/sourceColumnName` | 位置 | `background_job_item` / `legacy_import_row` | 只保存合规诊断位置 | 禁止回显已清理敏感原值 |
| 问题 | `errorCode/errorDetail` | 问题类型 / 处理建议 | 导入校验器 | 固定错误码 + 脱敏业务文案 | 技术堆栈不展示 |
| 映射 | `mappingStatus`、`externalIdentity` | 映射状态 / 来源身份 | `external_identity_map` | 原值按权限显示，不能大小写折叠后冒充同一身份 | 敏感标识掩码 |
| 确认 | `confirmationScope/result/confirmedBy/At` | 责任部门确认 | 确认记录 / `workflow_action` | 对象范围、结论、意见、时间 | 各部门只能写自身范围 |
| 进度 | `processed/success/skipped/failed` | 后台应用进度 | `background_job` | 服务端计数 + 最近进度时间 | 有批次查看权可见 |
| 结果 | `resultObjectType/Id` | 已形成对象 | 逐项结果 | 稳定对象引用，可打开对应 Wxx | 重新校验对象权限 |
| 保留 | `retentionClass/expiresAt` | 结果保留至 | `file_asset` | 失败诊断 30 天；导出结果 7 天；成功审计资产长期 | 下载前再次鉴权 |

### 5.1 期初对象口径提示

W18 只处理上线期初或治理型迁移。采购日常收到的供应商供给 Excel 不进入 W18，
必须在 W21 按已有公司 SKU 导入供给；该导入不会自动创建公司 SKU，
公司商品池只是公司 SKU 的查询视图。

| 对象 | 界面必须提示的口径 |
| --- | --- |
| 客户、合同 | 有效客户；仍在执行或仍有应收的合同 |
| 供应商、能力、资质 | 全部在用供应商；资质含有效期和合规附件 |
| 公司商品池（公司 SKU 集合）、卡券类目 | 全部启用对象；停用对象按批准清单导入 |
| 仓库与期初库存 | 全部在用仓库；库存只取统一基准日实盘数量，不导入历史流水 |
| 卡券销售单 | 商城已生效及之后状态且未作废的正式单据；草稿不迁移 |
| 卡券期初应收 | 由正式销售单成交金额派生；已收、已开票初始化为 0，后续进入 W13 逐单复核 |

## 6. 搜索、筛选、排序与默认视图

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 环境 | 验证环境 | `environment` | 生产环境切换需显著环境提示并重置选择 |
| 批次状态 | 未结束 | `status` | 可多选阶段；部分失败单独可筛 |
| 对象集合 | 全部有权对象 | `objectType` | 按固定对象类型筛选，不接受任意表名 |
| 基准日 | 全部 | `baselineFrom/baselineTo` | 仅业务日期范围 |
| 批次搜索 | 空 | `q` | 精确/前缀匹配批次号，不搜索文件正文 |
| 问题类型 | 全部 | `issueCode` | 服务端按固定错误码聚合与分页 |
| 问题范围 | 仅失败 | `rowStatus` | 可切换待映射、冲突、失败、跳过 |
| 排序 | 最近更新优先 | `sort` | 后台进度变化不打乱用户当前选中项 |

1440×900 的列表首屏至少展示 6 条批次；问题表默认 36px 行高。指标可点击并写入筛选摘要，纯统计项不得带 hover 或手型。列表工具栏常驻「清除筛选」，清全部筛选参数并回第 1 页，与批次详情一致（列表/详情清除行为不再分裂）。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 创建批次 | 列表主动作 | `CREATE_IMPORT_BATCH`；明确来源系统、对象集、环境和基准日 | 创建前展示范围摘要 | 返回稳定批次号并打开批次中心 | 保留输入；重复请求按 `requestId` 返回同一批次 |
| 上传合规包 | 文件阶段 | `UPLOAD_IMPORT_PACKAGE`；批次仍可接收文件 | 提示允许格式、禁止内容和保留策略 | 文件进入安全扫描，阶段仍为“安全接收” | 断点/重传不创建重复资产；拒绝文件显示脱敏原因 |
| 启动校验 | 阶段主动作 | 扫描通过、manifest 和规则版本完整 | 确认对象集与规则版本 | 创建后台校验任务并固定输入资产 | 结果未知时按请求 ID 查询，不重复创建任务 |
| 重新校验 | 问题修复后 | 原批次可修复且规则未失效 | 展示将失效的旧试算和确认 | 新校验版本形成，旧确认全部失效 | 旧结果保留审计；失败停在当前阶段 |
| 业务确认 | 责任确认卡 | 当前用户是当前责任人；试算版本与任务对象版本一致 | 展示对象数量、关键口径和影响摘要 | 同一事务追加确认事实与 `workflow_action`、完成对应 `work_item`；全部必要确认后允许提交应用 | 冲突时刷新最新试算并重新取得责任，不覆盖他人结论 |
| 退回修复 | 责任确认卡 | 当前用户是当前责任人；试算版本与任务对象版本一致 | 原因必填 | `RETURN_FOR_FIX` 作为本次试算确认的正式 `REJECTED` 结论：同一事务记录结构化退回事实、`workflow_action` 并完成当前 `work_item`；不转交、不创建本任务后继 | 输入保留；提交失败保留输入并查询结果。修复并形成新试算版本后，再创建新的确认任务 |
| 提交生产应用 | 阶段主动作 | 生产权限、验证环境结果仍在批准窗口内且规则/manifest/数据未变、全部确认、发起/复核分离、无阻塞问题、版本一致 | `FormalActionConfirmDialog` 二次确认环境、基准日、对象数和不可覆盖规则 | 启动关联后台应用任务，显示固定任务号和进度 | 结果未知停留并“查询最终结果”；不得再次创建批次 |
| 取消后台任务 | 进度区 | 服务端允许取消且仅有未开始项 | 展示已提交事实不会回滚 | 仅停止未开始项，结果显示“部分完成/已取消” | 取消请求不确定时查询任务状态 |
| 重跑失败项 | 结果区 | 使用原批次或显式修复批次；失败范围冻结 | 展示冻结目标、当前版本和将跳过项 | 创建修复任务；已成功对象按来源身份幂等跳过 | 版本/权限变化逐项失败并保留原因 |
| 下载结果 | 结果区 | 当前仍有对象和字段权限，资产未过期 | 敏感导出可要求用途说明 | 返回短时链接并记录下载审计 | 链接过期重新鉴权生成；不可沿用旧链接 |

任何“提交应用”都必须由服务端逐项重验权限、数据范围、状态、来源身份和预期版本。批量预览或发起时权限摘要仅用于审计，禁止替代执行时鉴权。

单批文件大小、行数与问题诊断下载上限必须由服务端按对象集配置；超过上限必须拆批后分别创建批次。禁止在浏览器内分片上传后冒充单一批次；禁止前端自行放宽服务端容量上限。

## 8. 数据契约

### 8.1 查询

```ts
type ImportBatchQuery = {
  environment: "VALIDATION" | "PRODUCTION"
  status?: string[]
  objectTypes?: string[]
  baselineFrom?: string
  baselineTo?: string
  q?: string
  page: number
  pageSize: number
  sort: "UPDATED_DESC" | "CREATED_DESC" | "BATCH_NO_ASC"
}

type ImportBatchView = {
  batchId: string
  batchNo: string
  environment: "VALIDATION" | "PRODUCTION"
  sourceSystem: { id: string; name: string }
  sourceObjectSet: string[]
  baselineDate: string
  importRuleVersion: string
  stage: "RECEIVE" | "VALIDATE" | "TRIAL" | "CONFIRM" | "APPLY" | "RESULT"
  status: string
  inputAsset?: SafeFileAssetView
  metrics: { total: number; valid: number; conflict: number; failed: number }
  confirmations: ImportConfirmationView[]
  activeConfirmationWorkItem?: {
    workItemId: string
    taskVersion: string
    workItemType: "IMPORT_BUSINESS_CONFIRMATION"
    subjectVersion: string
    assignmentMode: "DIRECT" | "POOL"
  }
  backgroundJob?: BackgroundJobView
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
  version: string
  updatedAt: string
}

type ImportIssueQuery = {
  batchId: string
  issueCodes?: string[]
  rowStatuses?: Array<"PENDING_MAPPING" | "CONFLICT" | "FAILED" | "SKIPPED">
  cursor?: string
  pageSize: number
  sort: "ROW_ASC" | "SEVERITY_DESC" | "ISSUE_CODE_ASC"
}

type ImportIssuePage = {
  rows: ImportIssueRowView[]
  nextCursor?: string
  totalCount: number
  issueVersion: string
  queriedAt: string
}
```

- 列表、批次、问题和进度均由 TanStack Query 管理；执行中必须按服务端指定间隔轮询或订阅，页面隐藏后降低频率。
- Query Key 必须包含用户、权限版本、环境、批次身份、筛选和问题版本；从待办进入时还必须包含
  `workItemId`、`confirmationScope` 和 `queueContextId`，避免把不同责任确认复用为同一视图缓存。
- 问题行服务端分页；响应不返回禁止保留的原始 SQL、完整敏感原值、存储对象键或内部堆栈。

### 8.2 责任确认任务与提交

#### 固定登记

| 项目 | 固定值 / 规则 |
| --- | --- |
| `work_item_type` | `IMPORT_BUSINESS_CONFIRMATION` |
| `business_object_type` | `LEGACY_IMPORT_BATCH` |
| `business_object_id` | 真实 `batchId`；不得追加范围后缀或生成替代 ID |
| `responsibility_key` | 服务端固定为已注册 `confirmationScope`，创建后不可变，客户端请求不包含该字段 |
| 任务粒度 | 开放唯一索引按 `businessObjectType + businessObjectId + workItemType + responsibilityKey` 约束；同一 `batchId × confirmationScope` 最多一个开放任务，不同范围可并存 |
| `handlerKey` / 去向 | `import_business_confirmation` / `W18` |
| 唯一领域命令 | `CompleteImportBusinessConfirmation` |
| 正式 decision | `CONFIRM_SCOPE` 或 `RETURN_FOR_FIX`；两者都经同一强类型命令完成当前任务 |
| 重试与新任务 | 修复并产生新的试算版本后才创建新任务；旧任务保持 `COMPLETED` 并可审计 |

本表为跨层强制契约。W01/W02 必须展示受控 handler；W18 必须按分派模式建立责任并提交导入确认强类型命令。责任池任务展示“开始处理”，本人责任任务展示“确认本范围”和“退回修复”；显式任务深链上下文不一致时必须失败关闭。

#### 提交

```ts
type ImportBusinessConfirmationContext = {
  batchId: string
  expectedBatchVersion: string
  expectedTrialVersion: string
  confirmationScope: string
}

type ImportBusinessConfirmationDecision =
  | (ImportBusinessConfirmationContext & {
      action: "CONFIRM_SCOPE"
      comment?: string
    })
  | (ImportBusinessConfirmationContext & {
      action: "RETURN_FOR_FIX"
      reasonCode: string
      comment?: string
    })

type ImportBusinessConfirmationCommand = {
  workItemId: string
  expectedTaskVersion: string
  expectedSubjectVersion: string
  decision: ImportBusinessConfirmationDecision
  idempotencyKey: string
}

type ImportExecutionCommand = {
  batchId: string
  expectedBatchVersion: string
  expectedTrialVersion?: string
  action: "START_APPLY" | "CANCEL_PENDING" | "RETRY_FAILED"
  reasonCode?: string
  comment?: string
  requestId: string
}

type ImportExecutionResult = {
  action: ImportExecutionCommand["action"]
  resultStatus: "STARTED" | "CANCELLED" | "RETRY_PREPARED" | "UNKNOWN"
  batchId: string
  batchStatus: "importing" | "partial_failed" | "failed" | "ready_to_apply"
  batchVersion: string
  trialVersion?: string
  backgroundJobId: string
  backgroundJobStatus: string
  backgroundJobVersion: string
  affectedItems: number
  nextStep: "MONITOR_PROGRESS" | "REVIEW_RESULT" | "START_APPLY"
  auditReceipt: string
}
```

- 创建、校验、确认、应用、取消和重跑分别使用独立 `requestId` 防重复，重复请求返回同一结果。
- `ImportBusinessConfirmationCommand` 只由 `IMPORT_BUSINESS_CONFIRMATION` 的
  `import_business_confirmation` handler 实现，是该任务唯一强类型完成命令，
  携带 `workItemId`、`expectedTaskVersion`、`expectedSubjectVersion` 和 `decision`。
  `BUSINESS_EXCEPTION` 仅承载异常，不允许代替正常确认任务。
- 服务端在同一事务校验当前责任人、`confirmationScope + ownerRole` 和对象版本，并写确认或退回事实、`workflow_action` 与当前任务 `COMPLETED` 终态；前端不得随后再调用“标记任务完成”。`RETURN_FOR_FIX` 是 `REJECTED` 完成结论，不是转交、退回团队或关闭。
- 服务端还必须校验任务的不可变 `responsibilityKey` 等于命令中经固定注册表规范化后的
  `confirmationScope`；不一致时失败关闭，不得依赖 `ownerRole` 猜测或覆盖责任范围。
- 业务确认响应同时返回 `resultStatus = CONFIRMED | REJECTED | UNKNOWN`、正式确认结果、当前任务完成结果、批次新版本和下一步；不得在同一动作响应里转交原任务或创建后继任务。最后一项确认只推进到 `ready_to_apply` 并返回 `nextStep=START_APPLY`，不得启动后台任务。
- 导入执行固定调用 `POST /admin/legacy-import-batches/{batchId}/commands`。HTTP 请求使用 `batch_id`、`expected_batch_version`、`expected_trial_version`、`reason_code`、`request_id` 等 snake_case 字段，路径与请求体批次必须一致，未知字段失败关闭。
- `START_APPLY` 是进入 `importing` 并启动后台任务的唯一动作；`CANCEL_PENDING` 只取消尚未应用项且必须填写原因；`RETRY_FAILED` 只重置失败行并返回 `nextStep=START_APPLY`，不得在同一动作自动重启后台任务。三种动作的批次、行、后台任务、审计和幂等收据同事务提交。
- 同一 `requestId` 同载荷重试返回收据中的原始结果，即使后台进度已继续推进；同一身份复用不同载荷返回冲突。前端成功后按服务端 `nextStep` 进入进度、结果或待应用区，不乐观改写计数。
- `UNKNOWN` 时前端不改变阶段、不乐观增加成功数，只按同一请求身份查询最终结果。
- 修复、重新校验并形成新试算版本后，服务端按新试算版本创建新的确认任务；已 `CONFIRMED/REJECTED` 的旧任务保持完成并可审计，不复用、不转交。规则版本、manifest 或对象范围变化同样使旧试算结论不再适用于新版本。

### 8.3 前端边界

- 前端只格式化日期、文件大小、百分比和固定状态文案。
- 前端可计算“当前页问题占比”用于辅助文案，但不得作为批次正式成功率、可应用数量或库存试算。
- 基准日口径、来源身份匹配、金额、税额、应收派生、库存数量和映射结论必须使用服务端结果。
- 不在前端读取文件内容后生成“校验通过”结论；浏览器预检查只用于即时格式提示。
- 已形成的正式对象只能通过对应业务修订、调整、变更、冲正或纠错动作修复，导入任务不得直接覆盖或删除。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 批次头、阶段条和当前区同尺寸 Skeleton | 应用壳导航可用 | 查询完成原位替换 |
| 刷新 | 保留当前批次和问题行，显示更新时间 | 非冲突只读操作可用 | 成功更新；失败保留旧数据 |
| 无批次 | 区分“尚未创建”与“当前筛选无结果” | 新建批次或清除筛选 | 创建/清除后恢复 |
| 无数据范围 | 不显示 0 批次 | 查看当前环境范围或申请权限 | 权限更新后重查 |
| 文件扫描中 | 阶段停在安全接收，显示后台进度 | 取消尚未完成上传 | 扫描通过或拒绝 |
| 文件被拒绝/隔离 | 明确禁止原因类别，不回显危险内容 | 更换合规包 | 新资产扫描通过 |
| 校验/应用中 | `BackgroundJobProgress` 展示计数、最近进度和任务号 | 查看问题；按规则取消未开始项 | 后台继续，刷新可恢复 |
| 部分成功 | 成功、跳过、失败分列；不得显示成全量成功 | 下载结果、重跑失败项、打开成功对象 | 修复批次完成 |
| 数据陈旧 | 显示最后进度更新时间；不推断任务卡死 | 刷新、进入 W29 查看任务异常 | 获取新进度 |
| 查询失败 | 有缓存则保留并标记失败；无缓存显示失败态 | 重试 | 查询成功 |
| 版本冲突 | 显示新试算/规则变化，禁止提交 | 重新加载并重新确认 | 新版本确认完成 |
| 待应用 | 全部必要确认已完成，批次仍未形成业务数据 | 有 `legacy_import_batch:execute` 权限时提交应用或取消未应用项 | `START_APPLY` 后进入进度；取消后进入结果 |
| 业务退回已确认 | 固定展示本次试算 `REJECTED`、退回原因、完成任务号和审计时间；不显示转交/后继任务 | 修复数据、重新校验 | 新试算版本形成后创建新的确认任务 |
| 正式动作成功 | `FormalActionResult` 固定展示批次号、全量/部分结果、正式对象数、更新时间和下一步 | 下载报告、打开正式对象、发起修复任务 | 用户明确关闭结果 |
| 正式结果不确定 | 固定结果区写“正在确认最终结果” | 查询最终结果、联系支持 | 得到同请求最终状态 |
| 字段级隐藏 | 标签保留、值掩码，下载同步裁剪 | 其它有权动作 | 权限更新后重查 |
| 权限收回 | 清除诊断缓存和下载链接，切无权限态 | 返回有权工作面 | 权限恢复后重查 |

## 10. 响应式、键盘与无障碍

| 视口 | 布局变化 | 保留内容 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 侧栏展开；批次中心 68/32 双栏；问题表至少 8 行 | 环境、阶段、规则版本、问题计数、主动作 | 无 |
| 1280×800 | 侧栏可折叠；右侧确认卡缩窄 | 环境警示、阶段和进度 | 次要文件说明折叠 |
| 1024×768 | 侧栏图标模式；双栏改 62/38，工具栏换行 | 批次身份、阶段、问题和阻塞原因 | 次要审计摘要移入子区 |
| 768×1024 | 导航抽屉；主区单列；阶段条可横向滚动；表格横向滚动并固定问题身份列 | 查看进度、问题、确认结论 | 文件详情与高级筛选折叠 |
| 375×812 | 只读批次摘要、阶段、进度和确认结果；简单“确认/退回”可用 | 环境、基准日、当前阶段、结果未知处理 | 不上传文件、不提交生产应用、不看大问题表，提示转桌面 |

- Tab 顺序：页头 → 环境/指标 → 阶段 → 当前区筛选 → 问题表 → 阶段主动作 → 结果区。
- 阶段切换使用 `aria-current="step"`；进度计数和正式结果通过 `aria-live=polite` 播报。
- 正式确认 Dialog 关闭后焦点回到触发按钮；阶段完成后焦点落到新阶段标题。
- 问题表支持方向键和 Enter 打开合规详情；错误不能只靠红色表达。
- 触控目标不小于 44×44；手机不提供文件拖放和复杂生产动作。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 今日工作台 / 待办 | W01 / W02 | `workItemId`、批次 ID、责任确认范围 | 完成确认后回原任务并刷新 |
| 客户、合同、销售单 | W03 / W04 / W05 | 成功对象稳定 ID、来源批次 ID | 对象页签关闭后回批次结果 |
| 卡券票款复核 | W13 | 期初卡券销售单范围、批次 ID | 票款复核独立完成，不回写导入结果 |
| 基础资料 | W14 | 基础资料稳定 ID、冲突类型、来源身份 | 修订或映射完成后回 W18 重新校验 |
| 商城同步与映射 | W17 | 卡券期初基线身份、同步水位 | 持续同步不由 W18 驱动 |
| 权限与审计 | W19 | 批次/任务/下载审计过滤 | 返回聚焦原批次 |
| 接口错误中心 | W29 | 后台任务 ID、请求追踪号、错误分类 | 技术异常解决后回批次查询最终状态 |

跨工作面只传稳定身份、任务和筛选上下文；不传金额、库存或“已成功”作为可信事实。

## 12. 验收清单

### 12.1 过程与布局

- [x] 任一批次都能明确看到环境、基准日、对象集、规则版本和六段阶段。
- [x] 上传成功后界面仍明确写“尚未形成正式数据”。
- [ ] 1440×900 下阶段、影响摘要、首批问题和当前主动作同屏可见。
- [x] 问题表能按固定错误码、对象、行列和处理状态筛选，不混入成功长表。
- [x] 刷新或重新打开页签能恢复批次、阶段、问题筛选和后台进度。

### 12.2 业务与数据

- [x] 验证环境校验与业务确认是生产应用前置条件。
- [x] 期初库存只接受统一基准日实盘数量；不导入历史流水。
- [x] 卡券草稿不迁移，期初已收和已开票为 0，后续进入 W13。
- [ ] 已成功对象不会因取消、失败重跑或新文件被直接覆盖、删除或回滚。
- [ ] 来源身份、规则版本、manifest、成功结果和映射谱系可追溯。
- [x] 部分成功正确区分成功、跳过和失败，并支持幂等修复批次。

### 12.3 安全与权限

- [x] 原始 SQL、数据库连接头和禁止字段不会进入长期 `file_asset` 或普通页面。
- [x] 成功资产与失败诊断资产分离，分别执行长期/30 天保留规则；导出 7 天到期。
- [ ] 下载时重验当前对象、数据范围和字段权限，并记录下载审计。
- [x] 业务确认人只能确认本人责任范围，系统管理员不能代替业务确认。
- [ ] `IMPORT_BUSINESS_CONFIRMATION` 已在后端、W01/W02 和 W18 handler 实际接线；业务确认和退回都使用 `ImportBusinessConfirmationCommand`；`RETURN_FOR_FIX` 形成正式 `REJECTED` 并完成当前任务，不转交、不创建后继，也不存在第二次“标记完成”调用。
- [ ] 修复并形成新试算版本后，按新版本创建新的确认任务；旧 `CONFIRMED/REJECTED` 任务保持完成且可审计。
- [ ] 固定 `work_item_type=IMPORT_BUSINESS_CONFIRMATION`、`handlerKey=import_business_confirmation`、`destinationWorkspaceId=W18` 与唯一领域命令 `CompleteImportBusinessConfirmation` 已完成代码登记、展示映射和端到端验证；在此之前运行时必须保持实施 blocker。
- [ ] 权限收回后页面不残留诊断原值、文件短链或敏感缓存。

### 12.4 正式动作与状态

- [x] 创建、校验、确认、应用、取消、重跑均有独立请求身份，重复请求返回同一结果。
- [ ] 正式应用结果不确定时不乐观跳阶段，可查询同请求最终结果。
- [x] 试算或规则变化使旧确认失效，并阻止按旧版本应用。
- [ ] §9 全部状态和 1440/1280/1024/768/375 五档视口完成验证。
- [ ] 键盘可完成问题筛选、打开详情、业务确认和结果查询。

## 13. 已确认决策

| 决策 | 结论 | 出处 |
| --- | --- | --- |
| 业务确认任务类型 | 必须采用单一 `IMPORT_BUSINESS_CONFIRMATION`；以 `confirmationScope + ownerRole` 分责；业务对象固定为真实 `LEGACY_IMPORT_BATCH/batchId`；`responsibilityKey` 固定为已注册 `confirmationScope` 且不可变；`handlerKey=import_business_confirmation`；去向 `W18`；唯一领域命令 `CompleteImportBusinessConfirmation`；decision 仅允许 `CONFIRM_SCOPE \| RETURN_FOR_FIX` | §1.3、§8.2 |
| 确认责任矩阵 | 各对象集合必须由对应责任部门分别确认；生产期初库存与期初应收必须保留独立责任人，禁止同一人兼任 | §2.1 |
| 生产应用复核 | 生产提交应用必须发起/复核分离；验证环境允许单人执行校验与业务确认 | §2.1、§2.2、§7 |
| 批次容量上限 | 文件、行数与诊断下载上限必须由服务端按对象集配置；超限必须拆批；禁止浏览器分片冒充单批 | §7 |
| 验证结论有效期 | 验证确认结果绑定规则版本、manifest 与数据更新时间；任一变化或超过批准窗口必须失效并重新校验确认 | §2.2、§7 |
| W18 范围 | 仅治理型批量导入与期初迁移；禁止以导入替代对象工作面正式业务主路径 | §1.3 |

## 14. 业务依据

- `erp-phase-1.md` §5.3：期初迁移范围、基准日、验证环境、卡券票款初始化和文件保留规则。
- `erp-phase-1.md` §8.2–§8.7：卡券销售单期初基线、来源身份、同步水位、映射和应收派生。
- `erp-data-model.md` §4.5、§6.1：审计、文件安全、`background_job`、逐项结果和下载重验。
- `erp-data-model.md` §6.12：`legacy_import_batch`、`legacy_import_row`、来源 HMAC、成功/失败资产和幂等重跑。
- `erp-ui-design.md` §3.4–§3.5、§4.8、§11：TaskTabs、响应式、M7 分阶段治理、后台任务与结果不确定契约。
- `erp-ui-flows.md` §6、§8：期初票款复核进入 W13；盘点导入仍须回到库存调整上下文。
