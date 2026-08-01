# W22 · 商品发布

> 状态：草稿<br>
> 页面模式：M2 高密度查询列表 + M4 对象中心<br>
> 主要路由：`/commerce/publications`、`/commerce/publications/:publicationId`<br>
> 主要角色：运营；采购、财务、系统管理员按职责只读或协同<br>
> 最后更新：2026-08-01

## 1. 定位与目标

### 1.1 用户目标

运营进入此工作面，应能完成以下任务：

1. 判断哪些 ERP SKU 尚未发布、等待商城确认、投递失败、已暂停或已在商城生效；
2. 查看一个稳定商品发布对象的全部不可变发布版本；
3. 为目标商城确认名称、规格、图片、分类、销售说明、销售价、区域、销售状态、商品能力和固定供应关系；
4. 发布新版本并持续看到商城确认结果；
5. 在供应停止、零库存、不可供、数据过期或成本/关键供给变化未确认时，直接看见系统已触发的安全暂停、商城投递以及已注册后续任务或强类型 follow-up blocker/证据；人工不承担首次暂停，恢复责任未确认前也不得发起恢复发布；
6. 从发布失败直接进入接口错误处理，不需要在多个技术菜单中寻找消息。

### 1.2 业务目标

- 以服务端已分配的 `product_publication.publicationId` 表达现有发布对象的稳定身份；多商城及唯一性规则未确认前，前端不以 `SKU + 商城` 推导唯一键或创建新对象。
- 以 `product_publication_revision` 保存不可变发布内容；历史订单始终引用下单时的发布修订。
- 一份发布修订恰好绑定一份固定 `supplier_offering_revision`，不在发布时动态比价或路由。
- ERP 只有收到商城成功确认后，才把该发布版本标记为商城已生效。
- 把“发布内容已形成”和“商城是否确认接收”分成两条状态轨，接口失败不得伪装为发布版本不存在。
- 供货价变化不得自动修改商城销售价；销售价、最小购买量和主动业务暂停必须由服务端权限/政策判定。销售价或销项税率变化且复核政策未配置时固定返回 `REVIEW_POLICY_UNCONFIGURED`；恢复责任未确认时固定返回 `RECOVERY_RESPONSIBILITY_UNCONFIRMED`。
- `STOPPED`、零库存、不可供、可供数据过期，以及成本或其它关键供给变化未确认时，系统对受影响在售发布实行 fail-closed：立即幂等形成不可变暂停修订/明确暂停动作、投递记录和 outbox；人工任务不阻塞首次暂停。

### 1.3 不在本工作面完成

- 不同步供应商目录、不处理外部商品映射；进入 W21。
- 不新建或修订 ERP 商品、SKU、分类和媒体资产；进入 W14。
- 不编辑供应商连接、密钥或能力；进入 W20。
- 不在发布详情直接修改已生效版本，也不覆盖旧图片引用。
- 不处理供应商订单、商城消费、结算或应付；分别进入 W25、W26、W27、W12。
- 不在本工作面编辑接口报文或数据库状态；失败闭环进入 W29。

## 2. 用户、权限与数据范围

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 运营 | 发布列表 | 被授权商城、类目和业务组织内的现有发布对象 | 按稳定 ID 查看/维护现有发布、准备普通新版本、按政策提交发布、暂停下单、查看投递；当前不得创建新发布对象或恢复安全暂停对象 |
| 采购 | W21，可进入发布详情 | 其负责供应商或供给关系关联的发布 | 查看固定供给、处理供给阻塞；不改销售价与销售文案 |
| 财务 | 按需从商品或经营页面进入 | 被授权组织的价格、税率与发布历史 | 只读核对价格及税口径；是否参与发布确认见 Q2 |
| 系统管理员 / 运维 | 失败筛选或 W29 | 全部获授权商城的投递摘要 | 查看投递、查询结果、进入错误中心；不改业务发布内容 |
| 管理层 | 无默认入口 | 授权范围内只读 | 查看发布覆盖与异常，不执行发布 |

### 2.1 权限表达

| 情况 | 页面行为 |
| --- | --- |
| 无 W22 模块权限 | 导航不展示；直接访问显示无权限页 |
| 有模块权限但无商城或商品数据范围 | 显示“当前角色没有可管理的商城商品范围”，不展示虚假的 0 指标 |
| 可查看但不可发布 | 展示全部有权字段；正式动作禁用并显示服务端 `actionBlockers` |
| 无供货价权限 | 固定供给身份、可供状态和能力可见，供货价掩码；商城销售价按独立字段权限处理 |
| 无媒体原文件权限 | 展示安全缩略图；不提供原文件下载 |
| 页面打开期间权限被收回 | 清除已揭示价格与媒体签名地址，内容切为无权限或字段掩码态 |

前端只渲染服务端返回的 `allowedActions`、`actionBlockers` 和 `fieldPermissions`。是否可发布不能仅由当前状态在浏览器推导。

### 2.2 安全暂停责任边界

