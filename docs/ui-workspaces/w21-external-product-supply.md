# W21 · 外部商品映射与供给

> 状态：草稿
> 页面模式：M3 连续处理队列 + M4 外部商品与供给中心
> 主要路由：`/supplier-api/catalog`、`/supplier-api/catalog/:externalProductId`
> 主要角色：采购、运营；系统管理员和研发运维只处理技术异常
> 最后更新：2026-08-01

## 1. 定位与目标

### 1.1 用户目标

- 采购能连续处理供应商目录中的新增、关键变化、停止供应和异常项，将外部商品关联到正确 ERP SKU，并确认独立供给版本。
- 运营能查看商品发布准备度、商城影响和需要运营协同的展示/类目问题，但不替代采购决定供应商映射、供货成本和供给有效期。
- 用户能比较“来源当前版本、来源新版本、ERP SKU、当前供给版本和拟生效供给版本”，不会在多个列表间拼凑差异。
- 停止供应、不可供、库存为零或数据陈旧时，能看见相关发布的暂停结果、替代候选证据和恢复责任 blocker，同时明确历史已支付订单仍使用下单快照。

### 1.2 业务目标

- 供应商目录先进入 `supplier_external_product` 不可变修订暂存区，未经映射和审核不得直接修改 ERP SKU 或商城商品。
- 以 `supplier_product_mapping` 保存可审计的外部商品到 ERP SKU 映射，以 `supplier_offering` + 不可变修订保存价格、税率、费用、区域、能力和有效期。
- 新商品及关键变化通过连续队列处理；供货价变更不自动修改商城销售价，供应商切换不使用动态路由。替代供给的选定与恢复发布责任尚未确认，当前只能准备候选证据，不得生成切换事实或打开 W22 恢复链。
- 同一商城发布版本只绑定一条确定供给修订；切换供给形成 W22 新发布版本，历史订单保留原商品、销售价和供应商成本快照。
- 对停止供应、零库存、不可供、可供数据过期，以及成本或其它关键供给变化尚未确认的在售商品实行 fail-closed：系统立即幂等形成暂停发布修订/动作和 outbox。仅 `STOPPED` 创建/复用已注册聚合异常任务，其它原因固定保留 blocker/证据；任何人工领取、确认或替代供给处理都不是暂停前置条件。

### 1.3 不在本工作面完成

- 不维护 Supplier Connector、地址或密钥引用；进入 W20。
- 不在 W21 编辑 ERP SKU 正式基础资料全文；关联已有 SKU，或保留队列上下文进入 W14 新建/修订。
- 不在 W21 设置商城销售价、最小购买量、展示图文和最终上下架版本；进入 W22。
- 不在供应商间自动比价、评分、拆量或运行动态路由；供应商切换由有权人员明确确认。
- 不在责任链确认前由页面假定“采购选定替代供给、运营恢复发布”为已生效规则。
- 不修改已支付商城订单的商品、供应商、供货成本或能力快照。
- 不在业务页面修复供应商协议、重放订单或查看原始敏感报文；技术异常进入 W29。

## 2. 用户、权限与数据范围

### 2.1 角色与主责

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 采购 | 待处理目录队列 | 被授权供应商、品类和采购组织的外部商品及供给 | 映射已有 SKU、确认供给版本、处理价格/区域/停止供应变化；可准备替代候选证据，不直接选定恢复供给 |
| 运营 | 发布准备与运营协同视图 | 被授权商城/类目相关外部商品、映射和供给摘要 | 查看准备度、处理运营类目/展示 blocker、进入 W22 发布 |
| 系统管理员 | 同步批次和技术异常摘要 | 被授权连接的同步水位、异常分类 | 重试技术同步或进入 W29；不能确认商品映射和成本 |
| 研发运维 | 技术诊断入口 | 连接、同步任务和脱敏错误 | 排查 Connector / 同步故障；不能选择 ERP SKU 或供给 |
| 财务 | 供给版本只读入口 | 被授权供应商成本、税率和历史版本 | 核对成本来源；无映射和版本确认动作 |

### 2.2 权限表达

| 情况 | W21 行为 |
| --- | --- |
| 无模块权限 | 不展示导航和命令入口；直接访问显示无权限页 |
| 有模块权限但无供应商/品类范围 | 展示“当前角色无目录数据范围”，不显示 0 项待办 |
| 运营无采购确认权 | 可见安全业务摘要和发布 blocker；映射、供给成本动作禁用并说明责任角色 |
| 管理员只有技术权限 | 可见同步批次和错误，不显示不必要成本、业务映射候选和确认动作 |
| 无成本字段权限 | 价格、税率、费用保留标签并掩码；不在影响摘要、提示或导出泄露 |
| 当前版本/状态不允许动作 | 动作原位禁用并展示服务端 `actionBlockers` |
| 权限收回或队列租约丢失 | 清除敏感字段和可写表单；保留稳定项身份，转只读/无权限态 |

### 2.3 业务主责边界

