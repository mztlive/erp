# W25 · 商城消费订单

> 状态：草稿<br>
> 页面模式：M2 高密度查询列表 + M4 对象中心<br>
> 主要路由：`/commerce/consumption-orders`、`/commerce/consumption-orders/:mallOrderId`<br>
> 主要角色：运营、客服、财务；销售、采购按数据范围只读钻取<br>
> 最后更新：2026-08-01

## 1. 定位与目标

### 1.1 用户目标

用户进入 W25，应在一个工作面内看清：

1. 商城订单稳定身份、支付事实、后续取消/退款/完成/余额恢复事实及其发生时间；
2. 订单原价、优惠、运费、实付与每条商品明细快照；
3. 一张或多张卡券和微信支付如何实际分摊到每条商品明细；
4. 卡券支付来源如何沿稳定卡实例引用追溯到客户、原销售单和唯一卡券明细；
5. 支付发生在唯一时间点 `T` 之前还是之后，以及为何走原人工履约或 ERP 自动履约；
6. `T` 后订单拆成了哪些供应商子订单、当前履约/取消/退款状态和异常责任方；
7. 每笔消费当前成本口径是 `ACTUAL`、`STANDARD` 还是 `NONE`，而不把缺失成本显示为零；
8. 遇到下单失败或结果未知时，从当前订单直接进入 W26/W29 处理，同时保留“商城支付已发生”的事实。

### 1.2 业务目标

- W25 是由不可变关键事实形成的追溯视图，不是商城可变员工订单的实时副本，也不是第二个商城订单写入口。
- 商城只向 ERP 发送支付成功、取消成功、退款成功、订单完成和卡券余额恢复成功五类事实，不同步处理中间态。
- 实时和历史回填使用同一业务事实键；同一事实只形成一份正式记录。
- 支付来源只有 `CARD` 和 `WECHAT`；ERP 不保留福利账户或其它支付兼容分支。
- 商品 × 支付来源的实际分摊矩阵必须行列同时守恒，ERP 不按订单总额猜测分摊。
- 商城退款、卡券余额恢复和供应商退款是三个独立结果，分别影响消费、卡余额、成本与应付。
- `T` 后支付在商品发布和固定供给完整时自动形成供应商订单；W25 与 W26 同屏可触达，但不混为一个对象。
- 第一份有效 `PAYMENT_SUCCEEDED` 必须无条件在一个领域事务内保存不可变支付事实、唯一 `mall_order` 及可保存的订单/商品/支付来源/分摊来源快照；客户、卡实例、商品映射或供给暂缺只让归集待处理并产生差异，绝不能造成“有支付事实、无商城订单”。归集条件齐全时形成消费归属；`T` 后固定供给完整时再以同一正式处理事务形成确定性供应商子订单、首个 `PLACE` 动作和 outbox。消息投递、重试和分析投影才异步执行。
- 卡实例基线缺失或来源暂未归属属于待归集差异；已存在卡实例的原销售单或初始余额与新来源冲突时，必须保留原基线并进入 `FINANCE_CORRECTION_REVIEW`，经复核只追加纠错，不能由运营直接覆盖。

### 1.3 不在本工作面完成

- 不创建、编辑、取消或删除商城员工订单；商城继续主责可变订单与员工端进度。
- 不在 ERP 伪造支付成功、退款成功、订单完成或余额恢复事实。
- 不展示待支付、支付中、商城退款处理中等商城中间状态。
- 不直接重试供应商下单、取消或退款；进入 W26 或 W29 按原幂等键处理。
- 不通过修改订单号、时间或接收时间改变其 `LEGACY_MANUAL` / `ERP_AUTOMATED` 履约归属。
- 不在支付分摊矩阵内编辑成本；成本事实与评估另行展示，经营分析进入 W28。
- 不建设员工主档、画像，不保存卡号、卡密、卡实例绑定手机号或其可逆映射。

## 2. 用户、权限与数据范围

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 运营 | 最近消费 / 异常视图 | 被授权商城、客户和商品范围 | 查看事实、分摊、投影/商品关联和异常去向 |
| 客服 | 售后事实 / 履约异常视图 | 被授权商城订单；按工单和客户范围 | 查看退款/余额恢复、进入供应商订单协同 |
| 财务 | 成本与未归集视图 | 被授权组织和客户 | 查看支付分摊、消费冲减、成本口径和结算去向 |
| 销售 | 从 W05 或客户进入 | 本人负责/协作客户的消费明细或授权汇总 | 只读追溯原销售单和消费事实 |
| 采购 | 从 W26 进入 | 其负责供应商相关订单 | 查看商品明细与支付已发生摘要；敏感地址另授权 |
| 系统管理员 / 运维 | 差异与接口异常 | 获授权商城的技术摘要 | 查看接收/归集状态、进入 W29；不改消费事实 |
| 管理层 | 从 W28 下钻 | 授权范围内只读 | 查看事实明细；经营汇总留在 W28 |

### 2.1 字段权限和敏感信息

| 数据 | 默认表现 | 揭示条件 |
| --- | --- | --- |
| 商城用户稳定引用 | 短引用 | 获授权排障角色可复制完整稳定 ID |
| 卡实例引用 | 短引用，明确“非卡号” | 有消费追溯权限可复制；任何角色都不能由其反推卡号/卡密 |
| 客户和原销售单 | 按客户数据范围展示 | 无明细权限时只显示授权汇总或掩码 |
| 收货人、手机号、地址 | 默认掩码 | 仅履约所需角色、短时揭示、记录审计；日志和导出不含未授权完整值 |
| 微信支付引用 | 短引用 | 财务/排障角色按权限查看 |
| 商品成本和税率 | 按财务/采购字段权限展示 | 无权限时保留成本口径标签，金额掩码 |
| 原始事实报文 | 不在普通业务页展示 | 受控排障进入 W29，仍只展示脱敏摘要 |