- 安全暂停由目录/供给领域事件触发，不是运营页面按钮，也不等待运营、采购或管理员领取任务。触发范围固定为：外部商品 `STOPPED`、库存为零、明确不可供、可供数据超过服务端新鲜度阈值，以及供货价、进项税率、费用、MOQ、区域或商品能力变化尚未确认。
- 安全暂停以“来源对象 + 暂停原因 + 来源版本”作为事件级幂等身份，先冻结全部受影响在售发布集合；同一领域事务为每个发布写本地暂停、不可变暂停修订/动作、`product_publication_delivery` 和 outbox。若触发原因为 `SUPPLIER_STOPPED`，整个来源事件再只创建一个正式后续 `work_item`，各发布子结果引用同一任务；其它原因只固定记录与 cause 匹配的 follow-up blocker 与证据，不伪造任务。重复事件返回原操作和原子结果，如有任务则返回原任务。
- 仅 `SUPPLIER_STOPPED` 后续任务复用已注册 `work_item_type=BUSINESS_EXCEPTION`，业务对象是触发暂停的 `SUPPLIER_EXTERNAL_PRODUCT` 或 `SUPPLIER_OFFERING`，并由服务端固定 handler 路由 W21。零库存、不可供、过期、成本/关键供给变化等正常核对在权威注册表增加固定类型前保持 W21 实施 blocker，不得借用 `BUSINESS_EXCEPTION`。商城投递结果未知则另由已注册 `INTEGRATION_RESULT_UNKNOWN` 进入 W29。
- 已注册人工任务当前只用于核对来源、固定影响和准备替代候选证据，不能选定替代供给或发起恢复发布。任务无人领取、租约过期、处理失败或被转交，以及其它原因尚无注册任务，都不能把商品恢复为可下单。
- 恢复发起人、确认人与采购/运营交接还未确认。因此只允许系统安全暂停和人工暂停；任何从安全暂停转为 `ON_SALE` 的提交都必须被 `RECOVERY_RESPONSIBILITY_UNCONFIRMED` 阻断。库存重新出现、来源恢复可用或候选供给已就绪均不得自动解除暂停。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 查看发布列表 | 侧栏“商品发布” | `/commerce/publications`；筛选进入 URL | 返回恢复筛选、分页和滚动位置 |
| 查看发布对象 | 列表行、W14 商品、W21 供给 | `/commerce/publications/:publicationId?section=overview`；稳定发布 ID 为页签身份 | 返回来源工作面并聚焦原对象 |
| 查看指定版本 | 版本时间线 | `?section=revisions&revision={revisionId}` | 后退恢复上一节与版本 |
| 处理投递失败 | 指标、待办或 W29 | `?section=delivery&delivery={deliveryId}` | 返回保留失败筛选 |
| 从全局搜索打开 | SKU、商品名、发布编号 | 聚焦已有发布页签或创建页签 | 原页签保留 |
| 刷新浏览器 | 任意对象中心状态 | 恢复稳定对象、锚点和选中版本；不恢复会话内编辑或临时确认框，刷新前必须明确提示未提交输入将丢失 | 当前发布对象 |

对象中心 TaskTabs 身份为 `publication:{publicationId}`，标题为 `发布 · {SKU编码}`。同一稳定发布对象重复打开只聚焦原页签；版本切换不产生新任务页签。会话内编辑存在时页签显示脏标记，关闭或刷新前必须明确提示“输入将丢失”。当前不提供服务端草稿保存 mutation、自动保存、`localStorage` / `IndexedDB` 持久化或 TanStack Query 持久缓存。

## 4. 页面布局

### 4.1 列表页（1440×900）

```text
┌ PageHeader：商品发布                  数据水位 09:36 [刷新] [新建发布·已阻断]
├ MetricStrip：[待发布] [待商城确认] [失败/转人工] [商城已生效] [已暂停]
├ ListToolbar：SavedView | 搜索 | 商城 | 发布状态 | 投递状态 | 类目 | 更多筛选
├ 筛选摘要 / SelectionScopeBar（仅有批量动作时）
├ BusinessTableFrame
│  SKU/商品（固定） | 目标商城 | 当前生效版 | 待确认版 | 固定供给 | 销售价
│  发布状态 | 投递状态 | 商城确认时间 | 负责人 | 操作（固定）
└ 服务端分页
```

1440×900 下页头、指标、工具栏和分页同时存在时，至少露出 6–8 条有效数据行，默认 36px 行高。

### 4.2 对象中心（1440×900）

```text
┌ PageHeader object-chrome：商品发布 › SKU                 [返回] [刷新] ─┐
├ DocumentHeader compact：商品名 [发布状态]                                 │
│  发布编码 · 最新版本 · SKU · 目标商城 · 商城生效版                        │
│ 状态轨：发布内容 ── 投递 ── 商城确认              [准备新版本] [更多]
├ 锚点：概览 | 发布内容 | 媒体 | 固定供给 | 投递与版本 | 审计
├ 概览：商城生效版本、最新待确认版本、安全暂停原因/来源、有效期、责任人、阻塞原因
├ 发布内容：名称/规格/类目/说明/销售价/税率/单位/区域/最小购买量/能力
├ 媒体：主图、轮播、详情图及替代文本
├ 固定供给：唯一供给版本、供应商、可供状态、有效期、能力；价格按权限
├ 投递与版本：不可变版本时间线 + 投递尝试 + 商城确认 + 失败摘要
└ 审计：创建、提交、暂停、重试、人工处理记录
```

### 4.3 区域说明

| 区域 | 目的 | 主组件 | 是否固定 |
| --- | --- | --- | --- |
| 页头与状态轨 | 同时辨认稳定发布、版本和商城确认 | `PageHeader object-chrome` + `DocumentHeader density="compact"` `StatusTrackSummary` | 顶部固定 |
| 指标与工具栏 | 快速进入当前处理水位 | `MetricStrip` `ListToolbar` | 列表顶部 |
| 发布内容 | 阅读当前选中修订的完整商城内容 | `DocumentSection` `DocumentSummary` | 否 |
| 媒体 | 验证安全资产、用途、顺序和替代文本 | 媒体列表 / 预览 | 否 |
| 固定供给 | 明确本版本唯一履约来源 | `RelatedDocumentList` | 否 |
| 版本与投递 | 区分业务版本和接口投递状态 | `RevisionTimeline` `AsyncSectionState` | 否 |
| 正式结果区 | 固定展示发布、暂停或重试结果 | `FormalActionResult` | 动作后固定在主区顶部 |

## 5. 展示内容与字段

### 5.1 列表与身份

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 身份 | `publicationId` / `skuCode` | 发布编号 / SKU | `product_publication`、`sku` | 稳定身份；SKU 列固定 | 有对象查看权限可见 |
| 商品 | `name` / `specification` | 商品 / 规格 | 当前展示修订快照 | 不用当前商品主档覆盖历史修订 | 基础可见 |
| 商城 | `targetMallName` | 目标商城 | `target_mall_id` 映射 | 稳定商城身份 | 按商城数据范围 |
| 状态 | `publicationStatus` | 发布状态 | `product_publication.status` | 草稿、待发布、商城生效、暂停、失效 | 全部有权用户 |
| 版本 | `currentAckedRevisionNo` | 商城生效版本 | 商城已确认修订 | 无确认时明确“尚未生效” | 全部有权用户 |
| 版本 | `latestRevisionNo` | 最新发布版本 | 最新不可变修订 | 与商城生效版本并列，不混成一个值 | 全部有权用户 |
| 投递 | `deliveryStatus` | 商城接收 | `product_publication_delivery` | 待发送、重试中、已确认、失败、转人工 | 运维摘要不含敏感报文 |
| 价格 | `salesPriceGross` / `salesTaxRate` | 含税销售价 / 销项税率 | 发布修订 | 人民币分精度；明确含税 | 按价格字段权限 |
| 供给 | `offeringLabel` / `availability` | 固定供给 / 可供状态 | 供给修订查询投影 | 只展示本修订绑定的一条 | 供货价另行授权 |
| 安全暂停 | `safetyPause` | 安全暂停 | 暂停操作、后续任务或强类型 follow-up blocker 查询投影 | 原因、来源版本、本地提交时间、商城投递；`SUPPLIER_STOPPED` 显示唯一后续任务，其它已落库原因显示与 cause 匹配的唯一 blocker，不能只写“待运营确认” | 业务摘要按对象权，技术错误脱敏 |