- 采购主责“外部商品对应哪个 ERP SKU”以及“这个供应商以什么成本、区域、数量、能力和有效期供给”。
- 运营主责 W22 的商城发布内容和发布确认；W21 只展示发布准备度与运营 blocker。
- 停止供应后的替代供给选定人、恢复发布发起人及两者交接规则尚未确认。确认前服务端必须返回 `RECOVERY_RESPONSIBILITY_UNCONFIRMED`，阻断选定替代供给和从 W21 发起 W22 恢复发布；安全暂停、查看影响和准备会话内候选证据不受阻断。
- 系统管理员和研发运维主责同步链路与错误，不得因修复技术异常自动接受错误映射或成本。
- 库存/可供状态可按正式策略自动同步；新商品、映射、供货价和其它关键供给变化必须按服务端规则进入人工确认。
- `STOPPED`、零库存、不可供、可供数据超过服务端新鲜度阈值时，系统对每个受影响的在售发布立即执行安全暂停；供货价、进项税率、费用、MOQ、区域或商品能力变化未确认时，同样先暂停新订单。暂停动作按“发布对象 + 原因 + 来源版本”幂等，并在同一事务追加不可变暂停修订/明确暂停动作、投递记录和 outbox。
- 上述安全暂停不等待人工任务。仅 `STOPPED` 事件按“来源对象 + 原因 + 来源版本”幂等创建/复用一个已注册 `BUSINESS_EXCEPTION` `work_item`；独立来源 `ERROR` 也使用该异常类型。零库存、不可供和过期只形成暂停与阻断证据；供货价/关键供给变化应进入正常审核队列，但其固定类型未登记前只返回实施 blocker。不得用 `BUSINESS_EXCEPTION` 伪装正常供给确认；是否有任务都不影响本地保持不可下单。
- 正常外部商品映射与供给复核的固定 `work_item_type` 尚未登记。登记前相关项必须展示 fail-closed 实施 blocker，不能领取或提交正式确认；安全暂停、历史订单保护和不可下单状态不受该 blocker 影响。
- W21 不维护私有任务状态机。已注册异常任务统一读取 W02 的任务详情/租约，并通过 W02 共享信封执行任务内动作或终结决策；未来正常映射/供给类型登记后也必须复用相同信封，不得另造顶层任务字段或页面私有完成接口。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 打开处理队列 | W01/W02 已注册异常任务、W20 目录同步结果 | `/supplier-api/catalog?mode=queue&changeType=actionable`；身份为队列过滤摘要 | 刷新恢复筛选、当前项和自动下一项偏好 |
| 定位当前项 | 队列上一项/下一项 | URL 写入 `currentExternalProductId`；仅已绑定正式任务时再写 `currentWorkItemId`，同时保存 `queueContextId/autoNext` | 后退恢复上一已查看项，不逆转正式动作 |
| 打开外部商品中心 | 列表/队列“在中心打开” | `/supplier-api/catalog/:externalProductId?section=overview`；身份 `external-product:{externalProductId}` | 队列页签保留；关闭中心回原项 |
| 创建/修订 ERP SKU | 映射区 | 打开 W14 并携带来源外部商品与队列上下文 | 完成后回 W21，重新查询新 SKU 稳定 ID |
| 进入商品发布 | 普通发布准备度区“去发布” | 仅非安全暂停恢复场景可打开 W22，携带 SKU、已确认供给修订和 blocker 摘要；恢复场景固定被 `RECOVERY_RESPONSIBILITY_UNCONFIRMED` 阻断 | 普通发布后回外部商品中心 |
| 查看连接 | 来源区 | 打开 W20 连接中心 | 返回原外部商品/队列项 |
| 排查技术异常 | 同步异常项 | 打开 W29，携带连接、同步任务和目录项 ID | 解决后回 W21 重新查询 |

队列 URL 必须保存筛选、`currentExternalProductId`、`queueContextId` 和自动下一项偏好；只有已注册异常任务才保存 `currentWorkItemId`。W01/W02 打开 W21 时把来源 `workItemId` 映射为 `currentWorkItemId`，并传入 `queueContextId`；W21 重新查询 W02 任务详情、业务对象版本和租约，不接受页面间传递的“已领取”或“已确认”结论。任务内动作成功仍停留在当前项，正式状态只能是 `PENDING` / `IN_PROGRESS`；终结决策完成后先固定展示结果，再按已保存偏好或由用户继续下一项。映射/供给表单有脏状态时离开或关闭页签必须确认。

## 4. 页面布局

### 4.1 连续处理队列

```text
┌ SequentialProcessBar
│ 第 3/42 项 · 新增/变化/停止供应 · 供应商甲 · [上一项] [下一项] [自动下一项]
├ 身份与风险：外部商品 ID/SKU ID · 来源版本 · 更新时间 · 变化类型 · 发布影响
├───────────────────────────────────────┬───────────────────────────────┐
│ 来源版本差异 约 58%                   │ 映射与供给决策 约 42%          │
│ 当前来源 vs 新来源                    │ ERP SKU 候选/当前映射          │
│ 名称/规格/类目/价格/区域/库存/能力    │ 当前供给 vs 拟生效供给         │
│ 图片/说明/售后摘要                    │ 有效期、影响、责任与 blocker    │
├───────────────────────────────────────┴───────────────────────────────┤
│ [暂挂] [退回数据修复] [确认映射] [确认供给版本]                     │
└ FormalActionResult（动作成功后先固定呈现）
```

### 4.2 外部商品与供给中心

```text
┌ PageHeader object-chrome：外部商品供给 › 外部商品 ID     [返回队列] ─┐
├ DocumentHeader compact：商品名 [变更类型] · 外部 ID · 版本 · 映射/供给轨
│ 映射：ERP SKU               供给：版本/有效期          发布：影响摘要
├ Alert：停止供应 / 不可供 / 数据陈旧 / 价格待确认 / 发布已暂停
├ 锚点：概览 | 来源版本 | 映射历史 | 供给版本 | 发布影响 | 同步记录 | 审计
├ 当前来源与 ERP SKU 摘要
├ RevisionTimeline + BusinessDiffPanel
├ Supplier Offering 版本表：成本、税率、费用、区域、能力、有效期
└ RelatedDocumentList：W22 发布、W26 历史订单、W29 异常
```

### 4.3 区域说明

| 区域 | 目的 | 主组件 | 是否固定 |
| --- | --- | --- | --- |
| 队列栏 | 连续处理且可恢复上下文 | `SequentialProcessBar` | 顶部固定 |
| 身份与风险条 | 防止映射错供应商、外部身份或版本 | `PageHeader object-chrome` + `DocumentHeader density="compact"` `Alert` | 队列栏下固定 |
| 来源差异 | 比较当前/新修订的白名单业务字段 | `BusinessDiffPanel` | 主阅读区 |
| SKU 映射 | 搜索已有 SKU、查看规格与映射历史 | `BusinessObjectCombobox` `DocumentSummary` | 决策侧栏 |
| 供给编辑 | 确认价格、税率、费用、区域、MOQ、能力和有效期 | TanStack Form + `ValidationSummary` | 有权时可写 |
| 影响摘要 | 展示发布暂停、待处理订单和历史快照边界 | `BatchImpactPreview` / 影响卡 | 正式确认前常驻 |
| 版本与审计 | 追溯来源、映射、供给和操作 | `RevisionTimeline` `AuditTimeline` | 中心只读 |
| 正式结果 | 映射/供给版本、影响和下一步 | `FormalActionResult` | 动作后固定 |

## 5. 展示内容与字段

