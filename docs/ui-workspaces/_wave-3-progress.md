# Wave 3 Progress — fill-w-gaps Integrate

**Date:** 2026-08-01  
**Agent:** INTEGRATE (fill-w-gaps wave 3)  
**Workspaces:** W11, W12, W13, W14

## Build status

| Check | Result |
| --- | --- |
| `cd erp-client && npm run build` | **OK** |
| Compile (Turbopack) | OK (~6.2s) |
| TypeScript | OK (~4.7s) |
| Static generation | OK (33 routes) |

Implement JSON had mixed per-workspace `build_ok` (W11/W13 claimed false due to parallel type noise). **Full-repo `npm run build` succeeds** on this integrate pass.

## Checkbox integration

| Workspace | Confirmed (verify) | Flipped `[ ]`→`[x]` | Rejected (left open) | Open remaining in §12 |
| --- | ---: | ---: | ---: | ---: |
| W11 | 10 | **8** | 0 | 1 |
| W12 | 10 | **9** | 0 | 1 |
| W13 | 12 | **9** | 1 | 4 |
| W14 | 12 | **5** | 2 | 9 |
| **Total** | 44 | **31** | 3 | **15** |

Rules applied:

- Only **verify.confirmed** items flipped.
- **verify.rejected** never flipped.
- Prefer **exact** checklist string match; skip confirmed paraphrases that are strict subsets of a longer §12 line or that fail full acceptance wording.
- Confirmed items with **no matching §12 line** (e.g. W11 detail preview dual-entry, unallocated partition note; W12 detail+register entry; W14 object-center skeleton / export snapshot / disable-not-delete / selector-scene mock) were not invented as flips.

### Not flipped despite partial confirm / paraphrase

| WS | Confirmed (verify) | Checklist status | Reason |
| --- | --- | --- | --- |
| W11 | 列表 detail 预览 + 页头登记回款/销项发票双入口 | no §12 line | Behavior shipped; acceptance list has no dedicated checkbox |
| W11 | 待核销视图回款与发票分区且不相加 | no §12 line | Covered by fact-track separation flip; no separate row |
| W11 | 1440×900 / 五档视口 / 键盘核销 | left open | items_still_open; not in verify.confirmed as acceptance-proven |
| W12 | 列表 detail 预览 + 登记付款/进项发票入口 | no §12 line | Shipped; no dedicated §12 checkbox |
| W12 | 五档视口与完整键盘路径 | left open | items_still_open; not confirmed as measured |
| W13 | 历史回款/发票通过 W11 正式事实及多对多分配登记 | left open | **verify.rejected** — AllocationWorkspace theater; W13 overlay only |
| W13 | 领取/续租/他人占用/暂挂/转交/驳回/从 W05·W11 返回不丢上下文 | left open | Confirmed only 暂挂 + returnTo/queueContextId; 转交/他人租约 still_open |
| W13 | `receivable_funds_review` 同事务 | left open | Not confirmed as real server same-tx; session-mock only |
| W13 | 五档视口 + 键盘 + 焦点 + 读屏位置 | left open | Keyboard/SequentialProcessBar confirmed; 五档视口 combined line not proven |
| W14 | 有效期重叠 / SKU 规格 / 基础单位 / 有库存停用阻断 | left open | **verify.rejected** — simulate-* theater only |
| W14 | 键盘资源切换/搜索/版本/返回焦点恢复 | left open | **verify.rejected** — focus restore broken (`data-row-id` missing) |
| W14 | 敏感字段掩码/短时揭示/附件/权限收回 | left open | Reveal mock only; 附件+收回 still_open (subset of stronger line) |
| W14 | 五层权限分别验收 | left open | permissionDemo badges only; not role-switchable full matrix |
| W14 | 业务选择器场景 mock / 选择器影响摘要 | no exact full §12 match | Weaker than 销售选品/采购供应商/提交再校验 rows; leave those open |
| W14 | 行 detail 半屏 + 对象中心子区 / 导出筛选快照 / 停用非删除 | no single matching §12 line | Shipped; not flipped without checklist row |

## Confirmed items (flipped)