### 5.2 发布修订内容

| 字段 | 用户文案 | 数据来源 | 口径与校验 |
| --- | --- | --- | --- |
| `skuRevisionId` | 商品修订 | `product_publication_revision.sku_revision_id` | 展示稳定版本号，不被主档后续变化覆盖 |
| `categoryId` | 商城发布类目 | 已审核 ERP 发布类目映射 | 不能直接使用旧商城分类 ID |
| `name` / `specification` | 展示名称 / 规格 | 发布修订快照 | 提交后不可原位修改 |
| `salesDescription` | 商城销售说明 | 发布修订快照 | 提交发布必填 |
| `salesPriceGross` | 含税销售价 | 发布修订 | 与供货价分开；供货价变化不自动写入 |
| `salesTaxRate` | 销项税率 | 发布修订 | 不得用供应商进项税率替代 |
| `baseUnitCode` | 计量单位 | 发布修订 | 与数量和最小购买量使用同一基础单位 |
| `minimumPurchaseQuantity` | 最小购买量 | 运营确认的发布策略 | 必须大于 0；不得从供应商最小订购量自动复制 |
| `salesRegion` | 可销售区域 | 发布修订 | 结构化区域摘要，可展开查看完整范围 |
| `saleStatus` | 商城销售状态 | 发布修订 | 上架、下架、暂停下单 |
| `productCapabilities` | 取消 / 退款 / 物流等能力 | 发布修订 | 来源于已确认能力，不由前端推断 |
| `validFrom` / `validTo` | 生效区间 | 发布修订 | 使用公司工作时区，区间冲突由服务端校验 |
| `contentHash` | 内容指纹 | 服务端 | 默认不展示全文；审计区可复制短摘要 |

### 5.3 媒体与投递

| 字段 | 用户文案 | 数据来源 | 展示规则 |
| --- | --- | --- | --- |
| `mediaRole` / `sortNo` | 主图 / 轮播图 / 详情图 | 发布修订媒体关系 | 主图恰好一张；按角色和顺序展示 |
| `altText` | 图片说明 | 发布修订媒体关系 | 每张媒体可读；缺失时阻断提交 |
| `securityScanStatus` | 安全检查 | `file_asset` 查询投影 | 未通过或保留期无效时阻断提交 |
| `attemptCount` / `lastAttemptAt` | 已尝试 / 最近尝试 | 发布投递 | 不把重试次数等同版本数 |
| `mallAckAt` / `mallVersion` | 商城确认时间 / 商城版本 | 发布投递 | 只有明确成功确认时展示 |
| `errorCode` / `errorSummary` | 失败原因 | 发布投递脱敏摘要 | 不显示原始报文、密钥或堆栈 |

## 6. 搜索、筛选、排序与默认视图

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 搜索 | 空 | `q` | 精确匹配发布编号、SKU；模糊匹配商品名 |
| 目标商城 | 当前角色默认商城或全部有权商城 | `mall` | 服务端按数据范围过滤 |
| 发布状态 | 有效对象 | `publicationStatus` | 默认排除失效，可显式查看历史 |
| 投递状态 | 全部 | `deliveryStatus` | “待商城确认”同时包含待发送、发送/重试中，不含失败 |
| 类目 | 全部 | `category` | 使用发布修订的已审核类目 |
| 固定供给 | 全部 | `supplier` / `availability` | 支持供应商、不可供、数据过期筛选 |
| 更新时间 | 最近更新降序 | `sort=updatedAt.desc` | 服务端排序；身份列稳定 |
| 保存视图 | 无 | `view` | 只保存筛选、排序和列，不保存业务数据 |