### 5.1 来源外部商品

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 身份 | `supplier/connection` | 供应商 / 连接 | `supplier_external_product` + W20 投影 | 稳定 ID + 安全名称 | 按供应商和连接范围 |
| 身份 | `externalProductId/externalSkuId` | 外部商品 ID / 外部 SKU ID | 外部商品稳定身份 | 保留来源原值；不做大小写或数值化合并 | 敏感外部 ID 可掩码 |
| 版本 | `revisionNo/externalRevisionToken` | ERP 观察版本 / 来源版本 | 外部商品修订 | 无来源版本时仍使用 ERP 修订号 | 技术 token 按字段权显示 |
| 版本 | `sourceUpdatedAt/syncedAt` | 来源更新时间 / ERP 接收时间 | 修订/同步任务 | 两个时间分别显示 | 有对象查看权可见 |
| 描述 | `name/specification/category` | 名称 / 规格 / 来源类目 | 外部商品修订 | 规范化白名单字段 | 有目录查看权可见 |
| 成本 | `supplyPriceGross/inputTaxRate` | 含税供货价 / 进项税率 | 外部商品修订 | 金额右齐、税率口径明确 | 按成本字段权限 |
| 费用 | `freightAmount/otherFeeAmount` | 运费 / 其它费用 | 外部商品修订 | 来源费用，不与供货价前端相加成正式成本 | 按成本字段权限 |
| 供给 | `supplyRegion/availableQuantity/availabilityStatus` | 可供区域 / 数量 / 状态 | 外部商品修订 | 数量带单位；不保证实时库存 | 按业务范围 |
| 供给 | `expectedShipTime/afterSalesNote` | 预计发货 / 售后说明 | 外部商品修订 | 安全文本摘要 | 不展示供应商协议原文 |
| 能力 | `capabilitySnapshot` | 商品级能力 | 外部商品修订 | 取消、退款、物流等固定能力摘要 | 与连接能力并列说明 |
| 审计 | `sourcePayloadHmac/hmacKeyVersion` | 来源内容指纹 | 外部商品修订 | 默认仅短指纹与密钥版本，用于幂等审计 | 仅技术/审计权限 |

原始供应商报文不能替代结构化字段，也不在普通详情展示。媒体只展示安全扫描通过的规范化预览；原始外链和未扫描文件不渲染。

### 5.2 映射与 ERP SKU

| 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- |
| `mappingStatus` | 映射状态 | `supplier_product_mapping` | 待审核、已生效、冲突、停用 | 服务端状态 |
| `skuId/skuCode` | ERP SKU | `sku` | 稳定身份和代码 | 按 W14 对象权限 |
| `skuRevisionId` | 当前 SKU 版本 | `sku_revision` | 映射确认时展示的当前修订 | 无基础资料查看权只显示安全摘要 |
| `skuName/specification/baseUnit` | ERP 名称 / 规格 / 基本单位 | SKU 当前修订 | 与来源字段并列 diff | 按字段权限 |
| `approvedBy/At/reason` | 映射确认 | 映射记录 | 确认人、时间、结构化依据 | 审计范围内可见 |
| `mappingHistory` | 映射历史 | 映射版本/审计投影 | 保留失效和冲突记录，不原位覆盖 | 只读 |

同一外部商品同一时点只能有一个有效 ERP SKU 映射；一个 ERP SKU 可关联多个供应商外部商品。变更映射不得反写已支付订单的 SKU 快照。

### 5.3 供给版本与影响

| 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- |
| `offeringId/revisionNo` | 供给 / 版本 | `supplier_offering` / 修订 | 稳定供给身份 + 不可变修订号 | 有供给查看权可见 |
| `status` | 供给状态 | 供给 | 启用、暂停、停止 | 服务端状态 |
| `supplyPriceGross/net/inputTaxRate` | 含税价 / 不含税价 / 进项税率 | 供给修订 | 全部服务端确认；口径分开 | 成本字段权限 |
| `freightAmount/serviceFeeAmount` | 运费 / 服务费 | 供给修订 | 金额右齐，不在前端推导总成本 | 成本字段权限 |
| `minimumOrderQuantity` | 最小起订量 | 供给修订 | 必须 >0，按 ERP 基本单位 | 采购可编辑 |
| `supplyRegion` | 可供区域 | 供给修订 | 结构化区域集合 | 采购可编辑 |
| `availabilityStatus/availableQuantity` | 可供状态 / 数量 | 供给修订/同步投影 | 展示水位；零库存不等于永久停止供应 | 有业务权可见 |
| `productCapabilities` | 商品级能力 | 供给修订 | 与连接能力交集由服务端确认 | 采购/运营可见 |
| `validFrom/validTo` | 有效期 | 供给修订 | 同一供给有效期不可重叠 | 采购可编辑，服务端校验 |
| `activePublicationCount` | 关联在售发布 | W22 投影 | 当前绑定该供给修订的发布数 | 有 W22 权限可钻取 |
| `historicalPaidOrderCount` | 历史已支付订单 | W25/W26 聚合 | 只提示历史快照仍有效，不作为改写目标 | 仅授权汇总可见 |

`minimumOrderQuantity` 是供应商供给约束，不等同 W22 的商城 `minimumPurchaseQuantity`，不得自动复制。供货价变化形成新供给修订，不覆盖旧价，也不得自动修改商城销售价。

### 5.4 变化分类与默认处理

| 变化 | 默认处理 | W21 人工动作 | 下游影响 |
| --- | --- | --- | --- |
| 新外部商品 | 进入只读队列并显示正式类型未注册 blocker | 可比较 SKU、准备草稿或到 W14 建新 SKU；当前不得确认映射/供给 | 登记正式类型并完成确认后才可进入 W22 |
| 名称/规格/类目关键变化 | 进入队列 | 核对映射仍然有效；必要时修订 SKU | 未确认不得自动发布 |
| 供货价/税率/费用/MOQ/区域/能力变化 | 系统立即幂等安全暂停受影响在售发布；正常供给审核类型未注册时固定返回 blocker/证据，不创建 `BUSINESS_EXCEPTION` | 可查看 diff 和准备草稿；登记正式类型前不得确认新供给 | 未确认期间保持不可下单；未来确认也不自动恢复，W22 另形成恢复发布版本 |
| 库存/可供状态变化 | 数量为零、不可供或超过新鲜度阈值时，系统立即幂等安全暂停并固定阻断证据，不临时创建任务类型 | 查看来源和 blocker；如需正常人工复核，先登记固定类型 | 暂停不等待人工；重新可供也不自动上架 |
| 停止供应 `STOPPED` | 系统立即幂等安全暂停全部受影响在售发布，并生成高优先级正式任务 | 处理停止供应任务：核对影响、确认停供事实，可准备会话内替代候选证据；当前不得选定替代供给或发起恢复 | 暂停已先发生；历史订单不改，责任规则确认前固定显示 `RECOVERY_RESPONSIBILITY_UNCONFIRMED` |
| 无变化 | 同步任务记录，不进入人工队列 | 无 | 更新水位，不造新业务版本 |
| 异常数据 `ERROR` | 创建已注册 `BUSINESS_EXCEPTION` 任务 | 退回来源/技术处理、查询原结果或确认异常已解决，不能强行映射 | 不进入正式商品和发布 |