页面打开期间权限收回时，必须立即清除已揭示地址、支付引用、成本和客户字段，不能只禁用按钮。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 查看消费订单列表 | 侧栏“商城消费订单” | `/commerce/consumption-orders`；筛选写入 URL | 返回恢复筛选、分页和滚动 |
| 查看订单详情 | 列表行、W28 下钻、全局搜索 | `/commerce/consumption-orders/:mallOrderId?section=overview` | 返回来源页签并聚焦原行 |
| 查看指定事实 | 事实时间线 | `?section=facts&fact={factId}` | 后退恢复上一事实 |
| 查看原销售单 | 来源追溯区 | 聚焦 `sales-order:{salesOrderId}` 并定位协同/消费入口 | 返回 W25 保留订单与来源位置 |
| 查看供应商子订单 | 履约区 | 打开 W26 稳定子订单页签 | 返回 W25 聚焦原子订单卡片 |
| 处理归集/接口差异 | 警告区 | 打开 W29 并携带事实/订单/差异 ID | 返回后重新查询，不改当前事实 |
| 刷新浏览器 | 任意详情 | 恢复商城订单、锚点和选中事实；不恢复地址揭示 | 当前订单 |

对象页签身份为 `mall-consumption-order:{mallOrderId}`，标题为 `消费 · {商城订单号}`。同一订单重复打开只聚焦原页签。选中事实、商品、支付来源或供应商子订单摘要不改变页签身份。

## 4. 页面布局

### 4.1 列表页（1440×900）

```text
┌ PageHeader：商城消费订单                         数据水位 09:36 [刷新] [导出]
├ MetricStrip：[支付成功] [待归集] [事实差异] [自动履约异常] [成本未覆盖]
├ ListToolbar：SavedView | 搜索 | 商城 | 事实时间 | 履约链 | 归集状态 | 更多筛选
├ 筛选摘要 / BackgroundJobProgress（导出存在时）
├ BusinessTableFrame
│ 商城订单（固定） | 客户 | 支付时间 | 实付 | 支付构成 | 关键事实
│ 履约链 | 供应商订单摘要 | 归集 | 成本口径 | 操作（固定）
└ 服务端分页
```

1440×900 基准视口中至少露出 6–8 条有效数据行。商城订单身份和行级主动作固定；金额右对齐并标注人民币含税/实付口径。

### 4.2 对象中心（1440×900）

遵循 `erp-ui-design.md` §4.5.1：`object-chrome` 导航壳 + compact 对象头。

```text
┌ PageHeader object-chrome：商城消费订单 › 外部单号        [返回] [刷新] ─┐
├ DocumentHeader compact：商城 · 客户 [履约链状态]                         │
│  外部单号 · 事实/归集轨 · 支付时间                                       │
│ 履约链：原人工 / ERP自动    归集：已归集 / 待归集 / 差异      [刷新] [更多]
├ 风险提示（适用时）：商城支付已发生；当前处理的是履约或归集异常
├ 锚点：概览 | 关键事实 | 商品明细 | 支付与分摊 | 来源追溯
│       | 供应商履约 | 成本口径 | 售后结果 | 审计
├ 概览：原价、优惠、运费、实付、下单/支付时间、履约链和 T 判定证据
├ 关键事实：支付、取消、退款、完成、余额恢复不可变时间线
├ 商品明细：商品发布修订、数量、售价、优惠/运费/实付、成本快照
├ 支付与分摊：CARD/WECHAT 来源 + 商品 × 支付来源守恒矩阵
├ 来源追溯：卡实例引用（非卡号）→ 客户 → 原销售单 → 唯一卡券明细
├ 供应商履约：一个或多个 W26 子订单状态；原人工链显示明确原因
├ 成本口径：逐消费来源 ACTUAL/STANDARD/NONE 及依据，不与支付矩阵混写
└ 售后结果：取消、退款、余额恢复与供应商退款分轨说明
```

### 4.3 支付分摊矩阵

```text
                       卡实例 …A17     卡实例 …C52     微信 …P90    明细实付
商品明细 1                  80.00           20.00           0.00      100.00
商品明细 2                   0.00           50.00          30.00       80.00
来源合计                    80.00           70.00          30.00      180.00
```

- 行合计、列合计和订单实付均由服务端给出及校验；前端只格式化和高亮差异。
- 卡实例列标题显示短引用并标注“非卡号”；微信列显示短支付引用。
- 成本分配不得塞入本矩阵；成本区按正式 `cost_entry` / `cost_allocation` 与评估链展示。
- 退款以独立反向事实/分配展示，不覆盖原支付矩阵。

### 4.4 区域说明

| 区域 | 目的 | 主组件 | 是否固定 |
| --- | --- | --- | --- |
| 页头与双轨状态 | 区分商城事实、归集和履约链 | `DocumentHeader` `StatusTrackSummary` | 顶部固定 |
| 支付已发生提示 | 防止履约失败被误解为支付不存在 | `Alert` | 存在履约异常时固定可见 |
| 事实时间线 | 按发生时间读取五类不可变结果 | `AuditTimeline` / 事实时间线 | 否 |
| 支付矩阵 | 证明商品与来源双向守恒 | `BusinessTableFrame` | 否 |
| 来源追溯 | 钻取原销售单和稳定卡实例基线 | `RelatedDocumentList` | 否 |
| 供应商履约 | 读取 W26 子订单摘要和异常入口 | `ResponsibilityPanel` | 否 |
| 成本口径 | 说明成本可信级别和依据 | `CostCoverageNotice` / 明细表 | 否 |

## 5. 展示内容与字段

