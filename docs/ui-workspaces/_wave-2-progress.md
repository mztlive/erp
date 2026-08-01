# Wave 2 Progress — fill-w-gaps Integrate

**Date:** 2026-08-01  
**Agent:** INTEGRATE (fill-w-gaps wave 2)  
**Workspaces:** W03, W04, W08, W09, W10

## Build status

| Check | Result |
| --- | --- |
| `cd erp-client && npm run build` | **OK** |
| Compile (Turbopack) | OK (~7.1s) |
| TypeScript | OK (~4.0s) |
| Static generation | OK (33 routes) |

Implement JSON had mixed per-workspace `build_ok` (W03/W04/W10 claimed false). **Full-repo `npm run build` succeeds** on this integrate pass (W09 scope typing and related blocks from wave 1 appear resolved).

## Checkbox integration

| Workspace | Confirmed (verify) | Flipped `[ ]`→`[x]` | Rejected (left open) | Open remaining in §12 |
| --- | ---: | ---: | ---: | ---: |
| W03 | 12 | **11** | 0 | 5 |
| W04 | 10 | **10** | 2 | 7 |
| W08 | 12 | **10** | 0 | 8 |
| W09 | 12 | **10** | 1 | 12 |
| W10 | 11 | **8** | 0 | 1 |
| **Total** | 57 | **49** | 3 | **33** |

Rules applied:

- Only **verify.confirmed** items flipped.
- **verify.rejected** never flipped.
- Prefer **exact** checklist string match; skip confirmed paraphrases that are strict subsets of a longer §12 line or that fail full acceptance wording.
- Confirmed items with **no matching §12 line** (e.g. object-route deep-link wording, W04/W05 `customerId` preselection consumption, W10 row-export secondary) were not invented as flips.

### Not flipped despite partial confirm / paraphrase

| WS | Confirmed (verify) | Checklist status | Reason |
| --- | --- | --- | --- |
| W03 | Object route `/sales/customers/:customerId` + section | no §12 line | Route exists; acceptance list has no matching checkbox |
| W03 | Related-object actions open W04/W05 with customerId | no §12 line | Links shipped; W05 list still ignores customerId (still_open) |
| W04 | 1440×900 density 6–8×36px rows | left open | **verify.rejected** — no viewport measurement |
| W04 | Keyboard list → detail → center → edit/submit | left open | **verify.rejected** — partial hooks only; no EDIT_DRAFT editor |
| W04 | 销售单保存 `contractRevisionId` / 附件短链审计 / 自动保存指纹 / 保存失败保留输入 / §9§10 | left open | Not in verify.confirmed (or incomplete vs still_open) |
| W08 | Draft save/submit + FormalActionResult retain | `- [ ]` 保存、提交、**审核、作废**和变更… | Subset of longer multi-action line; 作废 formal not shipped |
| W08 | Finance audit read-only pass/reject | `- [ ]` …完整使用 W02 `CompleteWorkItemEnvelope` | Honest mock only; envelope still open |
| W08 | 拆单合并规则 / 首次行引用 W07 / 暂挂 envelope / 审核同事务 / §9§10 | left open | Not confirmed |
| W09 | 1440×900 five water-levels + ≥5 queue + form co-visible | left open | **verify.rejected** — structure ≠ layout acceptance |
| W09 | Per-type M5 forms validation | no single matching line | Covered indirectly by formal success / keyboard; no exact §12 row |
| W09 | reverse/append corrections not overwrite | left open | Copy/guards yes; **full reverse/return UI still open** |
| W09 | 电子/服务敏感隔离 / 付款门禁 / Q1 任务模型 / §9§10 | left open | still_open / incomplete vs stronger checklist wording |
| W10 | Keyboard focus restore + phone readonly | combined with 五档视口 line | Viewport matrix not proven; leave combined line open |
| W10 | Row actions / export secondary / W09 deep links | no exact single §12 line | Behavior present; no dedicated checklist row |

## Confirmed items (flipped)

### W03 (`w03-customer-center.md`)

- [x] 1440×900 首屏同时展示客户身份、负责人、关系指标、主动作和首个业务摘要。
- [x] 用户可在客户中心内找到当前有效联系人、地址、合同和销售单，不去多个一级菜单拼现状。
- [x] 客户中心只展示票款和经营摘要，正式核销与分析分别进 W11/W15。
- [x] 客户主体修订产生新版本，历史合同和销售单快照不被覆盖。
- [x] 任一客户同一时点恰好一个 OWNER，协作销售有明确有效期。
- [x] 联系人手机、地址和银行账号按字段权限返回与掩码，日志不含完整值。
- [x] 应收、逾期和经营标签全部来自服务端，前端不从关联列表求和。
- [x] 无客户、筛选无结果、无数据范围和客户已停用四种状态可明确区分。
- [x] 分区失败不清空已确认的客户主体与其它正常分区。
- [x] 保存、版本冲突和结果不确定都保留用户输入且不乐观覆盖正式版本。
- [x] 键盘可完成选择客户、切换锚点、打开关联对象和保存简单修订。

