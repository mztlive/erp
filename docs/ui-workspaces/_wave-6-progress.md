# Wave 6 Progress — fill-w-gaps Integrate

**Date:** 2026-08-01  
**Agent:** INTEGRATE (fill-w-gaps wave 6)  
**Workspaces:** W25, W26, W27, W28, W29, W30

## Build status

| Check | Result |
| --- | --- |
| `cd erp-client && npm run build` | **OK** |
| Compile (Turbopack) | OK (~7.0s) |
| TypeScript | OK (~6.3s) |
| Static generation | OK (33 routes) |

Implement JSON claimed per-workspace `build_ok=false` for W25/W27/W28/W29/W30 (blocked by concurrent TS) and `build_ok=true` for W26. **Full-repo `npm run build` succeeds** on this integrate pass (routes for consumption-orders, supplier-api orders/settlements, card-business, integration-errors, history-backfill all present).

## Checkbox integration

| Workspace | Confirmed (verify) | Flipped `[ ]`→`[x]` | Rejected (left open) | Open remaining in §12 |
| --- | ---: | ---: | ---: | ---: |
| W25 | 12 | **14** | 0 | 10 |
| W26 | 13 | **11** | 0 | 9 |
| W27 | 12 | **12** | 1 | 10 |
| W28 | 12 | **14** | 0 | 5 |
| W29 | 12 | **13** | 0 | 9 |
| W30 | 12 | **17** | 0 | 2 |
| **Total** | 73 | **81** | 1 | **45** |

Rules applied:

- Only **verify.confirmed** items flipped.
- **verify.rejected** never flipped (W27 keyboard path left open).
- Prefer **exact** §12 string match; skip confirmed paraphrases that are strict subsets of longer envelope/export/§9 lines or that fail full acceptance wording.
- Confirmed items with **no matching §12 line** (URL-only restore, dual-pane layout, nested routes) were not invented as flips.
- Several confirmed claims map to **multiple** §12 lines (e.g. W25 matrix+CARD/WECHAT; W28 CostCoverageNotice+tax; W30 range/resume/report).

### Not flipped despite partial confirm / paraphrase

| WS | Confirmed (verify) | Checklist status | Reason |
| --- | --- | --- | --- |
| W25 | Field-mask + export BatchImpactPreview | left open | §12 requires 短时揭示到期清除 + 导出服务端快照/下载重新鉴权; notes: REVEAL_ADDRESS copy-only, client mock job only |
| W25 | §9 empty/no-permission/no-scope/filter-empty | left open | §12 requires **full** §9 matrix (stale watermark, mid-session revoke, projection lag still open) |
| W25 | 微信支付不错误挂到企业卡券收入归属 | left open | Matrix excludes welfare; income-attribution wording not explicitly confirmed |
| W25 | 商城退款/供应商退款三轨正式账本 | left open | items_still_open formal ledger drill-through |
| W26 | 暂挂 / 查询重放 W02 信封 | left open | Full claimToken/lease/subject-hash matrix still open; mock keeps PENDING only |
| W26 | 地址揭示 + 权限收回无缓存泄漏 | left open | Timed reveal mock only; mid-session revoke hard clear limited to unmount |
| W26 | 取消/退款领域命令 + 可验证终态/转交 | left open | TRANSFER_MANUAL / CONFIRM_VERIFIED_TERMINAL not wired end-to-end |
| W27 | Keyboard path: open preview → diff → 查询正式结果 | **rejected** | Enter opens center not preview; forceUnknown never set in UI |
| W27 | CompleteWorkItemEnvelope / Q4 cutoff / VOID export | left open | demo claimToken; items_still_open |
| W28 | 导出含水位/覆盖率并下载重新鉴权 | left open | Preview+job watermark only; no backend re-auth download |
| W28 | 成本口径按消费记录 append-only 评估链 | left open | Per-row fixture only |
| W28 | 每日全量重建 / 销售客户隔离 RBAC | left open | freshnessDemo + fieldHide demos only |
| W29 | WorkItemActionEnvelope / CloseWorkItemEnvelope full | left open | Session-mock; CLOSE_MISROUTED no UI button; real server idempotency open |
| W29 | 任务领取租约跨刷新恢复 | left open | session-memory claim only |
| W29 | §9/§10 + 键盘连续处理全路径 | left open | items_still_open |
| W30 | REATTRIBUTE UI / CONFIRM_REPORT happy-path seed | left open | API-only; no PENDING→CONFIRMED demo button |
| W30 | §9/§10 | left open | no exhaustive matrix |