### 5.1 订单身份与金额

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 身份 | `mallOrderId` / `externalOrderNo` | 商城订单 | `mall_order` | ERP 稳定 ID + 商城单号 | 有订单范围可见 |
| 来源 | `mallName` / `mallUserRef` | 来源商城 / 商城用户引用 | 商城支付事实 | 用户仅稳定引用，不建员工主档 | 引用默认缩短 |
| 客户 | `customerId` / `customerName` | 所属客户 | 支付事实映射 | 待归集时显示来源客户引用与差异 | 按客户范围 |
| 时间 | `orderedAt` / `paidAt` | 下单 / 支付成功 | 支付成功快照 | `paidAt` 决定履约链，非接收时间 | 可见 |
| 金额 | `grossAmount` | 商品原价 | 支付成功快照 | 人民币分精度 | 按金额权限 |
| 金额 | `discountAmount` / `freightAmount` | 优惠 / 运费 | 支付成功快照 | 与明细汇总守恒 | 按金额权限 |
| 金额 | `paidAmount` | 实付金额 | 支付成功快照 | `原价 - 优惠 + 运费` | 按金额权限 |
| 履约 | `fulfillmentChain` | 履约方式 | `mall_order.fulfillment_chain` | `T` 前原人工、`T` 及以后 ERP 自动 | 全部有权用户 |
| 归集 | `attributionStatus` | 归集状态 | `mall_order` / 消费条目 | 待归集、已归集、差异 | 可见 |

### 5.2 五类关键事实

| 事实 | 用户文案 | 必须展示 | 数据来源 | 口径 / 新鲜度 | 权限规则 | 金额 / 状态作用 |
| --- | --- | --- | --- | --- | --- | --- |
| `PAYMENT_SUCCEEDED` | 支付成功 | 业务事实键短摘要、订单版本、发生/接收时间、实时/回填 | 商城不可变支付成功事实 | 展示 `occurredAt/receivedAt`；金额含税并带支付分摊 | 财务可见金额；业务角色按订单范围 | 无条件建立唯一商城订单与来源快照；归集条件齐全后形成消费归属；同一事实键仅一份正式事实 |
| `ORDER_CANCELED` | 订单已取消 | 售后请求 ID、取消版本、范围、数量/金额、原因 | 商城取消结果事实 | 与支付事实分轨，显示接收水位 | 客服/运营按订单范围；原因字段分权 | 只记录取消结果，不冲减消费或资金 |
| `REFUND_SUCCEEDED` | 商城退款成功 | 售后请求 ID、退款号/版本、原商品与来源、实际金额 | 商城退款成功事实 | 多次部分退款逐事实展示，金额含税 | 财务/客服按授权字段 | 按原商品和支付来源追加消费反向记录 |
| `ORDER_COMPLETED` | 商城订单已完成 | 完成版本、实际完成时间 | 商城完成结果事实 | 业务发生时间与接收时间分列 | 对象可见即显示状态 | 不覆盖供应商履约事实 |
| `CARD_BALANCE_RESTORED` | 卡券余额已恢复 | 售后请求 ID、关联退款、恢复号/版本、卡实例引用、金额 | 商城余额恢复事实 | 卡实例只显示稳定引用，金额含税 | 禁止卡号/卡密；金额按财务字段权 | 只记录卡余额回补，不再次冲减消费、成本或应付 |

同一订单的支付、取消、完成、多次部分退款和多次余额恢复分别显示，不因商城订单号相同而合并。事实时间线以 `occurredAt` 为业务时间，同时显示 `receivedAt` 以帮助排查延迟。

### 5.3 商品明细与发布引用

| 字段 | 用户文案 | 数据来源 | 展示规则 |
| --- | --- | --- | --- |
| `externalItemId` | 商城明细 | `mall_order_item` | 稳定来源明细身份 |
| `skuId` / `productPublicationRevisionId` | ERP SKU / 发布版本 | 支付成功快照 | 待映射时保留来源身份并标记待归集 |
| `nameSnapshot` / `specSnapshot` | 商品 / 规格 | 下单快照 | 不用当前 W14/W22 内容覆盖 |
| `quantity` / `unitPriceGross` | 数量 / 含税单价 | 下单快照 | 金额与数量格式化 |
| `lineGrossAmount` | 明细原价 | 下单快照 | `数量 × 含税单价`，服务端校验 |
| `allocatedDiscountAmount` / `allocatedFreightAmount` | 分摊优惠 / 运费 | 商城实际分配 | ERP 不按比例推测 |
| `paidAmount` | 明细实付 | 下单快照 | 与支付来源行合计守恒 |
| `unitCostSnapshot` / `costSnapshotTotal` | 商城成本快照 | 支付事实 | 不等于最终成本；按权限显示 |
| `costTaxInclusion` / `costInputTaxRate` | 成本含税标识 / 进项税率 | 支付事实 | 缺失时不能标 `ACTUAL`，不使用销项税率替代 |

### 5.4 支付来源、来源追溯与成本

| 字段 | 用户文案 | 数据来源 | 展示规则 |
| --- | --- | --- | --- |
| `sourceType` | 支付方式 | `mall_payment_source` | 只允许卡券、微信 |
| `amount` | 来源金额 | 支付来源 | 所有来源合计等于订单实付 |
| `sourceCardInstanceRef` | 卡实例引用（非卡号） | 卡券支付来源 | 短引用；基线暂缺时标记待归集 |
| `wechatPaymentRef` | 微信支付引用 | 微信来源 | 不挂卡实例或企业卡券收入归属 |
| `originSalesOrderId` | 原销售单 | 卡实例基线 → 稳定来源身份 | 消费归原销售单和唯稳定一卡券明细，不按销售版本拆分 |
| `allocatedPaymentAmount` | 分摊金额 | `mall_item_funding_allocation` | 商品和来源双向守恒 |
| `costBasis` | 成本口径 | 当前成本评估链尾 | `ACTUAL`、`STANDARD`、`NONE` |
| `basisSource` | 成本依据 | 成本评估 | 商城快照、消费时点供给、供应商履约/结算或人工复核 |
| `gross/net/taxAmount` | 成本金额 | 成本事实与分配 | `NONE` 时为空，不展示 0 成本 |

卡券来源差异必须分为两类展示和路由：