## 6. 搜索、筛选、排序与默认视图

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 模式 | 待处理队列 | `mode=queue/list` | 队列用于处理；列表用于全量查询和中心入口 |
| 供应商/连接 | 全部有权范围 | `supplierId/connectionId` | 选择稳定对象，不自由输入内部 ID |
| 变化类型 | 需处理 | `changeType` | 新增、关键变化、停止供应、异常；无变化默认隐藏 |
| 映射状态 | 待审核 + 冲突 | `mappingStatus` | 可查已生效/停用历史 |
| 供给状态 | 全部 | `offeringStatus` | 启用、暂停、停止、待确认 |
| 发布影响 | 全部 | `publicationImpact` | 在售受影响、已暂停、无发布 |
| 数据新鲜度 | 新鲜 + 陈旧 | `freshness` | 陈旧阈值由服务端返回 |
| 搜索 | 空 | `q` | 搜外部商品 ID/SKU ID、ERP SKU 代码、规范化名称 |
| 排序 | 停止供应/不可供 → 价格变化 → 新增 → 其它，组内最早进入优先 | `sort` | 服务端稳定排序；非终结动作不自动下一项，终结结果固定展示后由用户继续 |

队列总数、位置和指标使用服务端快照，不用当前页求和。1440×900 的查询列表至少显示 6–8 行；外部身份和行级主动作固定。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 系统安全暂停 | 目录事件处理器（非页面按钮） | `STOPPED`、零库存、不可供、数据过期，或成本/关键供给变化未确认且存在受影响在售发布 | 无人工确认 | 按来源对象/原因/版本冻结影响集并原子写所有暂停子结果/投递/outbox；仅 `STOPPED` 另创建/复用一个 `BUSINESS_EXCEPTION`，其它原因返回任务类型 blocker/证据 | 调用结果未知时按原幂等键查询；投递失败进入 W29，绝不恢复可下单状态 |
| 领取/打开正式任务 | 已注册来源 `ERROR` 或 `STOPPED` 异常 | 有处理权；按 W02 `ClaimWorkItemCommand` 条件领取成功或返回只读状态 | 无 | 返回 W02 `WorkItemLease`；任务详情继续使用 W02 嵌套读模型，不复制顶层任务字段 | 被他人领取时只读，显示领取人/到期；页面不新建私有队列项 |
| 映射已有 SKU | 映射区 | 采购映射权；候选 SKU 有效；外部修订未变化；当前正式类型未登记 | 显示来源/ERP 规格 diff 和历史影响 | 仅更新未提交的页面草稿；登记正式类型前不得形成映射事实 | 显示 `WORK_ITEM_TYPE_UNREGISTERED` blocker；版本变化后重新核对 |
| 新建/修订 SKU | 映射区 | 有 W14 权限 | W21 不执行正式建档确认 | 打开 W14；返回后重新选择稳定 SKU | 导航失败留在当前队列且租约按协议处理 |
| 确认映射 | 队列主动作 | 当前固定类型未登记，入口强制禁用；未来登记后还需映射完整、无冲突且租约/版本一致 | 未来使用 `FormalActionConfirmDialog` 展示外部身份、ERP SKU 和映射依据 | 当前无提交；未来以 `CompleteWorkItemEnvelope<ExternalCatalogMappingDecision>` 原子写映射事实、审计和任务终态 | 当前展示注册 blocker；未来结果未知停留当前项并查询，不自动下一项 |
| 确认供给修订 | 供给区主动作 | 当前固定类型未登记，入口强制禁用；未来登记后还需映射有效且供给字段校验通过 | 未来展示当前/拟生效版本 diff、发布暂停和历史订单边界 | 当前无提交；未来以 `CompleteWorkItemEnvelope<ExternalCatalogOfferingDecision>` 追加不可变供给修订、审计和任务终态 | 当前展示注册 blocker；未来冲突不覆盖、未知停留查询 |
| 暂挂 | 已注册异常任务次动作 | 有队列处理权和有效租约 | 选择结构化原因，可填备注 | 使用 `WorkItemActionEnvelope<ExternalCatalogWorkItemAction>` 追加暂挂证据，返回 `PENDING` / `IN_PROGRESS`；不终结任务、不自动下一项 | 提交失败留当前任务；不得新增“暂挂”正式状态 |
| 退回数据修复 | 已注册 `ERROR` 异常项 | 有处理权；不能通过业务确认修复来源错误 | 原因必填，显示责任方 | 使用 `WorkItemActionEnvelope<ExternalCatalogWorkItemAction>` 追加退回请求/证据，任务仍为 `PENDING` / `IN_PROGRESS`；若需改变责任人必须另走 W02 正式转交协议 | 失败保留原因；不改变来源修订或任务终态 |
| 查询原动作结果 | 结果未知提示 | 已注册异常任务仍有效；持有原动作查询依据 | 无 | 使用 `WorkItemActionEnvelope<ExternalCatalogWorkItemAction>` 保存查询证据，成功仍返回 `PENDING` / `IN_PROGRESS`；不得因查到外部结果自动完成任务 | 继续停在当前项；由用户基于新证据执行唯一终结决策 |
| 确认异常已解决 | 已注册 `ERROR` 异常任务 | 数据修复证据已存在；任务租约、对象版本与指纹一致 | 展示异常来源、修复证据和下游未写入边界 | 使用 `CompleteWorkItemEnvelope<ExternalCatalogDecision>`；异常结论、审计和任务终态同事务 | 结果未知时任务不完成；按原幂等身份查询，不自动重做业务写入 |
| 处理停止供应任务 | 已注册 `STOPPED` 异常任务 | 采购供给权；任务租约、对象版本与指纹一致；安全暂停已经触发 | 强确认关联发布、历史订单边界和“不包含替代供给选定/恢复发布” | 使用 `CompleteWorkItemEnvelope<ExternalCatalogDecision>`；停止事实结论、审计和 `work_item` 终态同事务，原安全暂停保持 | 业务结果未知时不完成任务；按原操作查询。该动作从不承担首次暂停，也不能选定替代供给或恢复上架 |
| 准备替代候选 | 发布影响区 | 有对象查看权；当前任务可继续处理 | 无正式确认，始终显示“仅会话内候选证据” | 仅在当前页签准备候选对比；不持久化选定事实、不改供给指针、不打开 W22 恢复链 | 刷新/关闭前提示候选输入将丢失；正式选定固定被 `RECOVERY_RESPONSIBILITY_UNCONFIRMED` 阻断 |
| 触发重同步 | 同步记录 | 管理员/运维权限；W20 连接能力有效 | 展示连接、范围和来源水位 | 创建幂等目录同步任务 | 未知按任务号查询；不造重复修订 |