## Confirmed items (flipped)

### W25 (`w25-mall-consumption-orders.md`)

- [x] 单张订单一屏可读商城身份、金额、五类关键事实、支付分摊、来源销售和供应商履约。
- [x] W25 明确是事实追溯视图，不显示商城处理中间态，也没有修改商城订单入口。
- [x] 同一订单的多次部分退款和多次余额恢复逐笔展示，不按订单号合并。
- [x] 事实同时展示发生时间和接收时间；履约链只按支付事实发生时间与 `T` 比较。
- [x] 1440×900 下列表露出 6–8 行，订单身份和行级操作固定。
- [x] 支付来源只有卡券和微信；不存在福利账户或其它兼容分支。
- [x] 商品 × 支付来源矩阵行合计、列合计和订单实付均展示服务端守恒结果。
- [x] ERP 不按订单总额猜测优惠、运费或支付来源分摊。
- [x] 卡实例明确标注为非卡号，且能在权限内追溯到客户、原销售单和唯稳定一卡券明细。
- [x] `NONE` 成本显示为空和原因，不按零成本进入任何利润暗示。
- [x] `T` 前支付只显示原人工履约，不创建供应商订单；`T` 后支付才进入自动履约。
- [x] `T` 后缺少发布/供给时保留支付事实，并进入差异而不是拒收或复制事实。
- [x] 供应商下单失败或结果未知时明确“商城支付已发生，正在处理履约异常”。
- [x] W25 不能旁路 W26/W29 重试供应商动作或删除原支付事实。

### W26 (`w26-supplier-orders.md`)

- [x] 一屏能同时看清商城支付已发生、供应商订单身份及履约/取消/退款三条进度。
- [x] 已完成但部分退款能被正确表达，不因单一综合状态丢失事实。
- [x] 结果未知的唯一主路径是先查询原结果，不能直接再次下单。
- [x] 只有明确无结果且服务端确认可安全重试时才开放重放，并沿用原幂等键。
- [x] 取消和退款必须引用既有商城售后请求，重复提交不重复调用供应商。
- [x] 三类退款相关事实能分别看见缺口和责任方。
- [x] 履约主状态只使用九个正式枚举；“结果未知”快捷筛选等价于 `fulfillmentStatus = RESULT_UNKNOWN`，没有独立状态源。
- [x] 下单时发布版本、固定供给、商品和成本快照不受后续主数据变化影响。
- [x] 业务页和日志不展示密钥、完整请求报文或未脱敏响应。
- [x] 正式动作返回固定结果；超时可按 `operationId` / 幂等键查询，不靠 toast 猜状态。
- [x] 1440×900 首屏至少显示 6 条有效订单，身份和操作列固定。

### W27 (`w27-api-settlement.md`)