### W11 (`w11-customer-receivables.md`)

- [x] 应收、回款、销项发票在同一客户往来工作面可查，但事实和分配轨道明确分离。
- [x] 从 W05 到登记回款并核销，再返回 W05 不超过一个任务页签会话且上下文不丢。
- [x] 一笔回款可分配同主体多笔应收，一笔应收可由同主体多笔回款核销。
- [x] 销项发票独立完成多对多分配，不受回款核销进度替代。
- [x] 核销严格使用 `counterparty_party_id`；跨主体目标不会出现在池中且服务端再次拒绝。
- [x] 所有余额和净分配由服务端返回；前端拟合计不冒充正式结果。
- [x] 已过账事实和分配不可编辑/删除，退款、冲正和红票追加反向事实。
- [x] 幂等、并发余额变化、重复发票、结果不确定和权限收回均有可恢复状态。

### W12 (`w12-supplier-payables.md`)

- [x] 应付、付款、进项发票在同一工作面可查，但三类事实与两条核销轨道明确独立。
- [x] 一笔付款可分配同供应商多笔采购/结算应付，一笔应付可由同供应商多笔付款核销。
- [x] 进项发票可分配同供应商采购单和结算单可收票金额，不与付款进度混淆。
- [x] 不同供应商的目标不会进入同一核销池，服务端再次拒绝跨供应商提交。
- [x] 从 W08/W09 进入、完成付款并返回后，由来源页重新查询门禁；未核销付款不算满足。
- [x] 所有正式余额、净分配和门禁结论来自服务端，前端不自行推断。
- [x] 混合应付的默认顺序只采用服务端 `payablePriorityPolicyId/version`；策略缺失或陈旧时禁用混合自动分配并要求显式选择或分组。
- [x] 已过账事实不可编辑/删除，退款、冲正和红票追加反向事实并保留原记录。
- [x] 幂等、重复发票、并发余额变化、结果不确定和权限收回均可恢复。

### W13 (`w13-card-funds-review.md`)

- [x] `OPENING` 与 `SYNC_DELTA` 明确区分，后续任务不会复用或覆盖期初复核。
- [x] 一屏看清同步成交额、当前应收、净已收、净已开票、证据和当前指纹状态。
- [x] “从 0 起”不会创建 0 元回款/发票，且必须有明确证据和强确认。
- [x] 完成时重新计算并三方校验 `subject_hash`；变化时阻断而非静默通过。
- [x] 复核链递增、单根不分叉，旧记录不可编辑删除，当前缓存可从链重建。
- [x] 所有正式结论使用 `CompleteWorkItemEnvelope<CardFundsReviewDecision>`；账户、链尾、结论和领域版本全部位于 `decision`，领取令牌只使用 `claimToken`。
- [x] Q5 未决时，`REJECTED` 只形成驳回复核事实并完成当前任务；结果固定显示配置 blocker/协作说明，前后端均不能猜测或创建驳回后继任务。
- [x] 处理成功先展示固定复核号/结果再自动下一项；结果不确定时不移动。
- [x] 复核未完成时 W11/W15 能识别指标不可靠，不以 0 值冒充已核实。

### W14 (`w14-master-data.md`)

- [x] 列表能同时识别稳定身份、当前版本、启停生命周期、修订时序、生效区间和主要阻塞原因；“待生效”不会混入启停状态。
- [x] 新建、形成新版本和停用均保留原因、操作者、时间与正式结果。
- [x] 历史版本可读，当前名称变化不改变历史单据快照。
- [x] 仓库 SKU 策略只生成预警，不改变库存余额。
- [x] Q1 未确认期间仓库资料与策略可查询但所有仓库写操作均 fail-closed；仓储和系统管理员都不能直接维护。

## Files changed (from implement JSON; not re-touched by Integrate except docs)

### W11

```
erp-client/app/(workspace)/finance/customer-accounts/page.tsx
erp-client/features/customer-receivables/{types,api,queries,session,customer-receivables-page,allocation-session-panel}.ts(x)
erp-client/mock/customer-receivables.ts
erp-client/features/sales-orders/sales-order-detail-page.tsx
```