### W04 (`w04-contracts.md`)

- [x] 合同编号与行主动作固定，横向滚动不丢对象身份。
- [x] 单击行打开 detail 后可读完客户、结算/开票、有效期、附件与关联销售摘要，无需再点中心才读主事实。
- [x] 纸质预览使用宽 Dialog/打印页，不与 detail 半屏混用。
- [x] 所有可见工具栏控件可操作；未接入能力隐藏或显示不可用原因。
- [x] 合同编号唯一，对象页签以稳定 `contractId` 为身份。
- [x] 一份合同可关联多张销售单，但 UI 不自行创造“合同金额”事实。
- [x] 合同到期/终止后不进新销售单选择器，历史销售快照和合同版本仍可追溯。
- [x] 列表导出使用服务端选择快照与下载重新鉴权，不用前端当前页拼出全量结果。
- [x] `contractRevisionPolicy` 缺失时已生效合同只读，`REVISE` 不出现在允许动作中，深链或直接请求也不能创建修订工作副本。
- [x] 正式动作成功固定展示合同号、修订号、时间和下一步，不只靠 toast。

### W08 (`w08-purchase-orders.md`)

- [x] 1440×900 M2 首屏至少显示 6–8 条有效行，采购单号和行级动作固定。
- [x] 单击采购单在 detail 半屏读完状态、供应商、来源销售、明细、金额、票款和履约主事实。
- [x] 对象中心同屏可到应付/付款、进项发票、履约、变更和关联销售，无需跨三个菜单拼现状。
- [x] 编辑、查看、审核不建立三套平行路由；同一采购对象保持一个 TaskTab 身份。
- [x] 一张采购单严格限制为一张销售单、一个供应商、一种采购类型、一套付款条件和一个履约责任。
- [x] W07/W05 建单入口只消费采购创建依据，不要求未注册的采购建单 `work_item`；其缺失不得阻断 W07 销售通过。
- [x] 含税、不含税、税额按服务端舍入结果展示，销售/仓储无权时不泄露成本。
- [x] 生效后变化只走采购变更，不覆盖已发生付款、发票或履约事实。
- [x] `PrepaymentGate` 使用服务端有效付款净核销结果，四类采购履约入口均不能绕过。
- [x] 键盘可完成列表核对、草稿保存/提交和审核导航；焦点返回正确。

### W09 (`w09-fulfillment-operations.md`)

- [x] 侧栏只有一个“履约作业”入口；入库、仓发、代发、电子、服务用同页分段筛选。
- [x] 五类作业复用同一队列、租约、结果和自动下一项语言，但分别调用强类型正式事务。
- [x] 采购/仓储从默认着陆到处理第一项不超过两次点击。
- [x] 从 W05/W08/W10 进入时无需再次搜索对象，返回仍保留来源页签。
- [x] 入库合格量原子形成库存增加和销售预占，不合格量不入库存。
- [x] 仓发必须消耗本销售明细预占；直发不得写自有库存流水。
- [x] 选中契约的暂挂携带原因和幂等键，只按服务端结果释放租约与移动本轮游标，不写 `paused` 或第二任务状态。
- [x] 结果不确定时不乐观修改库存、预占、履约进度或队列位置。
- [x] 正式成功固定显示强类型事实号、库存/预占影响、剩余量和验收下一步。
- [x] 键盘可完成队列切换、保存、正式确认和结果继续；焦点恢复正确。

### W10 (`w10-inventory-ledger.md`)

- [x] 1440×900 首屏同时看见筛选、6–8 条余额和固定身份/动作列。
- [x] 任一余额可在一次打开详情后追溯最后流水、来源单据和有效预占。
- [x] 页面没有“编辑库存”或直接释放预占能力，调整从当前余额上下文创建正式单据。
- [x] 卡券实体卡、供应商直发、电子交付和线下服务不进入自有库存余额。
- [x] `available_quantity`、总数和状态均使用服务端正式结果，前端不重算后覆盖。
- [x] 期初库存只能来自 W18 基准日实盘导入，旧商城库存字段不作为事实。
- [x] 库存调整覆盖岗位分离、幂等、并发冲突和结果不确定。
- [x] 空数据、筛选无结果、无数据范围和权限收回可明确区分。

## Rejected items (not flipped)

### W04

1. **1440×900 density: header + metrics + toolbar + pagination still show 6–8×36px rows** — Compact density and seed rows exist; no viewport measurement / acceptance artifact.
2. **Keyboard: list search → detail → open center → edit/submit validation** — Partial DataTable/form hooks only; no draft EDIT_DRAFT editor on object center; full path not shipped.