- [x] 财务能在一个对象中心完成汇总核对、差异处理、提交复核和确认结算。
- [x] 未解决的阻断差异不能确认结算，处理结论均有证据和审计。
- [x] 采购只能追加供应商证据和意见；财务经办登记正式差异结论，另一名财务复核确认结算。
- [x] 经办和复核不能为同一人，前后端均有明确反馈。
- [x] 结算确认同事务追加成本差额并形成唯一应付，结果显示应付编号。
- [x] 确认后付款、进项发票和核销进入 W12，不在 W27 复制一套财务流程。
- [x] 供应商账单原值、订单、原成本和已确认结算均不可被页面覆盖。
- [x] ERP 金额、供应商金额和差异方向使用服务端舍入结果，含税/不含税标注清楚。
- [x] 版本变化会使旧提交失效；不会静默确认过期试算。
- [x] 新建草稿必须引用供应商当前结算期间策略及版本；策略缺失、过期或期间不匹配时 fail-closed，不接受任意自然日范围。
- [x] 1440×900 首屏显示至少 6 条结算单，身份和操作列固定。
- [x] 无模块权限、无数据范围、无结算单和筛选无结果可区分。

### W28 (`w28-card-consumption-analytics.md`)

- [x] 任何消费毛差、当前经营贡献和最终盈亏均与成本覆盖率同屏。
- [x] `ACTUAL`、`STANDARD`、`NONE` 的消费金额和占比同时可见，三者合计等于累计卡券消费。
- [x] `NONE` 不显示为零成本，也不进入成本和利润指标。
- [x] 成本覆盖不足阈值时利润明确标记“成本不完整，结果仅供参考”。
- [x] 销售/面值/消费/余额使用含税口径，成本和利润使用不含税口径，页面逐项标注。
- [x] 消费毛差两侧均为不含税；进项税率不被销项税率替代。
- [x] 当前经营贡献与未履约余额同屏，未到期范围不展示“最终利润”。
- [x] 微信支付消费与成本不进入企业卡券消费和利润指标。
- [x] Q2 默认日期口径未配置时不静默采用“本月/消费发生日”，也不显示虚假 0 指标；用户显式选择的 `from/to/dateBasis` 完整进入 URL 和 Query Key。
- [x] 投影、正式事实 outbox 和余额快照分别展示水位；`lagSeconds`、固定 60 秒 SLA 与越界告警可见，陈旧数据不宣称实时。
- [x] 稳定卡实例引用不可反推卡号/卡密，页面不存在卡号、卡密和绑定手机号字段。
- [x] 指标、图表、明细使用同一筛选摘要和数据水位。
- [x] 图表具备键盘和读屏等价数据，不依赖颜色或 hover。
- [x] 浏览器刷新、后退和跨对象下钻能恢复分析上下文。

### W29 (`w29-integration-error-reconciliation.md`)

- [x] 结果未知时不能直接重放，下单/取消/退款均先查询原结果。
- [x] 只有明确无结果且服务端确认安全时开放重放；本次任务动作幂等键与服务端锁定的 `originalActionIdempotencyKey` 分离，客户端不能传入或替换原键。
- [x] 业务明确拒绝、参数/映射错误和鉴权/签名失败不会进入无意义自动重试。
- [x] 原消息、尝试、差异、正式事实和处理记录均不可被页面覆盖。
- [x] 错误详情内能完成查询、重放、转交、关联补偿和终态验证，不需去日志平台猜结果。
- [x] 结果未知、资金未闭环或补偿未完成的任务不能通用关闭。
- [x] 解决必须引用与错误类型、资金影响匹配的 `evidencePolicyId/evidencePolicyVersion`、非空强类型证据并通过岗位分离；策略未配置时只能补证、暂挂或转交。
- [x] 无任务直接对账的“确认无误/有效差异”必须引用已配置原因注册表中的强类型原因和非空受控证据；注册表缺失时保持非终态，不接受自由字符串原因或可选证据。
- [x] 对账只生成差异，修改业务必须进入正式变更、纠错、重新归集或重放入口。
- [x] 无任务的直接对账命令只追加差异处理记录，不会隐式完成、转交或关闭 `work_item`。
- [x] 普通业务页面和导出不出现密钥、完整请求/响应、完整手机号和地址。
- [x] 环境、严重度和状态不只靠颜色表达。
- [x] 正式动作结果固定展示；结果不确定时停留当前项且不自动下一条。

### W30 (`w30-historical-consumption-backfill.md`)