已注册异常任务的任务字段只由 W02 共享信封承载。服务端校验信封中的 `workItemId`、领取人、`claimToken`、`leaseVersion`、`expectedSubjectVersion`、`expectedSubjectHash`，再重读外部修订、映射/供给版本、权限和岗位责任；客户端不提交 `completionAction`，服务端按 `handlerKey` 和注册处理器校验决策是否为唯一允许的终结动作。`HOLD`、`RETURN_FOR_DATA_FIX`、查询和保存证据都是非终结动作，成功后任务仍为 `PENDING` / `IN_PROGRESS`；只有 `CompleteWorkItemEnvelope<ExternalCatalogDecision>` 能原子写入已注册异常的强类型结论、审计与任务终态。正常映射/供给在类型登记前没有正式提交端点，登记后也复用 W02 的 `CompleteWorkItemEnvelope`。

## 8. 数据契约

### 8.1 查询

```ts
type ExternalCatalogQueueQuery = {
  supplierId?: string
  connectionId?: string
  changeTypes?: Array<"NEW" | "CHANGED" | "STOPPED" | "ERROR" | "UNCHANGED">
  mappingStatuses?: string[]
  offeringStatuses?: string[]
  publicationImpact?: string[]
  freshness?: string[]
  q?: string
  queueContextId?: string
  currentExternalProductId?: string
  currentWorkItemId?: string
  pageSize: number
  sort: string
}

type ExternalCatalogRegistrationBlocker = {
  code: "WORK_ITEM_TYPE_UNREGISTERED"
  message: string
  businessProcess: "MAPPING" | "OFFERING_REVIEW"
}

type ExternalCatalogExceptionWorkItem = QueueWorkItemDetail & {
  workItemType: "BUSINESS_EXCEPTION"
  businessObjectType: "SUPPLIER_EXTERNAL_PRODUCT" | "SUPPLIER_OFFERING"
}

type ExternalCatalogItemBase = {
  queuePosition: { current: number; total: number; snapshotId: string }
  externalProduct: {
    id: string
    supplier: { id: string; name: string }
    connection: { id: string; code: string }
    externalProductId: string
    externalSkuId?: string
    status: string
    currentRevision: ExternalProductRevisionView
    incomingRevision?: ExternalProductRevisionView
  }
  mapping?: SupplierProductMappingView
  skuCandidates?: SkuCandidateView[]
  offering?: { stableId: string; currentRevision?: SupplierOfferingRevisionView; proposedDefaults?: SafeOfferingDraftView }
  publicationImpact: PublicationImpactView
  syncContext: { jobId: string; sourceBatchIdentity: string; receivedAt: string }
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string; destinationWorkspaceId?: string }>
}

type ExternalCatalogItemView =
  | (ExternalCatalogItemBase & {
      changeType: "ERROR" | "STOPPED"
      workItem: ExternalCatalogExceptionWorkItem
      registrationBlocker?: never
    })
  | (ExternalCatalogItemBase & {
      changeType: "NEW" | "CHANGED"
      workItem?: never
      registrationBlocker: ExternalCatalogRegistrationBlocker
    })
  | (ExternalCatalogItemBase & {
      changeType: "UNCHANGED"
      workItem?: never
      registrationBlocker?: never
    })
```

- `changeType=ERROR|STOPPED` 时必须且只能返回一个已注册 `BUSINESS_EXCEPTION` `workItem`，并且不得返回 `registrationBlocker`；该任务直接复用 W02 嵌套 `QueueWorkItemDetail`，W21 不摊平或复制任务字段，查询响应也不返回 `claimToken`。
- `changeType=NEW|CHANGED` 时必须返回 `registrationBlocker` 且不得返回 `workItem`；正常目录映射和供给复核的固定 `work_item_type` 尚未写入当前权威注册表，其 W01/W02 入口、领取与正式确认整体禁用；不得用页面私有枚举或 `BUSINESS_EXCEPTION` 替代正常必经复核。
- `changeType=UNCHANGED` 若在全量查询中返回，既没有 `workItem` 也没有 `registrationBlocker`；它只表示同步水位，不进入人工队列。
- 外部目录项以 `externalProduct.id` 定位；已绑定任务时再以 `workItem.workItemId` 定位任务生命周期。页面不得另设私有处理项 ID 或独立任务状态。
- 队列、中心、候选 SKU、版本历史和影响查询均由 TanStack Query 管理。
- Query Key 包含用户、角色、权限版本、供应商/连接、筛选、`queueContextId`、`currentExternalProductId`、可选 `currentWorkItemId` / `subjectHash`、外部修订和供给版本。
- `claimToken` 只由 W02 `ClaimWorkItemCommand` 的成功结果返回并保存在当前会话内存，不进入查询响应、URL、TanStack Query 持久缓存、日志或分析事件；失去租约或切换用户时立即清除。
- 队列切换时预取下一项只读数据，但不得提前领取、解密敏感字段或提交动作。
- 来源修订返回结构化白名单字段和安全媒体；不返回原始供应商报文、密钥、签名或内部堆栈。
- 价格、税率、费用、发布影响和历史订单数量由服务端计算并标注水位。

### 8.2 提交