### W12

```
erp-client/app/(workspace)/finance/supplier-accounts/page.tsx
erp-client/features/supplier-payables/{types,api,queries,supplier-accounts-page,allocation-session}.ts(x)
erp-client/mock/{supplier-payables,session-state}.ts
erp-client/features/purchase-orders/purchase-order-detail-page.tsx
erp-client/features/fulfillment-operations/api.ts
```

### W13

```
erp-client/app/(workspace)/finance/card-funds-review/page.tsx
erp-client/features/card-funds-review/{types,api,queries,card-funds-review-page}.ts(x)
erp-client/mock/{card-funds-review,session-state,workspace-pages}.ts
```

### W14

```
erp-client/features/master-data/{types,data,session,filter,api,queries,master-data-page,master-data-preview,master-data-action-dialog,master-data-center-page}.ts(x)
erp-client/app/(workspace)/master-data/[resource]/[stableId]/page.tsx
```

**Docs touched by Integrate:**

- `docs/ui-workspaces/w11-customer-receivables.md`
- `docs/ui-workspaces/w12-supplier-payables.md`
- `docs/ui-workspaces/w13-card-funds-review.md`
- `docs/ui-workspaces/w14-master-data.md`
- `docs/ui-workspaces/_wave-3-progress.md` (this file)

## Counts summary

| Metric | Value |
| --- | ---: |
| verify.confirmed | 44 |
| checkboxes_flipped | **31** |
| verify.rejected | 3 |
| Wave-3 §12 open remaining | **15** |
| All `docs/ui-workspaces/w*.md` open `- [ ]` | **443** |
| build_ok | **true** |

### High-priority leftovers (wave 3)

| WS | Theme |
| --- | --- |
| W11 | 1440×900 / five-viewport + keyboard alloc acceptance; bank-ref reveal ACL/TTL/audit; W02 dual-control finance correction; export BackgroundJob 7-day reauth; W13 deep-link reliability matrix; UI for `bumpW11ReceivableBaseline` |
| W12 | Five-viewport + ⌘S/⌘↵ keyboard path; server export job; W02 SoD review queue; bank full-reveal short auth; independent supplier-refund reverse type |
| W13 | Real same-tx server (funds_review + workflow_action); bidirectional W11 formal fact pipeline (rejected theater); TransferWorkItemEnvelope UI; full lease/others-occupied; five-viewport acceptance; live allowedActions policy |
| W14 | EFFECTIVE_RANGE/SPEC/BASE_UNIT real validation (not simulate); keyboard focus restore via `data-row-id`; five-layer role demos; attachment + revoke scrub; cross-W eligibility re-check; object-center `revision=` URL/TaskTabs; §9/§10 matrix |

## Recommended next_wave

**next_wave: 4**

Suggested focus order:

1. **Shared §9 / §10.1 harness** — five-viewport + state-matrix acceptance (unblocks W11–W14 combined viewport lines and many earlier leftovers).
2. **W13 honesty** — either wire registerHistorical* into W11 formal receipt/invoice/allocation APIs, or keep rejected and stop “W11 内核” copy theater.
3. **W14 validation + focus** — real range/spec/unit blockers; fix DataTable `data-row-id` for preview close focus restore.
4. **Finance platform** — bank reveal ACL/TTL, export BackgroundJob reauth, W02 CompleteWorkItemEnvelope for finance corrections (W11/W12).
5. **Next workspace slice** — W15–W20 analytics/governance/permissions surfaces or remaining open W docs.

## Summary

Wave 3 integrated **31** verified checklist items across W11/W12/W13/W14. **3** verify-rejected claims stayed open (W13 W11-fact theater; W14 simulate validation + keyboard focus restore). Several confirmed paraphrases were **not** flipped because §12 lines demand stronger envelope/viewport/transfer proof, or lack a matching checkbox. Full `npm run build` **passes**. **15** open items remain in wave-3 workspace docs; **443** open across all W docs. Recommend **wave 4** for shared acceptance harness + W13/W14 honesty fixes + next open workspaces.