- 卡券稳定来源引用已保存但对应卡实例基线/来源对象尚未导入，或客户暂未归属时，保留支付事实并生成待归集差异，由运营协调补齐来源资料；补齐后仍按原 `businessFactKey` 归集。契约要求的稳定来源引用本身缺失属于事实完整性校验失败，不能由页面猜测补值。
- 同一稳定卡实例已经存在基线，但新来源声称的原销售单或初始余额不同，页面显示“基线冲突，禁止覆盖”；领域服务保留原基线，创建 `FINANCE_CORRECTION_REVIEW`，财务按 `subjectHash` 和证据复核后追加 `mall_card_instance_correction`。

### 5.5 供应商履约协同

| 情形 | W25 表现 | 数据来源 / 新鲜度 | 权限规则 | 下一步 |
| --- | --- | --- | --- | --- |
| `paidAt < T` | “原人工履约链”；明确历史回填只记账 | 支付事实、唯一 `T` 与回填来源；显示各自水位 | 对象可见即显示链路类型 | 不创建 W26 子订单；售后仍按结果事实回流 |
| `paidAt >= T` 且发布/供给完整 | 展示按供应商确定性拆分的一个或多个子订单 | 支付时固定的发布/供给快照与 W26 当前摘要 | 供应商成本按字段权 | 钻取 W26 查看接单、履约、取消、退款 |
| `paidAt >= T` 但商品、发布或供给缺失 | 支付事实保留，显示“自动履约条件不足” | 原支付事实与 W21/W29 blocker 水位 | 技术摘要脱敏 | 进入 W29 / W21 修复，使用原事实归集 |
| 供应商明确拒绝 | 支付仍显示成功，履约区显示拒单和售后责任 | W26 不可变拒绝结果 | 客服/采购按订单范围 | 进入 W26 协同退款或额度恢复 |
| 下单结果未知 | 不显示接单成功，也不创建第二张子订单 | W26 原操作与最后查询时间 | 原请求摘要按技术字段权 | 进入 W26 查询原请求；无查询能力则 W29 人工处理 |
| 供应商部分退款 | 履约、取消、退款三轨展示，并分列“未付应付冲减 / 已付现金退回” | W26 `supplier_refund_fact/allocation` 与 W12 成本、应付、付款事实 | 财务金额按字段权 | 未付部分追加负向应付与抵销；已付部分同事务追加原 `payment_allocation` 的 `REVERSE`、通用 `supplier_refund` 现金事实、负向应付及抵销；两者均追加成本冲减，且绝不替代商城退款 |

## 6. 搜索、筛选、排序与默认视图

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 搜索 | 空 | `q` | 商城订单号精确优先；客户、ERP 销售单、供应商子订单按权限搜索 |
| 来源商城 | 全部有权商城 | `mall` | 服务端过滤 |
| 事实发生期间 | 服务端角色策略；未配置时不预填 | `occurredFrom` / `occurredTo` | 使用事实发生时间，不用 ERP 接收时间；默认策略缺失时必须由用户显式选择完整起止时间后才查询，不静默采用 30 天 |
| 事实类型 | 全部 | `factType` | 支付、取消、退款、完成、余额恢复 |
| 履约链 | 全部 | `fulfillmentChain` | 原人工 / ERP 自动 |
| 归集状态 | 全部 | `attributionStatus` | 待归集、已归集、差异 |
| 支付方式 | 全部 | `paymentSource` | 卡券、微信、组合支付 |
| 供应商状态 | 全部 | `supplierStatus` | W26 综合只读摘要；不改订单事实 |
| 成本口径 | 全部 | `costBasis` 可多选 | ACTUAL、STANDARD、NONE；同单多口径按分项摘要展示，Q4 未确认前不归一成单一“混合”主标签 |
| 数据来源 | 全部 | `dataSource` | 实时 / 历史回填 |
| 排序 | 事实发生时间降序 | `sort=occurredAt.desc` | 服务端排序；相同时间使用稳定事实 ID |

- 指标点击改变列表筛选并写 URL，必须有选中态和筛选摘要。
- 单击行打开 `detail` 半屏，至少覆盖身份、金额、关键事实、支付构成、履约链、供应商摘要和成本口径。
- “当前筛选全部”导出使用服务端选择快照，不把当前页当全量。
- 全局搜索使用稳定单号；完整卡实例引用、手机号或地址不进入普通模糊搜索索引。

## 7. 操作契约

W25 没有修改商城订单或消费事实的业务写操作。页面动作以读取、受控揭示、合规导出和导航处理为主。

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 刷新 | 页头 | 有 W25 权限 | 无 | 重新获取订单、事实、归集和履约水位 | 保留旧数据并显示重试 |
| 查看原销售单 | 来源追溯 | 有 W05 对象权限 | 无 | 聚焦 W05 原销售单 | 无权限时保留受控来源摘要 |
| 查看商品发布版本 | 商品明细 | 有 W22 权限 | 无 | 打开下单时发布修订 | 返回 W25 聚焦原明细 |
| 查看供应商订单 | 履约卡片 | 有 W26 对象权限且存在子订单 | 无 | 打开 W26 并定位当前子订单 | 无权限时只显示授权状态摘要 |
| 处理归集 / 接口差异 | 警告区 | 有 W29 权限且存在差异任务 | 无 | 打开既有 W29 任务 | 重复打开聚焦既有任务，不新建事实 |
| 揭示履约地址 | 地址区 | `REVEAL_ADDRESS`；当前职责确有履约需要 | 明确用途与短时期限 | 短时展示并写访问审计 | 超时自动重新掩码；结果未知不展示完整值 |
| 导出当前选择 | 列表工具栏 | `EXPORT`；选择快照和字段权限有效 | `BatchImpactPreview` 展示范围与遮罩 | 创建后台导出任务，固定显示任务号和到期时间 | 按请求 ID 查询结果，不重复创建导出 |
| 查看事实技术摘要 | 审计区 | 排障字段权限 | 无 | 打开脱敏信封摘要或 W29 | 原始报文不进入 W25 |