- 指标点击使用按钮语义、选中态和筛选摘要；浏览器后退恢复上一筛选。
- 单击列表行打开 `detail` 半屏，包含身份、选中版本关键内容、唯一固定供给、投递状态和错误摘要；重操作进入对象中心。
- 批量操作只允许对服务端冻结选择快照执行；预览后逐项重验权限、当前版本和投递状态。
- 导出为后台任务，字段清单和掩码规则冻结；下载时重新鉴权。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 新建发布对象 | 列表页头 | 多商城/唯一性规则未确认，入口强制禁用 | 不打开确认框 | 无创建命令；固定显示 `PUBLICATION_IDENTITY_POLICY_UNCONFIRMED` | 规则写入权威合同并由服务端提供稳定创建命令后重新查询；不用 `SKU + 商城` 在前端去重 |
| 准备普通新版本 | 现有对象中心 | `PREPARE_REVISION`；以 `publicationId` 定位的现有对象有效，且不是从安全暂停恢复 `ON_SALE` | 无正式确认 | 以指定历史/当前版本为起点进入当前页签会话编辑态；不调用保存 mutation | 版本变化提示重新取基线；刷新/关闭前提示会话输入将丢失 |
| 会话内编辑 | 编辑区 | 有现有对象编辑权限；基线版本有效 | 无 | 只更新当前 TaskTab 内存中的输入和脏标记；不持久化、不自动保存、不形成发布修订 | 刷新/关闭前明确提示会丢失；用户取消离开则继续保留当前会话输入 |
| 提交发布 | 编辑区主动作 | `PUBLISH`；必填字段、主图、安全扫描、唯一供给、类目和有效期校验通过；服务端返回 `publishGate.kind=READY` | `FormalActionConfirmDialog` 展示商城、价格、状态、供给、生效时间和复核政策结论 | 形成不可变发布修订和 outbox；固定展示版本号与“等待商城确认” | 销售价/销项税率变化且政策未配置时返回 `REVIEW_POLICY_UNCONFIGURED`；安全暂停恢复返回 `RECOVERY_RESPONSIBILITY_UNCONFIRMED`；结果未知时查询原请求，不创建新版本重试 |
| 系统安全暂停 | 目录/供给事件处理器（非页面按钮） | 固定安全原因命中且存在受影响在售发布；不要求人工权限或任务租约 | 无人工确认 | 冻结受影响集合，同事务幂等提交所有本地暂停子结果/投递/outbox；`SUPPLIER_STOPPED` 另创建唯一 `BUSINESS_EXCEPTION`，其它原因固定返回与 cause 匹配的 `followUpBlocker` | 结果未知按原幂等键查询；投递失败转 W29，本地仍保持暂停 |
| 人工发布暂停版本 | 更多动作 | `PAUSE`；当前对象可暂停 | 展示受影响商城与生效时间 | 形成新的暂停发布修订并投递 | 失败不覆盖旧版本；按原修订查询/重试 |
| 重试投递 | 投递区 | `RETRY_DELIVERY`；明确可重试且未在发送中 | 展示版本、商城、原幂等键 | 继续原投递，结果区展示尝试编号 | 超时先查询商城确认；仍未知转 W29 |
| 查询最终结果 | 结果未知状态 | `QUERY_RESULT` | 无 | 更新明确确认、明确失败或仍未知 | 仍未知保留当前状态与人工处理入口 |
| 批量重试失败项 | 列表选择栏 | 有批量权限；服务端选择快照有效 | `BatchImpactPreview` 展示逐状态数量 | 创建后台任务，各项沿原幂等键处理 | 显示成功/跳过/失败；不修改发布内容 |
| 打开错误处理 | 失败摘要 | 有 W29 权限 | 无 | 打开对应错误任务 | 权限不足时保留脱敏失败说明 |

### 7.1 正式动作边界

- “提交发布”形成新的不可变发布修订；“重试投递”不能形成新修订。
- 消息幂等依据为服务端已分配的 `publicationId + revisionId + destinationIdentity`，前端同时携带唯一 `requestId` 处理接口超时；不从 SKU/商城字段推导发布对象唯一性。
- 销售价或销项税率是否变化、是否需要复核以及复核是否满足，全部使用服务端 `publishGate`。即使前端比较为“无变化”也不得自行跳过政策；价格/税率变化且政策未配置时必须 fail-closed。
- 系统安全暂停的事件幂等依据为“来源对象 + 暂停原因 + 来源版本”；受影响发布子结果再按“发布对象 + 原因 + 来源版本”去重。冻结影响集合后，所有本地暂停子结果、暂停修订/动作、投递记录和 outbox 必须原子提交；仅 `SUPPLIER_STOPPED` 同事务创建唯一已注册后续任务，其它原因同事务记录与 cause 匹配的 blocker/evidence。outbox 消费和商城确认可以异步，但任何失败都不能回滚为可下单。
- 安全原因仍有效时，服务端必须阻断任何 `ON_SALE` 提交；来源重新可用只移除恢复 blocker 的一部分，不自动创建上架版本。即使供给已有效，恢复责任规则未确认前仍固定返回 `RECOVERY_RESPONSIBILITY_UNCONFIRMED`；当前只允许安全暂停或人工暂停继续执行。
- 结果未知时先查询原请求或商城确认，不以再次点击“发布”解决。
- 商城业务拒绝、鉴权失败和字段映射错误直接转人工；临时网络或限流才按策略自动重试。
- 商城确认前，界面不得把版本显示为“商城已生效”。
- 固定供给、图片、销售价或销售状态变化必须形成新修订；不得对历史修订就地编辑。

## 8. 数据契约

本节固定 UI 需要的查询和提交语义，不固定最终 HTTP 路径。

### 8.1 列表查询