### W09

1. **1440×900 co-visibility: five water-levels, ≥5 queue rows, source context, critical form, primary action** — MetricStrip + grid + sticky bar present; no browser/viewport proof of simultaneous co-visibility.

## Files changed (implement wave 2, unique)

```
erp-client/app/(workspace)/fulfillment/page.tsx
erp-client/app/(workspace)/inventory/page.tsx
erp-client/app/(workspace)/procurement/orders/page.tsx
erp-client/app/(workspace)/procurement/orders/[purchaseOrderId]/page.tsx
erp-client/app/(workspace)/sales/contracts/page.tsx
erp-client/app/(workspace)/sales/contracts/[contractId]/page.tsx
erp-client/app/(workspace)/sales/customers/page.tsx
erp-client/app/(workspace)/sales/customers/[customerId]/page.tsx
erp-client/features/contracts/{types,api,queries,filter-contracts,contracts-list-page,contract-preview-panel,contract-paper-dialog,contract-detail-page}
erp-client/features/customers/{types,session,api,queries,filter-customers,customer-center-page,customer-detail-page,customer-form-sheet}
erp-client/features/fulfillment-operations/{types,api,queries,fulfillment-operations-page}
erp-client/features/inventory/{types,api,queries,inventory-ledger-page}
erp-client/features/procurement-confirmation/procurement-confirmation-page.tsx
erp-client/features/purchase-orders/{types,api,queries,purchase-orders-list-page,purchase-order-preview-panel,purchase-order-detail-page}
erp-client/features/sales-orders/sales-order-detail-page.tsx
erp-client/mock/{customers,contracts,purchase-orders,fulfillment-operations,inventory,session-state,work-items}
```

**Docs touched by Integrate:**

- `docs/ui-workspaces/w03-customer-center.md`
- `docs/ui-workspaces/w04-contracts.md`
- `docs/ui-workspaces/w08-purchase-orders.md`
- `docs/ui-workspaces/w09-fulfillment-operations.md`
- `docs/ui-workspaces/w10-inventory-ledger.md`
- `docs/ui-workspaces/_wave-2-progress.md` (this file)

## Counts summary

| Metric | Value |
| --- | ---: |
| verify.confirmed | 57 |
| checkboxes_flipped | **49** |
| verify.rejected | 3 |
| Wave-2 §12 open remaining | **33** |
| All `docs/ui-workspaces/w*.md` open `- [ ]` | **474** |
| build_ok | **true** |

### High-priority leftovers (wave 2)

| WS | Theme |
| --- | --- |
| W03 | Global search / multi-tab focus; CHANGE_OWNER/MANAGE_COLLABORATORS; contact/address CRUD + disable-customer formal; reveal audit; §9/§10.1 |
| W04 | Viewport density proof; full keyboard edit path; draftVersion/contentHash auto-save; attachment audit URLs; terminate formal; SO contractRevisionId E2E; §9/§10.1 |
| W08 | W02 CompleteWorkItemEnvelope review; WorkItemActionEnvelope defer; void draft; bulk export job; full §9/viewport; 物流费/代发边界 UI |
| W09 | Q1 FULFILLMENT_TASK_MODEL_UNCONFIRMED hard blocker + single Candidate cutover; production CompleteWorkItemEnvelope; DOMAIN_OPERATION path; reverse/return formal UI; §9/viewport |
| W10 | Five-viewport + keyboard combined acceptance; server cursor pagination / Saved Views; sales-role restricted browse; live mid-session permission revoke UX |

## Recommended next_wave

**next_wave: 3**

Suggested focus order:

1. **Shared §9 / §10.1 harness** — five-viewport + state-matrix acceptance shared across specialized SPAs (unblocks many still_open lines).
2. **W04 rejected gaps** — density proof + draft edit keyboard path; contractRevisionId handoff into W05.
3. **W09 Q1 cutover honesty** — hard blocker or single Candidate; stop dual-path ambiguity in mock.
4. **W02 envelope depth** — W08 review / W09 post complete envelopes where tasks are registered.
5. **Next workspace slice** — W11–W15 finance/quality/master surfaces or remaining open W docs.

## Summary

Wave 2 integrated **49** verified checklist items across W03/W04/W08/W09/W10. **3** verify-rejected claims stayed open; several confirmed paraphrases were **not** flipped because §12 lines demand stronger end-to-end, envelope, or viewport proof, or lack a matching checkbox. Full `npm run build` **passes**. **33** open items remain in wave-2 workspace docs; **474** open across all W docs. Recommend **wave 3** for shared acceptance harness + envelope/Q1 honesty + next open workspaces.