### 7.1 明确禁止

- 不提供“修改支付状态”“补一条成功事实”“删除重复订单”“编辑分摊”“改履约链”或“强制归集到某销售单”。
- 归集条件修复后，领域服务使用原业务事实键重新归集；W25 只刷新结果，不生成第二份消费事实。
- 供应商下单、查询、取消和退款使用 W26 的原幂等动作；W25 不提供旁路按钮。
- 商城退款事实、余额恢复事实和供应商退款事实必须分轨展示，不能用一个“退款完成”按钮同时推进三类正式事实。

## 8. 数据契约

### 8.1 列表查询

```ts
type MallConsumptionOrderListQuery = {
  q?: string
  mallIds?: string[]
  occurredFrom: string
  occurredTo: string
  factTypes?: string[]
  fulfillmentChains?: Array<"LEGACY_MANUAL" | "ERP_AUTOMATED">
  attributionStatuses?: string[]
  paymentSources?: Array<"CARD" | "WECHAT" | "MIXED">
  supplierStatuses?: string[]
  costBases?: Array<"ACTUAL" | "STANDARD" | "NONE">
  dataSources?: Array<"REALTIME" | "BACKFILL">
  sort: string
  page: number
  pageSize: number
}

type MallConsumptionOrderRowBase = {
  mallOrderId: string
  mallId: string
  mallName: string
  externalOrderNo: string
  customerId?: string
  customerLabel: string
  paidAt: string
  paidAmount: string
  paymentComposition: { cardAmount: string; wechatAmount: string; sourceCount: number }
  factSummary: Array<{ factType: string; latestOccurredAt: string; count: number }>
  fulfillmentChain: "LEGACY_MANUAL" | "ERP_AUTOMATED"
  supplierOrderSummary: { total: number; statuses: string[]; hasException: boolean }
  attributionStatus: string
  costBasisBreakdown: Array<{
    basis: "ACTUAL" | "STANDARD" | "NONE"
    lineCount: number
    costAmount?: string
  }>
  dataSource: "REALTIME" | "BACKFILL" | "MIXED"
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
}

type MallConsumptionOrderCostBasisPolicy =
  | {
      costBasisPolicyState: "CONFIGURED"
      normalizedCostBasis?: "ACTUAL" | "STANDARD" | "NONE" | "MIXED"
    }
  | {
      costBasisPolicyState: "UNCONFIGURED"
      normalizedCostBasis?: never
    }

type MallConsumptionOrderRow =
  MallConsumptionOrderRowBase & MallConsumptionOrderCostBasisPolicy
```

指标、总数与列表使用同一权限快照和事实水位。Query Key 至少包含用户、角色、权限/数据范围版本、筛选和时区。

### 8.2 对象中心查询

```ts
type MallOrderFactView = {
  factId: string
  factType:
    | "PAYMENT_SUCCEEDED"
    | "ORDER_CANCELED"
    | "REFUND_SUCCEEDED"
    | "ORDER_COMPLETED"
    | "CARD_BALANCE_RESTORED"
  businessFactKeySummary: string
  externalOrderVersion: string
  afterSalesRequestId?: string
  originalPaymentFactId?: string
  occurredAt: string
  receivedAt: string
  dataSource: "REALTIME" | "BACKFILL"
  processingStatus: string
  resultDetails: Record<string, string | number | null>
}

type MallOrderItemView = {
  mallOrderItemId: string
  externalItemId: string
  skuId?: string
  productPublicationRevisionId?: string
  supplierOfferingRevisionId?: string
  nameSnapshot: string
  specSnapshot: string
  quantity: string
  unitPriceGross: string
  lineGrossAmount: string
  allocatedDiscountAmount: string
  allocatedFreightAmount: string
  paidAmount: string
  salesTaxRate: string
  unitCostSnapshot?: string
  costSnapshotTotal?: string
  costTaxInclusion?: string
  costInputTaxRate?: string
  attributionStatus: string
}

type CostAssessmentView = {
  assessmentId: string
  assessmentNo: number
  costBasis: "ACTUAL" | "STANDARD" | "NONE"
  basisSourceLabel: string
  grossAmount?: string
  netAmount?: string
  taxAmount?: string
  taxInclusion?: string
  inputTaxRate?: string
  assessedAt: string
}

type MallConsumptionOrderView = {
  identity: {
    mallOrderId: string
    mallId: string
    mallName: string
    externalOrderNo: string
    paymentFactId: string
  }
  customer: {
    sourceCustomerRef: string
    customerId?: string
    customerLabel: string
    attributionStatus: string
  }
  orderedAt: string
  paidAt: string
  amounts: {
    gross: string
    discount: string
    freight: string
    paid: string
    conservationStatus: "VALID" | "DIFFERENCE"
  }
  fulfillment: {
    chain: "LEGACY_MANUAL" | "ERP_AUTOMATED"
    cutoverId: string
    cutoverAt: string
    decidedByOccurredAt: string
  }
  facts: MallOrderFactView[]
  items: MallOrderItemView[]
  paymentSources: Array<{
    paymentSourceId: string
    sourceNo: number
    sourceType: "CARD" | "WECHAT"
    amount: string
    sourceReference: string
    mallCardInstanceId?: string
    attributionStatus: string
    attributionIssue?: {
      type: "SOURCE_OBJECT_MISSING" | "UNATTRIBUTED" | "BASELINE_CONFLICT"
      ownerRole: "OPERATIONS" | "FINANCE"
      workItemId?: string
      correctionId?: string
    }
    origin?: {
      customerId: string
      salesOrderId: string
      salesOrderNo: string
      salesOrderLineId: string
    }
  }>
  fundingAllocations: Array<{
    mallOrderItemId: string
    paymentSourceId: string
    allocatedPaymentAmount: string
  }>
  conservation: {
    itemRowResults: Array<{ mallOrderItemId: string; expected: string; actual: string; valid: boolean }>
    sourceColumnResults: Array<{ paymentSourceId: string; expected: string; actual: string; valid: boolean }>
  }
  consumptionEntries: Array<{
    consumptionEntryId: string
    factId: string
    itemId: string
    paymentSourceId: string
    direction: "CONSUMPTION" | "REVERSAL"
    amount: string
    occurredAt: string
    attributionStatus: string
    originSalesOrderId?: string
    reversesConsumptionEntryId?: string
    currentCostAssessment: CostAssessmentView
  }>
  supplierOrders: Array<{
    supplierFulfillmentOrderId: string
    fulfillmentOrderNo: string
    supplierLabel: string
    itemIds: string[]
    fulfillmentStatus:
      | "RECEIVED"
      | "SUBMITTING"
      | "ACCEPTED"
      | "REJECTED"
      | "RESULT_UNKNOWN"
      | "FULFILLING"
      | "SHIPPED"
      | "COMPLETED"
      | "EXCEPTION"
    cancelStatus: string
    refundStatus: string
    supplierRefundSummary?: {
      refundFactCount: number
      costReductionGross: string
      payableReductionGross: string
      cashRefundGross: string
      reversedPaymentAllocationCount: number
    }
  }>
  address: { maskedSummary: string; revealAllowed: boolean }
  freshness: {
    factWatermark: string
    attributionUpdatedAt: string
    supplierUpdatedAt?: string
    costAssessedAt?: string
    queriedAt: string
  }
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
  fieldPermissions: Record<string, "full" | "masked" | "hidden">
}
```