```ts
type ProductPublicationListQuery = {
  q?: string
  mallIds?: string[]
  publicationStatuses?: string[]
  deliveryStatuses?: string[]
  categoryIds?: string[]
  supplierIds?: string[]
  availability?: "available" | "unavailable" | "stale"
  sort: string
  page: number
  pageSize: number
}

type SafetyPauseCause =
  | "SUPPLIER_STOPPED"
  | "ZERO_INVENTORY"
  | "SUPPLY_UNAVAILABLE"
  | "AVAILABILITY_STALE"
  | "COST_CHANGE_UNCONFIRMED"
  | "CRITICAL_SUPPLY_CHANGE_UNCONFIRMED"

type SafetyPauseFollowUpWorkItemRef = {
  workItemId: string
  workItemType: "BUSINESS_EXCEPTION"
  businessObjectType: "SUPPLIER_EXTERNAL_PRODUCT" | "SUPPLIER_OFFERING"
  businessObjectId: string
  subjectVersion: string
  subjectHash: string
  handlerKey: string // 服务端固定注册并路由 W21
}

type SafetyPauseNoTaskBlocker = {
  code: "NO_MANUAL_FOLLOW_UP_TASK_BY_CURRENT_POLICY"
  message: string
  evidenceReference: string
}

type SafetyPauseReviewRegistrationBlocker = {
  code: "NORMAL_REVIEW_WORK_ITEM_TYPE_UNREGISTERED"
  message: string
  evidenceReference: string
}

type SafetyPauseFollowUpBlocker =
  | SafetyPauseNoTaskBlocker
  | SafetyPauseReviewRegistrationBlocker

type SafetyPauseAffectedPublicationView =
  | {
      publicationId: string
      pauseArtifactKind: "REVISION"
      pauseRevisionId: string
      pauseActionId?: never
      deliveryId: string
      outboxMessageId: string
    }
  | {
      publicationId: string
      pauseArtifactKind: "ACTION"
      pauseRevisionId?: never
      pauseActionId: string
      deliveryId: string
      outboxMessageId: string
    }

type KnownSafetyPauseOperationBase = {
  operationId: string
  resultStatus: "COMMITTED" | "ALREADY_SAFE"
  sourceObjectType: "SUPPLIER_EXTERNAL_PRODUCT" | "SUPPLIER_OFFERING"
  sourceObjectId: string
  sourceVersion: string
  subjectHash: string
  availabilityEffect: "PAUSED"
  affectedPublications: [SafetyPauseAffectedPublicationView, ...SafetyPauseAffectedPublicationView[]]
  committedAt: string
}

type SystemSafetyPauseOperationView =
  | (KnownSafetyPauseOperationBase & {
      cause: "SUPPLIER_STOPPED"
      followUpWorkItem: SafetyPauseFollowUpWorkItemRef
      followUpBlocker?: never
    })
  | (KnownSafetyPauseOperationBase & {
      cause: "ZERO_INVENTORY" | "SUPPLY_UNAVAILABLE" | "AVAILABILITY_STALE"
      followUpWorkItem?: never
      followUpBlocker: SafetyPauseNoTaskBlocker
    })
  | (KnownSafetyPauseOperationBase & {
      cause: "COST_CHANGE_UNCONFIRMED" | "CRITICAL_SUPPLY_CHANGE_UNCONFIRMED"
      followUpWorkItem?: never
      followUpBlocker: SafetyPauseReviewRegistrationBlocker
    })
  | {
      operationId: string
      resultStatus: "UNKNOWN"
      cause: SafetyPauseCause
      sourceObjectType: "SUPPLIER_EXTERNAL_PRODUCT" | "SUPPLIER_OFFERING"
      sourceObjectId: string
      sourceVersion: string
      subjectHash: string
      originalIdempotencyKey: string
      availabilityEffect: "FAIL_CLOSED_PENDING_RESULT"
      affectedPublications?: never
      followUpWorkItem?: never
      followUpBlocker?: never
      committedAt?: never
    }

type PublicationCreationBlocker = {
  code: "PUBLICATION_IDENTITY_POLICY_UNCONFIRMED"
  message: string
}

type PublicationPublishGate =
  | {
      kind: "READY"
      gateVersion: string
      submissionKind: "NORMAL"
      priceOrTaxChanged: boolean
      policyVersion: string
      reviewDisposition: "NOT_REQUIRED" | "SATISFIED"
      reviewEvidenceReference?: string
    }
  | {
      kind: "REVIEW_POLICY_UNCONFIGURED"
      gateVersion: string
      submissionKind: "NORMAL"
      priceOrTaxChanged: true
      blocker: { code: "REVIEW_POLICY_UNCONFIGURED"; message: string }
    }
  | {
      kind: "REVIEW_BLOCKED"
      gateVersion: string
      submissionKind: "NORMAL"
      priceOrTaxChanged: boolean
      policyVersion: string
      blocker: { code: "REVIEW_REQUIRED" | "REVIEW_PENDING" | "REVIEW_REJECTED"; message: string }
    }
  | {
      kind: "RECOVERY_RESPONSIBILITY_UNCONFIRMED"
      gateVersion: string
      submissionKind: "RECOVERY"
      blocker: { code: "RECOVERY_RESPONSIBILITY_UNCONFIRMED"; message: string }
    }

type ProductPublicationRow = {
  publicationId: string
  skuId: string
  skuCode: string
  productName: string
  specification: string
  targetMallId: string
  targetMallName: string
  publicationStatus: string
  currentAckedRevisionId?: string
  currentAckedRevisionNo?: number
  latestRevisionId?: string
  latestRevisionNo?: number
  salesPriceGross?: string
  salesTaxRate?: string
  fixedOffering: {
    offeringRevisionId: string
    supplierName: string
    availability: string
    fieldVisibility: Record<string, boolean>
  }
  safetyPause?: SystemSafetyPauseOperationView
  latestDelivery?: {
    deliveryId: string
    status: string
    attemptCount: number
    mallAckAt?: string
    errorSummary?: string
  }
  updatedAt: string
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
}

type ProductPublicationListResult = {
  items: ProductPublicationRow[]
  total: number
  page: number
  pageSize: number
  permissionVersion: string
  dataScopeVersion: string
  queriedAt: string
  creationBlocker: PublicationCreationBlocker
}
```

列表响应的 `creationBlocker` 在多商城/唯一性规则确认前必填，创建入口必须禁用；已有行仍按服务端 `publicationId` 查看和维护。Query Key 必须包含当前用户、角色、数据范围版本及全部筛选。

### 8.2 对象中心查询

```ts
type ProductPublicationRevisionView = {
  revisionId: string
  revisionNo: number
  skuRevisionId: string
  supplierOfferingRevisionId: string
  categoryId: string
  categoryLabel: string
  name: string
  specification: string
  salesDescription: string
  minimumPurchaseQuantity: string
  salesPriceGross: string
  salesTaxRate: string
  baseUnitCode: string
  salesRegion: unknown
  saleStatus: string
  productCapabilities: string[]
  validFrom: string
  validTo?: string
  contentHash: string
  media: Array<{
    fileAssetId: string
    mediaRole: "MAIN" | "CAROUSEL" | "DETAIL"
    sortNo: number
    altText: string
    thumbnailUrl: string
    securityScanStatus: string
  }>
}

type PublicationDeliveryView = {
  deliveryId: string
  revisionId: string
  targetMallId: string
  status: string
  attemptCount: number
  lastAttemptAt?: string
  mallAckAt?: string
  mallVersion?: string
  errorCode?: string
  errorSummary?: string
}

type ProductPublicationView = {
  identity: {
    publicationId: string
    skuId: string
    skuCode: string
    targetMallId: string
    targetMallName: string
  }
  status: string
  currentAckedRevisionId?: string
  selectedRevision: ProductPublicationRevisionView
  revisions: Array<{
    revisionId: string
    revisionNo: number
    saleStatus: string
    createdAt: string
    createdBy: string
    contentHash: string
    deliverySummary: string
  }>
  deliveries: PublicationDeliveryView[]
  safetyPause?: SystemSafetyPauseOperationView
  publishGate: PublicationPublishGate
  freshness: { queriedAt: string; integrationUpdatedAt: string }
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
  fieldPermissions: Record<string, "full" | "masked" | "hidden">
  objectVersion: string
}
```

`ProductPublicationRevisionView` 必须包含 §5.2、§5.3 的完整快照、唯一 `supplierOfferingRevisionId`、媒体列表和内容指纹。页面切换历史修订时不得以当前主数据补写缺失值。