- [x] `rangeStart` 固定等于目标范围最早业务事实或已登记历史边界 `requiredHistoryStart`，正式任务范围严格为 `[requiredHistoryStart,T)`。
- [x] 来源覆盖起点晚于 `requiredHistoryStart`、存在任一区间缺口或边界无法证明时阻断正式执行；不能缩晚起点后标记“全历史完成”。
- [x] `occurredAt = T` 不进入历史回填，按实时/补投契约处理。
- [x] `T` 前支付只补台账，全部标记 `LEGACY_MANUAL`，不创建供应商订单。
- [x] 五类关键事实完整回填，同一订单下支付、取消、完成、多次退款和多次余额恢复不会被合并。
- [x] 实时与回填按同一业务事实键去重，只形成一份正式事实。
- [x] 失败或中断只续跑原任务、原范围和原幂等键，不新建重叠正式批次。
- [x] 回填不覆盖现有实时事实、消费、退款、余额恢复、成本或成本评估。
- [x] 商城订单成本有完整税口径时标 ACTUAL；否则按消费时点供给版本标 STANDARD；仍无来源标 NONE。
- [x] 不使用当前供给价、不猜测税率、不用销项税率替代进项税率。
- [x] NONE 成本为空而不是 0，只进入消费金额和覆盖率分母。
- [x] 技术报告及其确认后版本均包含范围、T、总笔数/金额、去重数、ACTUAL/STANDARD/NONE、覆盖率、未归集和失败清单。
- [x] 报告、列表和明细统计使用同一任务快照并可追溯规则/Schema 版本。
- [x] `processingStatus` 与 `reportReviewStatus` 分开验收；技术 `COMPLETED` 不等于报告已确认或全业务完成。
- [x] 报告复核策略未配置或报告未确认时，下载文件固定标“未确认”，确认动作和正式下游门禁 fail-closed；不会仅因技术完成解锁。
- [x] 普通页面、日志和导出不泄露卡号、卡密、绑定手机号、完整地址或原始敏感报文。
- [x] 后台任务不伪装同步完成；进度滞留、部分完成和失败均有明确恢复路径。

## Files changed (from implement JSON; not re-touched by Integrate except docs)

### W25

```
erp-client/features/mall-consumption-orders/{types,api,queries,consumption-orders-list-page,consumption-order-center-page}.ts(x)
erp-client/mock/mall-consumption-orders.ts
erp-client/mock/workspace-pages.ts
erp-client/app/(workspace)/commerce/consumption-orders/page.tsx
erp-client/app/(workspace)/commerce/consumption-orders/[mallOrderId]/page.tsx
```

### W26

```
erp-client/features/supplier-orders/{types,api,queries,url-state,supplier-orders-list-page,supplier-order-preview-panel,supplier-order-center-page}.ts(x)
erp-client/mock/supplier-orders.ts
erp-client/app/(workspace)/supplier-api/orders/page.tsx
erp-client/app/(workspace)/supplier-api/orders/[supplierOrderId]/page.tsx
```

### W27

```
erp-client/features/supplier-settlements/{types,url-state,api,queries,supplier-settlements-page}.ts(x)
erp-client/mock/supplier-settlements.ts
erp-client/app/(workspace)/supplier-api/settlements/page.tsx
erp-client/app/(workspace)/supplier-api/settlements/[statementId]/page.tsx
```

### W28

```
erp-client/app/(workspace)/analytics/card-business/page.tsx
erp-client/features/card-business-analytics/{types,api,queries,card-business-analytics-page}.ts(x)
erp-client/mock/card-business-analytics.ts
```

### W29

```
erp-client/app/(workspace)/governance/integration-errors/page.tsx
erp-client/app/(workspace)/governance/integration-errors/errors/[taskId]/page.tsx
erp-client/app/(workspace)/governance/integration-errors/differences/[differenceId]/page.tsx
erp-client/features/integration-errors/{types,url-state,api,queries,integration-errors-page,integration-error-detail-page}.ts(x)
erp-client/mock/integration-errors.ts
```