`MallOrderFactView` 必须包含事实 ID、类型、业务事实键短摘要、商城版本、售后请求 ID（适用时）、原支付事实、`occurredAt`、`receivedAt`、实时/回填来源、处理状态和专有结果字段。`MallOrderItemView` 必须覆盖 §5.3 的下单快照和字段权限。

### 8.3 提交与后台任务

W25 不提交消费业务事实。唯一页面写请求为受控审计或后台导出：

```ts
type ConsumptionOrderUtilityCommand =
  | {
      action: "REVEAL_ADDRESS"
      mallOrderId: string
      purposeCode: string
      requestId: string
    }
  | {
      action: "EXPORT"
      selectionSnapshotId: string
      fieldSetId: string
      requestId: string
    }
```

- 地址揭示返回短时凭据，离开页面、权限变化或到期后立即清除。
- 导出通过 `background_job` 执行；发起和下载时分别鉴权，结果文件按统一规则到期。
- 请求结果未知时使用 `requestId` 查询既有审计/任务，不重复揭示或创建第二个导出。
- 消费事实接收不因用户刷新触发。第一份有效支付通过验签、金额与分摊守恒校验后，按 `(mallId, externalOrderNo)` 串行处理：
  - 无论归集条件是否齐全，领域事务都以 inbox 和 `businessFactKey` 幂等保存不可变支付事实，并无条件创建唯一 `mall_order` 及契约中可保存的订单、商品、支付来源和守恒分摊来源快照；客户或 ERP 商品暂未定位时使用来源引用和 `PENDING` 归集状态。任一必需写入失败整笔回滚，重复消息返回既有订单与事实。
  - 客户、卡实例和商品归集条件完整时，锁定上述原订单与事实形成消费归属；`paidAt >= T` 且发布、固定供给和能力完整时，同一正式处理事务创建确定性供应商子订单及明细、首个 `PLACE` 动作和 outbox。若首次接收时条件已齐，这些写入属于首次事实/订单事务；若条件后补齐，则属于锁定原事实和订单的续归集事务。两种情况下“子订单 + 动作 + outbox”都必须原子，不能留下“有子订单无动作”或“有动作无 outbox”。
  - 前置条件缺失时保留已经形成的不可变事实和唯一商城订单，并同事务登记待归集状态、差异/正式任务；不能因归集失败删掉或推迟创建订单。修复后锁定原事实与订单，继续使用同一 `businessFactKey` 和稳定唯一键只补齐缺失归属或履约对象；唯一冲突加载既有对象，不生成第二份订单、消费、动作或 outbox。
- outbox 的对外投递、结果查询与重试，以及成本/经营分析投影消费可以异步；它们不得反向改变上述正式事实或唯一履约链。
- 供应商退款事实由领域事务按未付与已付净额拆分：未付部分追加负向 `payable_entry` 与 `payable_entry_offset`；已付部分同事务追加原 `payment_allocation` 的 `REVERSE`、通用 `supplier_refund` 现金回款事实、负向应付及抵销。两类同时追加对应成本冲减，任何时点不产生负开放应付，也不代替商城退款或卡余额恢复。

### 8.4 前端边界