### 8.3 提交

```ts
type PublishRevisionCommand = {
  publicationId: string
  expectedObjectVersion: string
  expectedPublishGateVersion: string
  requestId: string
  content: {
    skuRevisionId: string
    supplierOfferingRevisionId: string
    categoryId: string
    name: string
    specification: string
    salesDescription: string
    minimumPurchaseQuantity: string
    salesPriceGross: string
    salesTaxRate: string
    baseUnitCode: string
    salesRegion: unknown
    saleStatus: "ON_SALE" | "OFF_SALE" | "PAUSED"
    productCapabilities: string[]
    validFrom: string
    validTo?: string
    media: Array<{ fileAssetId: string; mediaRole: string; sortNo: number; altText: string }>
  }
}

type PublishRevisionResult = {
  operationId: string
  publicationId: string
  revisionId: string
  revisionNo: number
  deliveryId: string
  deliveryStatus: "PENDING"
  committedAt: string
}

// 领域事件消费者使用；前端只读操作结果，不发起此命令。
type SystemSafetyPauseTrigger = {
  cause: SafetyPauseCause
  sourceObjectType: "SUPPLIER_EXTERNAL_PRODUCT" | "SUPPLIER_OFFERING"
  sourceObjectId: string
  sourceVersion: string
  subjectHash: string
  affectedPublicationIds: string[] // 服务端冻结的完整在售影响集合
  occurredAt: string
  idempotencyKey: string
}

```

- 表单使用 TanStack Form；服务端返回字段级错误和置顶校验摘要。
- `expectedObjectVersion` / 内容指纹用于冲突检查；禁止静默覆盖他人新修订。
- 发布工作副本的服务端持久化策略尚未确认。当前只保留 TaskTab 会话内输入，不定义草稿保存 mutation、不自动保存、不写本地持久存储；刷新/关闭前必须明确提示输入将丢失。
- `expectedPublishGateVersion` 与对象版本一起防止绕过复核/恢复 blocker。服务端提交时重算 `PublicationPublishGate`；只有 `READY` 可写不可变修订，`REVIEW_POLICY_UNCONFIGURED`、`REVIEW_BLOCKED` 和 `RECOVERY_RESPONSIBILITY_UNCONFIRMED` 都不得写修订/outbox。
- 投递是后台流程；本地发布提交成功和商城确认成功必须分别反馈。
- `SystemSafetyPauseTrigger` 由服务端从可信目录/供给事实生成，浏览器不得构造。重复触发必须返回同一操作结果；调用方得到 `UNKNOWN` 时只按原 `idempotencyKey` 查询，不得新建暂停版本重试。
- 安全暂停事务以本地 fail-closed 为成功边界：冻结的全部受影响发布暂停子结果、投递和 outbox 要么一起提交，要么一起不提交；`SUPPLIER_STOPPED` 还必须同事务提交唯一后续 `work_item`，其它原因则同事务提交与 cause 匹配的 `followUpBlocker`。商城投递失败由各自原 outbox 重试并进入 W29，不能删除任何暂停结果。
- `COMMITTED` / `ALREADY_SAFE` 都是已落库结果，`affectedPublications` 必须覆盖首次冻结的非空集合，每项通过 `pauseArtifactKind` 强制且只能返回 `pauseRevisionId` / `pauseActionId` 之一，并返回 `deliveryId`、`outboxMessageId`。若 `cause=SUPPLIER_STOPPED`，结果必须且只返回一个 `followUpWorkItem`，其 `workItemType=BUSINESS_EXCEPTION`、业务对象与触发来源一致、handler 路由 W21。`ZERO_INVENTORY` / `SUPPLY_UNAVAILABLE` / `AVAILABILITY_STALE` 强制返回 `NO_MANUAL_FOLLOW_UP_TASK_BY_CURRENT_POLICY` blocker，明确不伪造人工任务；成本/关键供给变化强制返回 `NORMAL_REVIEW_WORK_ITEM_TYPE_UNREGISTERED`。这些分支均不得返回 `BUSINESS_EXCEPTION`。
- `resultStatus=UNKNOWN` 是独立分支，不得返回 `affectedPublications`、`committedAt`、`followUpWorkItem` 或 `followUpBlocker`；页面使用 `originalIdempotencyKey` 查询原操作，并以 `FAIL_CLOSED_PENDING_RESULT` 保持不可下单，不能用“没有后续任务”推断暂停未发生。列表、对象中心和操作结果全部复用同一 `SystemSafetyPauseOperationView`，不重新定义可选任务/blocker 结构。

### 8.4 前端边界