```ts
type ExternalCatalogWorkItemAction =
  | { kind: "HOLD"; reasonCode: string; comment?: string }
  | { kind: "RETURN_FOR_DATA_FIX"; reasonCode: string; suggestedResponsibleRole?: string; comment?: string }
  | { kind: "QUERY_ORIGINAL_RESULT"; operationId: string }
  | { kind: "SAVE_EVIDENCE"; evidenceReferences: string[]; comment?: string }

type ExternalCatalogDecision =
  | {
      kind: "CONFIRM_ERROR_RESOLVED"
      expectedExternalRevision: string
      resolutionCode: string
      evidenceReferences?: string[]
      comment?: string
    }
  | {
      kind: "CONFIRM_STOP_SUPPLY"
      expectedExternalRevision: string
      expectedOfferingRevision?: string
      reasonCode: string
      comment?: string
    }

type ExternalCatalogMappingDecision = {
  kind: "APPROVE_MAPPING"
  expectedExternalRevision: string
  expectedMappingVersion?: string
  skuId: string
  reasonCode: string
  comment?: string
}

type ExternalCatalogOfferingDecision = {
  kind: "CONFIRM_OFFERING_REVISION"
  expectedExternalRevision: string
  expectedMappingVersion: string
  expectedOfferingRevision?: string
  offeringDraft: {
    supplyPriceGross: string
    inputTaxRate: string
    freightAmount: string
    serviceFeeAmount: string
    minimumOrderQuantity: string
    supplyRegion: string[]
    productCapabilities: string[]
    validFrom: string
    validTo?: string
  }
  reasonCode: string
  comment?: string
}

type ExternalCatalogActionEvidence = {
  evidenceReferences?: string[]
  resultSummary?: string
  checkedAt: string
}

type ExternalCatalogBusinessResult = {
  decisionKind: ExternalCatalogDecision["kind"]
  externalProductId: string
  auditEventId: string
  offeringRevision?: string
  publicationImpact: PublicationImpactView
}

type ExternalCatalogWorkItemActionCommand = WorkItemActionEnvelope<ExternalCatalogWorkItemAction>
type ExternalCatalogWorkItemActionResult = WorkItemActionResult<ExternalCatalogActionEvidence>
type CompleteExternalCatalogWorkItemCommand = CompleteWorkItemEnvelope<ExternalCatalogDecision>
type CompleteExternalCatalogWorkItemResult = CompleteWorkItemResult<ExternalCatalogBusinessResult>
```

- 供给表单使用 TanStack Form；金额和比例以十进制定点字符串提交，前端不以浮点计算正式净价或成本。
- 已注册来源 `ERROR` 与 `STOPPED` 异常的非终结动作统一提交 W02 `WorkItemActionEnvelope<ExternalCatalogWorkItemAction>`；其幂等键只标识本次任务内动作。成功必须返回 `WorkItemActionResult` 且 `workItemStatus: "PENDING" | "IN_PROGRESS"`，即使查到外部结果、保存证据或发出数据修复请求也不自动完成/跳过任务。
- 已注册异常的唯一终结决策统一提交 `CompleteWorkItemEnvelope<ExternalCatalogDecision>`。客户端只提交共享信封和强类型 `decision`，不读取后再回传 `completionAction`；服务端按任务 `handlerKey`、注册处理器和当前事实验证允许的决策。
- 服务端重验共享信封中的领取人、原始 `claimToken` 对应的不可逆摘要、`leaseVersion`、`expectedSubjectVersion`、`expectedSubjectHash` 与业务对象重算指纹，并校验外部修订、映射/供给版本、SKU 状态、单位、权限和岗位责任。
- 正常映射/供给类型登记前，`ExternalCatalogMappingDecision` / `ExternalCatalogOfferingDecision` 仅定义未来处理器的强类型决策形状，不存在可调用提交端点；登记后分别包装为 W02 `CompleteWorkItemEnvelope<TDecision>`，不得重新摊平任务字段。
- 非终结动作返回动作证据和可选新租约；终结决策返回 `CompleteWorkItemResult`、异常处理业务结果、审计号和最终对象指纹。只有终结成功后才允许展示下一 `workItemId`。
- `UNKNOWN` 时锁定当前项写动作，不移动队列；按原幂等身份查询最终结果。
- 外部修订在用户处理期间变化时，旧表单不允许直接提交；显示来源 diff 并要求重新确认。
- 终结异常任务必须在一个领域事务内写入强类型异常处理结论、追加审计并完成 `work_item`；处理 `STOPPED` 任务不写正常映射、供给修订或 `ON_SALE` 发布。系统安全暂停在人工任务之前独立提交，不使用人工任务信封，也不受租约状态影响。

### 8.3 前端边界