- 前端不计算订单金额、行列守恒、履约链、原销售单归属、成本口径或退款净额。
- 分摊矩阵可以做视觉合计，但正式合计、有效性和尾差归属完全采用服务端结果。
- 前端只格式化金额、数量、时间、短引用和状态文案；不得把 `NONE` 格式化为 0.00。
- 后续商品、供给、客户或销售版本变化不能覆盖支付发生时的不可变快照。
- 异步消费/成本分析投影必须显示独立 outbox 水位、`projectionUpdatedAt` 和 `lagSeconds`；正常消费 outbox 后 60 秒内可见，超过 60 秒标记陈旧并告警。每日全量重建/对账校验投影，重建失败只影响查询新鲜度，不修改正式事实；商城余额快照使用独立 `balanceSnapshotAt`，不得用分析更新时间冒充余额时点。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 列表或对象中心按成稿结构显示 Skeleton | 应用壳导航可用 | 查询完成原位替换 |
| 刷新 | 保留事实、矩阵和履约摘要，显示各自水位 | 允许阅读；敏感揭示不跨刷新保留 | 成功更新时间，失败保留旧数据 |
| 空数据 | “当前范围没有消费订单” | 调整期间、商城或返回来源 | 新事实到达后显示 |
| 筛选无结果 | 显示筛选摘要 | 清除筛选 | 恢复服务端已配置期间；未配置时重新显式选择 |
| 无数据范围 | 不展示全局 0 指标 | 查看角色 / 申请范围 | 权限更新后重查 |
| 查询失败 | 有缓存保留并标陈旧；无缓存显示失败态 | 重试 | 查询恢复 |
| 数据陈旧 | 分别标记事实、归集、供应商与成本水位 | 刷新、进入 W29 | 投影/查询追平 |
| 字段级隐藏 | 客户、地址、支付引用或成本按字段掩码 | 其它有权导航可用 | 权限更新后重查 |
| 支付事实已保存、待归集 | 保留完整原始事实摘要和唯一商城订单，缺失归属明确标记；不显示成“缺订单” | 打开 W29/W21 查看差异 | 条件补齐后按原事实和订单归集 |
| 支付版本差异 | 第一份有效支付保持正式；后到版本标记差异 | 打开 W29 | 追加纠错/售后闭环，不覆盖原支付 |
| 卡实例来源对象缺失 / 未归属 | 原支付事实和稳定来源引用保留，来源列显示待归集和运营责任 | 打开既有差异任务 | 补齐基线/归属资料后按原事实键归集 |
| 卡实例基线冲突 | 显示原基线、新来源摘要和“禁止覆盖” | 打开 `FINANCE_CORRECTION_REVIEW`；运营只能补充证据 | 财务复核后追加纠错，原基线永久保留 |
| 分摊不守恒 | 矩阵高亮服务端差异，不生成消费和供应商动作 | 查看差异任务 | 来源修复后重新处理原事实 |
| `T` 前订单 | 明确“原人工履约链”，供应商区不显示缺单错误 | 查看历史事实 | 不转自动履约 |
| `T` 后自动履约条件不足 | 支付已发生提示 + 阻塞清单 | 进入 W21/W29 | 条件补齐后原事实继续归集 |
| 供应商结果未知 | 支付和消费事实保持；供应商区警告未知 | 进入 W26 查询原请求 | 得到明确结果或转人工 |
| 部分退款 / 多次恢复 | 时间线逐笔显示，不覆盖原记录 | 查看分配与来源 | 后续事实追加 |
| 供应商退款含已付金额 | 分列成本冲减、未付应付冲减、付款分配反向和现金退款事实 | 钻取 W26/W12 查看正式记录 | 同一事务恢复；不能用商城退款补缺口 |
| 成本 `NONE` | 金额为空并显示无可用成本，不暗示零成本 | 查看差异 / W28 口径 | 新评估追加后更新链尾 |
| 地址揭示中 / 到期 | 显示短时状态与剩余有效期；到期自动掩码 | 重新申请揭示 | 服务端重新鉴权 |
| 导出后台任务 | 显示进度、遮罩字段和结果到期时间 | 查看/下载有权结果 | 原任务查询，不重复创建 |
| 权限收回 | 立即掩码并清除已揭示值 | 返回有权模块 | 恢复后重查 |

## 10. 响应式、键盘与无障碍

| 视口 | 布局变化 | 保留内容 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 列表 6–8 行；对象中心金额、事实和履约摘要同屏 | 订单、客户、支付、履约链、归集、成本口径、主动作 | 无 |
| 1280×800 | 次要列进入列设置；支付矩阵可横滚 | 身份、实付、关键事实、履约与异常 | 数据来源、最近接收时间可隐藏 |
| 1024×768 | 图标侧栏；详情覆盖；对象中心单列分区 | 支付已发生提示、事实、分摊、来源、供应商摘要 | 完整审计和成本依据折叠 |
| 768×1024 | 导航抽屉；身份/操作固定；矩阵横滚并固定行列标题 | 订单、金额、事实、CARD/WECHAT、履约链 | 筛选进面板；地址只显示掩码摘要 |
| 375×812 | 紧凑只读卡片；分摊改按商品展开的来源列表 | 订单、支付、关键事实、履约状态、异常入口 | 不展示完整矩阵、列设置、复杂导出或地址揭示 |

- `/` 聚焦列表搜索，方向键移动，Enter 打开详情预览。
- 事实时间线使用有序语义并同时播报事实类型、发生时间和结果；不只读图标。
- 支付矩阵提供标题、行头、列头和可读合计；差异单元格用文字说明，不只用红色。
- 从 W05/W26/W29 返回后焦点恢复原触发元素；事实筛选变化通过 `aria-live=polite` 播报结果数。
- 地址揭示 Dialog 关闭后焦点返回掩码字段；到期重新掩码时不把完整值留在 DOM 或无障碍树。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 客户中心 | W03 | `customerId`、消费期间 | 返回保留客户上下文 |
| 销售单中心 | W05 | `originSalesOrderId`、唯稳定一卡券明细、来源消费 ID | 返回聚焦 W25 来源追溯区 |
| 商品 / SKU | W14 | `skuId`、下单快照 | 当前主数据只作对照，不覆盖历史 |
| 商品发布 | W22 | `productPublicationRevisionId` | 打开下单时修订；返回聚焦商品行 |
| 主责迁移 / T | W24 | `cutoverId`、`enabledAt` | 只读追溯，W25 不改履约链 |
| 供应商订单 | W26 | `supplierFulfillmentOrderId`、来源订单/明细 | W26 处理动作；返回保留子订单卡片 |
| API 结算 | W27 | 供应商订单、成本/退款来源 | 返回 W25 成本区 |
| 卡券消费与经营 | W28 | 客户、原销售单、卡实例、期间、消费条目 | 下钻打开 W25，返回保留分析筛选 |
| 接口错误与对账 | W29 | `factId`、`differenceId`、子订单或消息 ID | 处理后回 W25 刷新，不覆盖事实 |
| 历史消费回填 | W30 | `backfillJobId`、业务事实键、`[rangeStart,T)` | 回填项下钻 W25；实时与回填共用同一事实 |