- 前端只格式化金额、税率、时间和区域摘要，不计算正式销售价、税额、供货利润或发布状态。
- 可在表单内提示必填和图片数量，但服务端必须再次校验唯一固定供给、安全扫描、类目映射、有效期和权限。
- “待商城确认”可由服务端投递状态映射为展示标签；不能按本地计时擅自改为失败。
- 媒体签名 URL、价格掩码和权限结果不进入长期浏览器缓存。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 列表或对象中心按成稿结构显示 Skeleton | 应用壳导航可用 | 查询完成后原位替换 |
| 刷新 | 保留旧行和当前修订，标记正在刷新 | 允许阅读；正式动作提交前重新校验 | 成功更新水位，失败保留旧数据 |
| 空数据 | “尚无商品发布”，同时展示 `PUBLICATION_IDENTITY_POLICY_UNCONFIRMED` | 无可用新建动作 | 多商城/唯一性规则写入权威合同并实现服务端创建命令后重查 |
| 筛选无结果 | 展示筛选摘要 | 清除筛选 | 恢复默认视图 |
| 无数据范围 | 不展示 0 指标和对象内容 | 查看当前角色/申请范围 | 权限更新后重查 |
| 查询失败 | 有缓存则保留并标记失败；无缓存显示 `BusinessFailureState` | 重试 | 查询成功 |
| 数据陈旧 | 显示查询水位；投递状态超过 SLA 时提示 | 刷新、查看 W29 | 服务端状态追平 |
| 字段级隐藏 | 字段标签保留，价格或供给值掩码 | 其它有权动作可用 | 权限变化后重查 |
| 会话内编辑中 | 页签显示脏标记和“未持久化”；不显示“已保存”或自动保存状态 | 继续编辑、提交正式发布或放弃输入 | 关闭/刷新前明确提示；继续离开则输入丢失 |
| 版本冲突 | 显示现版本与编辑基线差异 | 刷新比较、复制输入到新基线 | 不覆盖服务器版本 |
| 发布身份规则未确认 | 新建入口禁用并显示 `PUBLICATION_IDENTITY_POLICY_UNCONFIRMED`；已有对象可按 `publicationId` 维护 | 查看/维护已有对象；不能新建 | 规则和服务端创建合同落地后重查 |
| 复核政策未配置 | 销售价/销项税率变化时显示 `REVIEW_POLICY_UNCONFIGURED`，提交禁用 | 查看 diff 和责任提示；不能发布 | 服务端政策配置并重新计算 `publishGate` |
| 恢复责任未确认 | 安全暂停对象的 `ON_SALE` 编辑/提交显示 `RECOVERY_RESPONSIBILITY_UNCONFIRMED` | 只允许查看、查询投递和安全/人工暂停；不能恢复 | 责任与交接规则写入权威合同后重查 |
| 发布提交中 | 防重复提交，显示请求编号 | 查询请求状态 | 明确成功/失败后结束 |
| 发布成功、待商城确认 | 固定结果区显示发布版本和投递编号 | 查看投递、继续其它工作 | 商城确认后刷新状态轨 |
| 安全暂停已落库 | `COMMITTED` / `ALREADY_SAFE` 高风险 Alert 展示固定原因、来源版本、暂停操作号和投递号；`SUPPLIER_STOPPED` 强制显示唯一 `followUpWorkItem`，其它原因强制显示唯一 `followUpBlocker`；本地已不可下单 | 查询投递；有已注册任务时打开 W21，其它原因只查看 blocker/证据；不可直接恢复 | 商城确认暂停后更新投递轨；恢复责任未确认期间本地始终保持暂停 |
| 安全暂停结果未知 | `UNKNOWN` 分支不展示影响集、提交时间、后续任务或 blocker；明确标记 `FAIL_CLOSED_PENDING_RESULT`，不把对象显示为可销售 | 按 `originalIdempotencyKey` 查询原暂停操作、进入 W29 | 得到 `COMMITTED` 或 `ALREADY_SAFE`；不得创建第二暂停版本 |
| 投递失败 | 发布版本仍可读，投递轨显示失败摘要 | 按权限重试或进入 W29 | 原幂等投递成功 |
| 结果未知 | 不显示为商城生效，不创建新修订 | 查询最终结果、转人工 | 得到明确确认或失败 |
| 后台批量任务 | `BackgroundJobProgress` 显示成功/跳过/失败 | 查看逐项结果 | 原任务续查，不重复发起 |
| 权限收回 | 清除敏感缓存，切无权限/掩码态 | 返回有权模块 | 权限恢复后重查 |

## 10. 响应式、键盘与无障碍

| 视口 | 布局变化 | 保留内容 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 侧栏展开；列表 6–8 行；对象中心内容与版本轨并列 | SKU、商城、双版本、发布/投递状态、主动作 | 无 |
| 1280×800 | 侧栏可折叠；次要列进入列设置 | 身份、商城、销售价、状态、操作 | 负责人和最近尝试时间可隐藏 |
| 1024×768 | 图标侧栏；详情覆盖式；工具栏换行 | 身份、当前生效版、待确认版、失败原因 | 媒体与供给摘要折叠 |
| 768×1024 | 导航抽屉；表格横向滚动；对象中心单列 | 身份列与操作列固定；发布和投递两条状态轨 | 高级筛选进面板，版本字段缩为卡片 |
| 375×812 | 紧凑卡片；只读当前发布和投递结果 | SKU、商城、版本、状态、失败入口 | 不提供新建、复杂编辑、批量、列设置和正式发布 |

- 列表支持 `/` 聚焦搜索、方向键移动、Enter 打开详情预览。
- 对象中心 Tab 顺序遵循页头动作 → 锚点 → 内容 → 版本与投递；历史版本切换后焦点落到版本标题。
- 正式确认关闭后焦点返回触发按钮；成功结果使用 `aria-live=polite`，结果未知使用持续可读的警告区。
- 状态必须同时有文字、图标和 tone；不能只用颜色区分待确认、失败和已生效。
- 图片预览必须使用审核后的 `altText`；无障碍说明缺失时提交发布同样被阻断。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 商品 / SKU 中心 | W14 | `skuId`、`skuRevisionId`、目标商城 | 返回聚焦原商品修订 |
| API 供应商连接 | W20 | `connectionId`、能力摘要 | 返回保留发布对象与选中版本 |
| 外部商品映射与供给 | W21 | `supplierOfferingRevisionId`、`skuId`、阻塞原因 | 处理后回 W22 重新校验，不自动提交 |
| 商城消费订单 | W25 | `productPublicationRevisionId`、来源订单 | 返回保留消费订单上下文 |
| 供应商订单 | W26 | 已支付订单引用的发布/供给修订 | 只读历史引用，不改发布版本 |
| 卡券经营 / 成本 | W28 | SKU、发布修订、期间 | 返回分析页保留筛选 |
| 接口错误与对账 | W29 | `deliveryId`、消息/差异任务 ID、原幂等键 | 处理完成后 W22 刷新投递状态 |

跨工作面导航只传稳定身份、选中版本和筛选上下文，不传价格、权限或“已确认”结论作为事实。

## 12. 验收清单

### 12.1 页面与效率

- [x] 运营能在列表一次筛选出待商城确认、失败、转人工和已暂停发布。
- [x] 1440×900 下列表露出 6–8 条有效数据行，SKU 身份列和操作列固定。
- [x] 对象中心一屏可同时识别稳定发布、当前商城生效版本和最新待确认版本。
- [x] 选中任一历史修订可看到当时完整发布内容、媒体、唯一固定供给和投递结果。
- [x] 用户文案不出现表名、组件名或“服务端分页”等实现词。

### 12.2 业务与数据