### W30

```
erp-client/features/history-backfill/{types,url-state,api,queries,history-backfill-page}.ts(x)
erp-client/mock/history-backfill.ts
erp-client/app/(workspace)/governance/history-backfill/page.tsx
erp-client/app/(workspace)/governance/history-backfill/[jobId]/page.tsx
```

**Docs touched by Integrate:**

- `docs/ui-workspaces/w25-mall-consumption-orders.md`
- `docs/ui-workspaces/w26-supplier-orders.md`
- `docs/ui-workspaces/w27-api-settlement.md`
- `docs/ui-workspaces/w28-card-consumption-analytics.md`
- `docs/ui-workspaces/w29-integration-error-reconciliation.md`
- `docs/ui-workspaces/w30-historical-consumption-backfill.md`
- `docs/ui-workspaces/_wave-6-progress.md` (this file)

**Implement files_changed (unique paths):** 51

## Counts summary

| Metric | Value |
| --- | ---: |
| verify.confirmed | 73 |
| checkboxes_flipped | **81** |
| verify.rejected | 1 |
| Wave-6 §12 open remaining | **45** |
| All `docs/ui-workspaces/w*.md` open `- [ ]` | **243** |
| build_ok | **true** |

### High-priority leftovers (wave 6)

| WS | Theme |
| --- | --- |
| W25 | REVEAL_ADDRESS dialog+expiry; export re-auth download; full §9/§10; server payment-version/idempotency; three-track refund ledger drill |
| W26 | W02 claim/lease/subject-hash UI; CONFIRM_VERIFIED_TERMINAL + TRANSFER_MANUAL; BatchImpactPreview 7-day export; permission-revoke scrub; §9/§10/keyboard |
| W27 | Real CompleteWorkItemEnvelope claim/lease; VOID draft + export job; BackgroundJobProgress scan; cutoff policy UI; rejected keyboard forceUnknown path; §9/§10 |
| W28 | Full RBAC customer isolation; daily rebuild job; append-only cost-assessment chain; export re-auth; §9/§10 |
| W29 | CLOSE_MISROUTED UI; real server idempotency; claimToken across refresh; live backoff; full envelope matrix; §9/§10 continuous keyboard path |
| W30 | CONFIRM_REPORT happy-path seed + button; real file download; REATTRIBUTE UI; §9/§10 |

## Recommended next_wave

**next_wave: 7**

Suggested focus order:

1. **Shared §9 / §10 harness** — five-viewport + keyboard/screen-reader acceptance residual across W20–W30 open lines.
2. **W02 envelopes honesty pass** — claimToken/lease/subject-hash conflict UI for W26/W27/W29 (or keep explicit blockers).
3. **Export / reveal polish** — W25 address reveal+audit expiry; server snapshot export re-auth pattern; W28/W30 download disclaimers vs real jobs.
4. **W27 rejected keyboard path** — preview-on-Enter vs center; forceUnknown 查询正式结果 demo.
5. **W30 report confirm happy path** — reportReviewPolicy seed PENDING→CONFIRMED + CONFIRM_REPORT button.
6. **Cross-wave residual** — permission-revoke scrub, CLOSE_MISROUTED UI, REATTRIBUTE after mapping repair, three-track refund formal drill.

## Summary

Wave 6 integrated **81** verified checklist items across W25–W30 specialized session-mock SPAs (mall consumption fact-trace list+center, supplier three-track orders, API settlement SoD+diff, card-business CostCoverageNotice analytics, integration-errors M3 dual-pane, history-backfill jobs+report). **1** verify-rejected W27 keyboard claim stayed open. Compound envelope/export/§9 lines that exceed mock evidence were **not** flipped. Full `npm run build` **passes**. **45** open items remain in wave-6 workspace docs; **243** open across all W docs. Recommend **wave 7** for shared acceptance residual + W02/export/reveal honesty.