- 前端只格式化来源/ERP diff、时间、金额、比例、数量和固定状态文案。
- 前端可高亮字段变化，但变化分类、是否关键、发布是否暂停、候选 SKU 相似度和风险均采用服务端结果。
- 供货净价、税额、综合成本、发布影响、库存是否陈旧和能力交集不得由前端形成正式结论。
- 映射和供给审批只能追加/更新正式版本指针，不修改旧修订和历史订单快照。
- 当前只允许在页签会话内比较替代候选，前端不自动选择“最便宜”或“库存最多”的供应商。责任链未确认前，服务端不提供选定替代供给或发起 W22 恢复的 mutation。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 队列栏、身份、diff 和决策区同尺寸 Skeleton | 应用壳导航可用 | 查询完成原位替换 |
| 刷新 | 保留当前项和输入，标记来源/影响水位 | 查看；正式提交重验版本 | 成功更新；变化时进入冲突态 |
| 队列已处理完 | 明确“本筛选项已处理完” | 返回 W01/W02、清除筛选、查看已处理 | 新任务到达或换筛选 |
| 筛选无结果 | 展示筛选摘要 | 清除筛选 | 返回待处理全量 |
| 无数据范围 | 不显示 0 项 | 查看供应商/品类范围或申请权限 | 权限更新后重查 |
| 查询失败 | 无缓存显示 `BusinessFailureState`；有缓存保留当前项并标陈旧 | 重试；正式动作禁用 | 查询恢复 |
| 正常类型未注册 | 新商品、映射或供给复核可只读查看 diff，并显示 `WORK_ITEM_TYPE_UNREGISTERED` blocker | 准备草稿、进入 W14 补基础资料；不能领取或提交正式确认 | 权威注册表、W01/W02 handler 与共享信封处理器全部登记后重新查询 |
| 无 SKU 候选 | 显示“没有合适的现有 SKU” | 打开 W14 新建；不允许随便选近似项 | 建档后返回重查 |
| 被他人领取 | 显示领取人、租约到期和只读内容 | 在中心查看、稍后重试 | 租约释放/转交 |
| 租约丢失 | 保留本地输入但禁用正式动作 | 复制安全备注、重新领取并重新校验 | 获得新租约 |
| 来源版本变化 | `ConflictResolutionDialog` 显示新旧来源 diff | 重新加载并重填受影响字段 | 基于新修订确认 |
| 供给版本冲突 | 展示当前已生效版本和本人草稿 diff | 重新加载、放弃或重新应用 | 基于最新版本提交 |
| 停止供应 `STOPPED` | 高风险 Alert，明确“安全暂停已触发”，列出唯一聚合异常任务、暂停子结果、outbox、商城投递、历史订单边界和 `RECOVERY_RESPONSIBILITY_UNCONFIRMED` | 处理 `BUSINESS_EXCEPTION`、准备会话内替代候选证据、查询投递或进入 W29；不能选定替代或打开 W22 恢复链 | 责任规则写入权威合同后重新查询；投递异常不解除本地暂停 |
| 零库存/不可供/数据过期/成本待确认 | 高风险 Alert 展示已安全暂停、投递身份和任务类型 blocker/证据 | 查询投递、查看 diff 或准备草稿；不允许借异常任务确认正常供给 | 固定审核类型登记并完成正常确认后，仍需 W22 新发布版本恢复 |
| 数据陈旧 | 显示来源、库存和发布各自水位 | 重同步、进入 W20/W29 | 同步追平 |
| 任务内动作成功 | 固定展示动作记录/证据、可选新租约和 `PENDING` / `IN_PROGRESS` 状态 | 继续处理当前异常任务；用户可手动查看其它项 | 不自动完成、不自动下一项 |
| 保存失败 | 保留异常证据或未来映射/供给草稿和当前项 | 已注册动作使用同幂等键重试 | 成功或放弃 |
| 异常终结成功 | `FormalActionResult` 固定展示异常处理结论、审计号、对象指纹、时间和下一步 | 结果固定展示后按偏好或由用户进入下一项；停止供应项仅可继续查看影响/候选证据 | 用户明确关闭结果；替代选定与恢复发布仍被责任 blocker 阻断 |
| 正式结果不确定 | 当前项不完成、不跳下一项；固定查询入口 | 查询最终结果 | 得到最终状态 |
| 后台重同步 | `BackgroundJobProgress` 显示连接、目录范围、水位、进度和任务号 | 查看任务、处理失败项 | 追平后刷新；失败进入 W29 |
| 字段级隐藏 | 成本等字段标签保留并掩码 | 其它有权动作；缺关键字段权时确认被 blocker 阻止 | 权限更新后重查 |
| 权限收回 | 清除敏感字段与可写状态，保留稳定对象身份 | 返回有权模块 | 权限恢复后重新领取 |

## 10. 响应式、键盘与无障碍

| 视口 | 布局变化 | 保留内容 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 侧栏展开；队列 diff/决策 58/42；关键字段与决策同屏 | 外部身份、来源版本、ERP SKU、价格/区域变化、发布影响、主动作 | 无 |
| 1280×800 | 侧栏可折叠；两栏约 56/44 | 身份、关键 diff、供给摘要和动作 | 媒体/售后说明折叠 |
| 1024×768 | 侧栏图标模式；两栏改上下可切换但队列栏固定 | 版本、映射、关键供给字段、blocker | 次要版本元数据折叠 |
| 768×1024 | 导航抽屉；来源 diff、映射、供给、影响按分段单列 | 阅读差异、选择已有 SKU、简单确认/暂挂 | 复杂供给字段用全屏分段表单 |
| 375×812 | 只保证任务阅读、关键 diff、简单暂挂/退回和结果查看 | 外部身份、变化类型、当前映射、风险、责任人 | 不创建 SKU、不编辑完整供给版本、不切换供应商，提示转桌面 |

- Tab 顺序：队列导航 → 身份与风险 → diff 字段 → SKU 选择 → 供给字段 → 影响摘要 → 决策动作。
- `SequentialProcessBar` 播报当前位置；新项打开后焦点落到外部商品标题。
- diff 不能只用红/绿；每项明确“原值”“新值”和变化类型，掩码字段只播报“已变化”。
- 正式 Dialog 关闭焦点回触发源；任务内动作结果播报后仍停留当前项，异常终结结果先播报，再按已保存偏好或由用户移动到下一项。
- 表单校验失败焦点到 `ValidationSummary` 或首个错误字段；触控目标不小于 44×44。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 今日工作台 / 待办 | W01 / W02 | 已注册来源 `ERROR` 或 `STOPPED` 异常的 `currentWorkItemId=workItemId`、`queueContextId` | 非终结动作后仍为原 `PENDING` / `IN_PROGRESS` 任务；终结成功后回任务结果或继续队列 |
| 商品/SKU/供应商基础资料 | W14 | 外部商品 ID、来源修订、拟建 SKU 上下文 | 建档后回 W21 并重新选择稳定 SKU |
| API 供应商连接 | W20 | 连接 ID、目录同步任务 ID、能力摘要 | 返回当前项并刷新水位 |
| 商品发布 | W22 | 普通发布只传 SKU ID、已确认供给修订 ID、发布 blocker 与来源项 ID；安全暂停恢复在 Q3 确认前禁止跳转 | 普通发布后回中心“发布影响”；恢复 blocker 不得被导航规避 |
| 商城消费/供应商订单 | W25 / W26 | 历史订单快照或供给引用（只读） | 返回原供给版本，不允许改历史 |
| 权限与审计 | W19 | 映射/供给对象 ID、版本和审计事件 | 返回版本时间线 |
| 接口错误与对账 | W29 | 连接、同步任务、目录项、追踪号 | 解决后回原队列项重新查询 |

跨工作面只传稳定身份、版本引用和队列上下文；价格、映射状态、可供性、权限和暂停结果必须在目标工作面重新查询。

## 12. 验收清单

### 12.1 连续处理与布局

- [x] 已注册来源 `ERROR` 和 `STOPPED` 异常可以连续处理；新增、映射、正常供给复核及其它安全暂停原因在类型登记前可连续浏览但始终显示 fail-closed blocker。
- [x] 1440×900 下外部身份、关键 diff、SKU 映射、供给摘要、发布影响和决策同屏可见。
- [x] 队列筛选、位置、当前项和自动下一项可刷新恢复；打开 W14/W22 后队列上下文不丢。
- [x] 任务内动作成功后仍显示 `PENDING` / `IN_PROGRESS` 且不自动下一项；异常终结成功先显示固定结果，再按偏好或由用户继续。