跨工作面传稳定身份和筛选上下文，不传地址、完整卡实例引用、金额计算或“已归集”结论作为可信事实。

## 12. 验收清单

### 12.1 页面与事实视图

- [x] 单张订单一屏可读商城身份、金额、五类关键事实、支付分摊、来源销售和供应商履约。
- [x] W25 明确是事实追溯视图，不显示商城处理中间态，也没有修改商城订单入口。
- [x] 同一订单的多次部分退款和多次余额恢复逐笔展示，不按订单号合并。
- [x] 事实同时展示发生时间和接收时间；履约链只按支付事实发生时间与 `T` 比较。
- [x] 1440×900 下列表露出 6–8 行，订单身份和行级操作固定。

### 12.2 金额、分摊与归属

- [x] 支付来源只有卡券和微信；不存在福利账户或其它兼容分支。
- [x] 商品 × 支付来源矩阵行合计、列合计和订单实付均展示服务端守恒结果。
- [x] ERP 不按订单总额猜测优惠、运费或支付来源分摊。
- [x] 卡实例明确标注为非卡号，且能在权限内追溯到客户、原销售单和唯稳定一卡券明细。
- [ ] 微信支付不错误挂到企业卡券收入归属。
- [x] `NONE` 成本显示为空和原因，不按零成本进入任何利润暗示。

### 12.3 履约与三类退款事实

- [x] `T` 前支付只显示原人工履约，不创建供应商订单；`T` 后支付才进入自动履约。
- [x] `T` 后缺少发布/供给时保留支付事实，并进入差异而不是拒收或复制事实。
- [x] 供应商下单失败或结果未知时明确“商城支付已发生，正在处理履约异常”。
- [ ] 商城退款只冲减消费，卡券余额恢复只记余额回补；供应商退款未付部分冲减成本和开放应付，已付部分还会同事务反向原付款分配并登记供应商现金退款，且两者都不替代商城退款。
- [x] W25 不能旁路 W26/W29 重试供应商动作或删除原支付事实。
- [ ] 卡券稳定引用对应的基线/来源对象缺失或未归属由运营协调补齐；契约必需引用缺失不猜测补值；既有基线归属/余额冲突必须保留基线，经 `FINANCE_CORRECTION_REVIEW` 追加纠错。

### 12.4 幂等、安全、状态与响应式

- [ ] 实时与回填按同一业务事实键去重，同一事实只形成一份正式记录。
- [ ] 后到的不同支付版本进入差异，不创建第二份订单、消费或供应商动作。
- [ ] 第一份有效支付无论归集条件是否完整，都同事务形成不可变事实、唯一商城订单及可保存来源快照；条件齐全后按同一事实键补齐消费归属，`T` 后供给完整时原子形成确定性供应商子订单、首个 `PLACE` 动作和 outbox。
- [ ] 地址、手机号、支付引用、卡实例和成本字段按权限掩码；短时揭示有审计且到期清除。
- [ ] 导出使用服务端选择快照、字段清单和掩码，下载时重新鉴权。
- [ ] §9 所有状态完成组件或浏览器验收。
- [ ] 1440、1280、1024、768、375 五档视口符合 §10。

## 13. 待确认事项

| ID | 问题 | 影响 | 建议决策人 | 当前建议 |
| --- | --- | --- | --- | --- |
| Q1 | W25 默认查询期间应为 30 天、90 天还是按角色保存上次选择？ | 首屏性能、运营习惯和 Saved View | 运营 + 财务 + 客服 | 由服务端角色期间策略配置；未配置时不预填且必须显式选择完整 `occurredFrom/occurredTo`，不在前端暂定 30 天 |
| Q2 | 哪些客服/采购场景允许在 W25 短时揭示完整履约地址？ | 字段权限、审计和隐私 | 客服负责人 + 采购负责人 + 安全负责人 | 仅当前责任子订单所需角色可揭示；其余从 W26 受控查看 |
| Q3 | 商城关键事实从发生到 ERP 可见的正常延迟和告警阈值是多少？ | 数据陈旧、SLA 指标和 W29 升级 | 商城负责人 + 运维 | 服务端按事实类型返回 SLA 状态，前端不本地推断 |
| Q4 | 一张订单同时有多种成本口径时，列表主标签使用“混合”还是按最低可信口径显示？ | 列表扫读和财务风险表达 | 财务负责人 | 策略未配置时只展示各口径分项和金额占比，不返回归一主标签；确认后由服务端策略返回 `MIXED` 或选定可信口径，前端不自行挑选 |

## 14. 业务依据

- `erp-phase-2.md` §9：商城主责、五类关键事实、组合支付分摊和员工信息边界。
- `erp-phase-2.md` §10：固定供应关系、供应商子订单、状态与失败补偿。
- `erp-phase-2.md` §13：消息/事实双层幂等、可靠性、周期对账和监控。
- `erp-phase-2.md` §15 P2-P07/P2-P08、§16、§17.3–§17.5：W25/W26 页面、权限和验收。
- `erp-data-model.md` §6.17：唯一 `T`、卡实例、商城事实、订单、支付来源、分摊、消费与成本评估。
- `erp-data-model.md` §6.19、§7.6、§8.4、§9.4：供应商履约、结果未知、退款分轨和事务不变量。
- `erp-data-model.md` §12：消费/成本分析投影水位和重建边界。
- `erp-mall-data-mapping.md` §10.4–§10.5：共同信封、支付快照、行列守恒、退款结果及供应商数据契约。
- `erp-ui-design.md` §3.4–§3.5、§4.3、§4.5、§6、§9–§11、§15：TaskTabs、M2/M4、响应式、状态和正式结果。
- `erp-ui-flows.md` §11：W25 一屏信息、W26 协同和“支付已发生，处理履约异常”文案边界。