- [x] 每个发布修订恰好绑定一条固定供给修订。
- [ ] 类目、销售说明、最小购买量、至少一张主图和安全扫描由服务端校验。
- [x] 最小购买量不会从供应商最小订购量自动复制。
- [x] 供货价变化不会自动修改商城销售价。
- [x] 图片、固定供给、价格或销售状态变化均形成新修订，不覆盖历史。
- [ ] 已支付订单仍可钻取其下单时发布修订，后续修订不改变历史快照。
- [ ] `STOPPED`、零库存、不可供、数据过期或成本/关键供给变化未确认时，系统不等待人工即执行安全暂停；来源恢复也不会自动上架。
- [x] 发布工作副本策略未确认时只有 TaskTab 会话内编辑；无草稿保存 mutation、无自动保存/本地持久化，刷新或关闭前明确提示输入将丢失。
- [x] 多商城/唯一性规则未确认时，列表固定返回 `PUBLICATION_IDENTITY_POLICY_UNCONFIRMED` 并禁止新建；已有对象仍可按服务端 `publicationId` 查看/维护。

### 12.3 动作、投递与异常

- [x] 提交发布形成不可变修订和 outbox，正式结果固定显示版本号与投递编号。
- [ ] 系统安全暂停按“来源对象 + 原因 + 来源版本”冻结全部受影响发布；所有暂停子结果/投递/outbox 原子提交。`STOPPED` 再与唯一 `BUSINESS_EXCEPTION` 任务同事务，其它原因只返回 blocker/证据，不伪造任务。
- [x] `SystemSafetyPauseOperationView` 是列表/对象/操作结果的唯一结构：`SUPPLIER_STOPPED + COMMITTED/ALREADY_SAFE` 强制唯一 `followUpWorkItem`，其它已落库原因强制唯一 `followUpBlocker`，`UNKNOWN` 二者均禁止且保持 fail-closed。
- [ ] 已注册安全暂停任务只用于核对来源/影响和准备候选证据，不阻塞首次暂停，也不能选定替代供给或发起恢复；任务失败、丢租约、无人领取或尚无注册正常复核类型时，本地仍保持不可下单。
- [x] 销售价/销项税率变化且复核政策未配置时，`PUBLISH` 固定被 `REVIEW_POLICY_UNCONFIGURED` 阻断；无变化也只使用服务端 `publishGate` 结论。
- [x] 恢复责任未确认时，任何安全暂停到 `ON_SALE` 的提交都被 `RECOVERY_RESPONSIBILITY_UNCONFIRMED` 阻断；只允许安全暂停或人工暂停。
- [x] 商城成功确认前不显示为“商城已生效”。
- [ ] 同一发布版本重复发送不会创建重复商城商品或第二份 ERP 修订。
- [ ] 结果未知先查询，自动/人工重试继续使用原幂等键。
- [ ] 业务拒绝、鉴权失败和字段映射失败可直接进入 W29，原始报文与密钥不泄露。
- [ ] 批量重试使用服务端选择快照、逐项重验并展示成功/跳过/失败。

### 12.4 权限、状态与响应式

- [ ] 无模块权限、无数据范围、无发布、筛选无结果和字段级掩码可区分。
- [ ] 权限收回后不残留供货价、媒体签名 URL 或失败报文。
- [ ] §9 所有状态均有组件或浏览器验收。
- [ ] 1440、1280、1024、768、375 五档视口符合 §10 降级边界。
- [ ] 键盘能完成筛选、打开预览、浏览版本和读取正式结果。

## 13. 待确认事项

| ID | 问题 | 影响 | 建议决策人 | 当前建议 |
| --- | --- | --- | --- | --- |
| Q1 | 发布工作副本是否服务端持久化并支持自动保存？ | 编辑恢复、协作冲突和数据模型 | 产品 + 运营 + 后端 | 确认前只提供 TaskTab 会话内编辑；不定义服务端保存 mutation、不自动保存或本地持久化，刷新/关闭前明确提示会丢失 |
| Q2 | 商城销售价或销项税率变化是否需要财务复核，还是运营单岗发布？ | `allowedActions`、审批记录和页面流程 | 财务负责人 + 运营负责人 | 服务端政策未配置且价格/税率变化时 fail-closed，返回 `REVIEW_POLICY_UNCONFIGURED`；无变化也不由前端跳过服务端政策 |
| Q3 | 安全暂停后的恢复发布由谁发起和确认，供给处理如何与发布责任交接？ | 恢复责任、任务交接和处理 SLA；不改变系统立即安全暂停规则 | 采购负责人 + 运营负责人 | 确认前只允许安全暂停/人工暂停和候选证据准备；任何恢复 `ON_SALE` 均返回 `RECOVERY_RESPONSIBILITY_UNCONFIRMED`，不把“采购确认 → 运营发布”当作已固定链路 |
| Q4 | 同一 ERP SKU 是否会发布到多个商城，以及各商城的价格/区域是否独立？ | 稳定发布唯一键、列表入口和复制版本 | 业务负责人 | 确认前禁止新建并返回 `PUBLICATION_IDENTITY_POLICY_UNCONFIRMED`；已有对象只按服务端稳定 `publicationId` 查看/维护，前端不假定 `SKU + 商城` 唯一键 |
| Q5 | 商城确认 SLA 与自动重试上限是多少？ | 陈旧提示、失败升级和监控指标 | 运维 + 商城负责人 | 按错误类别配置，并由服务端返回下次动作时间 |

待确认事项确认后，应把结论写回相应章节并从本表移除，不长期保留建议与正式契约并存。

## 14. 业务依据

- `erp-phase-2.md` §7：外部商品同步、映射、价格/库存变化和 ERP 向商城发布。
- `erp-phase-2.md` §13.1、§13.2：发布幂等、可靠消息、结果未知与人工处理。
- `erp-phase-2.md` §15 P2-P03、§16、§17.1：商品发布页面、角色职责与核心验收。
- `erp-data-model.md` §6.15：`product_publication`、不可变修订、媒体、投递、唯一固定供给及必需约束。
- `erp-data-model.md` §7.7、§9.4：集成投递状态和商城/供应商断言。
- `erp-mall-data-mapping.md` §10.3：完整商品发布字段、类别/媒体来源和商城确认边界。
- `erp-ui-design.md` §3.4–§3.5、§4.3、§4.5、§6、§9–§11、§15：任务页签、响应式、M2/M4、二期挂载、状态和验收。
- `erp-ui-flows.md` §10：执行投递失败进入统一错误处理，同一业务对象内呈现协同结果。