### 12.2 映射与版本

- [x] 外部修订先暂存，未经审核不直接修改 ERP SKU 或商城商品。
- [x] 同一外部商品同一时点只有一个有效 SKU 映射；一个 SKU 可有多个外部供给。
- [x] 供货价和关键供给变化形成不可变新修订，不覆盖旧版本。
- [ ] MOQ、有效期、单位、映射唯一性和版本并发均由服务端校验。
- [ ] 来源版本变化或租约丢失时旧表单不能正式提交。
- [ ] 已注册来源 `ERROR` 和 `STOPPED` 人工异常项都能追溯聚合正式 `work_item`；W21 直接复用 W02 嵌套读模型，不私建领取、租约、对象指纹或完成状态。
- [ ] `HOLD`、`RETURN_FOR_DATA_FIX`、查询和保存证据使用 `WorkItemActionEnvelope`，成功明确保持 `PENDING` / `IN_PROGRESS`；终结异常使用 `CompleteWorkItemEnvelope<ExternalCatalogDecision>`，客户端不提交 `completionAction`。
- [ ] 正常映射/供给类型登记前没有正式写入口；登记后强类型业务写入、审计和任务完成通过 W02 共享完成信封原子提交，单独完成任务不会写业务事实。

### 12.3 主责与下游影响

- [x] 采购负责映射和供给；运营负责 W22 发布；管理员/运维只处理技术异常。
- [x] 供货价变化不自动修改商城销售价；`minimumOrderQuantity` 不自动复制为商城最小购买量。
- [ ] 每个发布版本只绑定一个明确供给修订，不存在自动动态供应商路由。
- [ ] `STOPPED`、不可供、零库存、数据过期或成本/关键供给变化未确认时，系统不等待人工即幂等形成全部暂停子结果/投递/outbox。仅 `STOPPED` 按来源对象/原因/版本创建/复用一个 `BUSINESS_EXCEPTION`；其它原因只返回 blocker/证据，不得借异常类型伪装正常映射/供给复核。
- [ ] 开放正常目录映射/供给复核队列前，已在 `erp-data-model.md` 登记固定 `work_item_type` 并同步 W01/W02 handler；登记前只允许已注册的异常任务，正常确认入口保持 blocker。
- [ ] 人工任务无人领取、租约丢失或处理失败时，商品都不会恢复可下单；替代供给选定/恢复责任未确认时固定返回 `RECOVERY_RESPONSIBILITY_UNCONFIRMED`，不得从 W21 发起 W22 恢复链。
- [ ] 已支付订单永久保留下单时商品、销售价、供应商和成本快照。

### 12.4 状态、权限和响应式

- [ ] 价格、税率和费用按字段权限掩码，提示、审计、导出和缓存不泄露。
- [ ] 已注册异常任务动作有 W02 共享租约、版本、指纹、幂等和结果不确定查询路径；任务字段不在 W21 命令顶层重复定义。
- [ ] §9 全部状态与 1440/1280/1024/768/375 五档视口完成验证。
- [ ] 键盘可完成读 diff、选择 SKU、准备供给草稿、处理已注册异常和继续下一项；未注册正式确认入口不可聚焦为可用按钮。
- [x] 页面不出现原始供应商报文、密钥、签名或协议实现词。

## 13. 待确认事项

| ID | 问题 | 影响 | 建议决策人 | 当前建议 |
| --- | --- | --- | --- | --- |
| Q1 | 名称、规格和来源类目变化中，哪些规范化差异可归为“无业务变化”？ | 人工队列规模和 diff 规则；不改变 §2.3 已固定的安全暂停触发条件 | 采购负责人 + 运营负责人 + 数据治理 | 默认三类变化均进入人工核对；只有服务端已登记的等价归一规则可降为无变化，页面不得自行忽略 |
| Q2 | 外部商品映射和供给版本是否需要不同人员复核？ | 队列步骤和岗位分离 | 采购负责人 + 内控 | 高风险类目和成本变更采用经办/复核分离；普通新映射按配置策略 |
| Q3 | 停止供应后替代供给的选定人、恢复发布的发起/确认人及交接规则是什么？ | 供应商切换和 W22 恢复发布链 | 采购负责人 + 运营负责人 | 确认前 fail-closed：只可准备会话内候选证据；服务端不提供替代供给选定或 W22 恢复 mutation，并固定返回 `RECOVERY_RESPONSIBILITY_UNCONFIRMED` |
| Q4 | 来源媒体、售后说明和能力差异的保留期限与安全检查规则是什么？ | 预览、版本历史和文件资产 | 安全 + 采购 + 运营 | 只长期保留通过安全扫描且进入正式版本的媒体；其它暂存按治理策略到期清理 |
| Q5 | 正常目录映射与供给复核分别登记哪些固定 `work_item_type` 及唯一完成动作？ | 正常人工队列能否实施、去重、责任路由和 W01/W02 handler | 架构负责人 + 采购负责人 + 数据治理 | 先在权威注册表固化正常映射/供给复核类型，再开放队列；`BUSINESS_EXCEPTION` 仅用来源 `ERROR` 和 `STOPPED` 异常处置，不写正常映射或供给确认 |

确认后的关键变化、复核和暂停规则必须写回 §5、§7 和 §8；不得留给页面实现自由解释。

## 14. 业务依据

- `erp-phase-2.md` §3.4–§3.6：统一能力、固定供应关系、人工供应商切换和商城支付与供应商接口解耦。
- `erp-phase-2.md` §7：外部商品暂存、映射审核、价格/库存变化、停止供应和 ERP 发布边界。
- `erp-phase-2.md` §16、§17.1：采购/运营/运维职责及商品同步、映射、发布核心验收。
- `erp-data-model.md` §4.4–§4.5、§6.14：外部商品不可变修订、映射唯一性、供给版本、有效期与内容 HMAC。
- `erp-data-model.md` §6.15、§8.4：每个发布版本唯一供给、暂停规则和历史已支付订单快照。
- `erp-ui-design.md` §3.4–§3.5、§4.4–§4.5、§11：M3 队列、M4 中心、版本/租约/结果不确定和五档响应式。
- `erp-ui-flows.md` §9、§11：跨角色映射协同、供应商异常补偿和历史支付事实不得删除。
