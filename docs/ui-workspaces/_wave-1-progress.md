# Wave 1 Progress — fill-w-gaps Integrate

**Date:** 2026-08-01  
**Agent:** INTEGRATE (fill-w-gaps wave 1)  
**Workspaces:** W01, W02, W05, W06, W07

## Build status

| Check | Result |
| --- | --- |
| `cd erp-client && npm run build` | **FAIL** |
| Compile (Turbopack) | OK (~5.9s) |
| TypeScript | **Failed** |

**Error (blocking, outside wave-1 surface):**

```
./features/fulfillment-operations/fulfillment-operations-page.tsx:437:47
Type error: Argument of type '{ scope: string; ... }' is not assignable to parameter of type 'FulfillmentQueueFilters'.
  Types of property 'scope' are incompatible.
    Type 'string' is not assignable to type '"mine" | "role_pool"'.
```

Wave-1 implementers reported mixed per-workspace `build_ok`; full-repo build is currently blocked by W09 fulfillment queue filter typing, not by W01/W02/W05/W06/W07 alone.

## Checkbox integration

| Workspace | Confirmed (verify) | Flipped `[ ]`→`[x]` | Rejected (left open) | Open remaining in §12 |
| --- | ---: | ---: | ---: | ---: |
| W01 | 10 | **10** | 2 | 7 |
| W02 | 9 | **8** | 3 | 8 |
| W05 | 13 | **8** | 0 | 16 |
| W06 | 9 | **8** | 2 | 8 |
| W07 | 11 | **10** | 0 | 9 |
| **Total** | 52 | **44** | 7 | **48** |

Rules applied:

- Only **verify.confirmed** items flipped.
- **verify.rejected** never flipped.
- Prefer **exact** checklist string match; skip confirmed paraphrases that are strict subsets of a longer §12 line or that fail full acceptance wording.
- W05 history line uses doc wording **关键快照** (verify said 金额快照); same checklist row, flipped after exact-line match on doc text.

### Not flipped despite partial confirm / paraphrase

| WS | Confirmed (verify) | Checklist status | Reason |
| --- | --- | --- | --- |
| W02 | 第 9 节关键状态（初载/无待办/…） | `- [ ]` 第 9 节**全部**状态… | Key states ≠ full §9 matrix; notes still open list-fail/summary-partition/claimed-by-other |
| W02 | （implement claimed）同类通过/驳回后下一项；完成同事务；五档视口 | left open | **verify.rejected** |
| W05 | 正式销售单无直接编辑…必须通过销售变更单 | longer line + 影响确认/财务复核 | Only SCO start + session mock; full impact/finance review not confirmed |
| W05 | CARD 审批处理器形态 | 共用处理器和完成信封… | Panel/claim demo only; envelope/W02 share not exact match |
| W05 | 1440 列表筛选/固定列/detail | 至少 6–8 行 + 首屏 | Row-count / same-screen not verified |
| W05 | 正式操作确认 + 导出权限版本 | 所有操作权限/前置/…；查询对象导出… | Partial FormalActionResult/export only |
| W05 | TaskTab 单焦点 | 关闭后恢复来源焦点 | Path-level single tab only; restore-on-close open |
| W06 | 刷新/后退/W09 恢复草稿 | left open | **verify.rejected** (in-memory draft) |
| W06 | 权限收回清理敏感快照 | left open | **verify.rejected** (header/React state incomplete) |
| W06 | Q2 workItem fail-closed（巩固） | already `[x]` | No change |
| W07 | 1440/响应式骨架与暂挂保留队列 | no single matching line | 同屏 item flipped separately; 五档 + full envelope line not confirmed |
| W01 | 筛选 URL + 浏览器后退；1440 同屏 | left open | **verify.rejected** |

## Confirmed items (flipped)

### W01 (`w01-today-workspace.md`)

- [x] 销售用户登录后，一次点击可从首条任务进入处理面。
- [x] 任一有任务角色从着陆到处理第一条待办不超过两次点击。
- [x] 指标和列表使用同一权限/数据范围版本，数量不由前端当前页求和。
- [x] 正式待办与投影指标分别展示正确的新鲜度。
- [x] 投影超过 1 分钟时明确标记陈旧，不宣称实时。
- [x] 无模块权限、无数据范围、无任务和筛选无结果四种状态可区分。
- [x] 服务端返回固定 `work_item_type`，前端只做展示分组。
- [x] 从目标页返回后恢复原筛选和任务焦点。
- [x] 键盘可完成筛选、折叠分组、打开任务和返回。
- [x] 读屏器能够识别指标选中态、分组开合、任务状态和结果数量变化。

### W02 (`w02-unified-task-queue.md`)

- [x] 从 W01 任务条目到 W02 当前处理器不超过一次点击。
- [x] 全部类型视图可找任务，正式处理时收敛到单类型或兼容处理器组。
- [x] 查询、重放、保存证据和暂挂等任务内动作使用 `WorkItemActionEnvelope`…
- [x] 审批、确认、结果未知和补偿任务无人工关闭入口。
- [x] 权限收回后当前处理器不残留敏感快照或租约令牌。
- [x] 网络超时不自动跳到下一项，可用同一幂等键查询最终结果。
- [x] 租约丢失和版本冲突都保留本地输入但阻止提交。
- [x] 仅用键盘可完成筛选、领取、打开对象、做决定和继续下一项。

### W05 (`w05-sales-orders.md`)

- [x] 卡券与非卡券在同一列表、对象中心、编号和版本体系，业务性质创建后不可修改。
- [x] 创建来源与当前主责分列；任一时点只有一个写入主责。
- [x] 一期商城主责卡券商业字段只读；二期迁移只改主责、不换身份、单号或销售版本。
- [x] 每个卡券销售版本恰好一条卡券明细，且页面不出现玩法、卡号、卡密或手机号。
- [x] 非卡券以验收完成履约；卡券以履约期限到期完成，不因已消费完提前完成。
- [x] 履约完成且应收结清才能关闭；开票未完成不阻塞关闭。
- [x] 历史销售版本保存精确合同/主数据修订和关键快照，不被当前值覆盖。
- [x] 采购驳回后页面只提供改品/改价重提、照原条件申请低毛利承接、不做并作废三条固定出路；不存在通用重提或恢复旧 W07 任务入口。

### W06 (`w06-customer-acceptance.md`)

- [x] 一次验收可分配多个履约批次，同一履约批次可被多次验收。
- [x] 1440×900 下销售单身份、至少两条履约来源、本次验收摘要和主动作同屏可见。
- [x] 短少、拒收和服务不通过结果明确说明“仅记录验收事实”，不暗示库存/票款已处理。
- [x] 页面所有字段能追溯到销售版本、履约事实、验收事实、正式投影或权限结果。
- [x] 可验收量和履约完成采用服务端净事实，前端不按表头状态推断。
- [x] 已过账验收不可编辑，误录通过新反向事实与 `REVERSE` 分配纠正。
- [x] 正式成功固定展示验收单号；超时不会误报成功或重复过账。
- [x] 键盘可完成来源选择、数量填写、保存和正式确认；读屏能听到错误与结果。

### W07 (`w07-procurement-confirmation-queue.md`)

- [x] 1440×900 下队列位置、不可变销售提交、至少两条确认分行、覆盖摘要和主动作同屏可见。
- [x] 多供应商拆分能逐明细说明确认数量，不能用总量掩盖单行缺口。
- [x] 打开 W05 深挖后返回仍恢复队列位置、筛选、当前项和显式 URL / 当前会话的自动下一项临时值；`preferenceScope` 未配置时不产生本地或服务端持久偏好。
- [x] 全部确认引用具体 `submissionId` 和 `subjectHash`，不读取可变销售草稿。
- [x] 驳回形成本次采购确认的正式 `REJECTED` 结论并完成当前任务…固定结果完整展示销售三条出路。
- [x] 查询 View 不返回 `claimToken`；仅领取/续租 mutation 返回令牌，且只存在会话内存。
- [x] 重复点击和超时重试不重复推进销售状态或生成多个采购创建依据。
- [x] 正式动作结果不确定时停留当前项，不自动下一项。
- [x] 键盘可完成分行编辑、保存、驳回/通过和连续切换。
- [x] 读屏可识别队列位置、租约变化、行覆盖、错误与固定结果。

## Rejected items (not flipped)

### W01

1. **筛选进入 URL，刷新和浏览器后退能恢复。** — Filters write via `router.replace`, so Back does not restore prior filter stack.
2. **1440×900 同屏 + pagePreviewLimit。** — Layout intent only; no viewport measurement / acceptance proof.

### W02

1. **同类任务通过、驳回或暂挂后可直接处理下一项。** — DEFER advances; 通过 often leaves W02 via `handlerHref`; 驳回 is theater.
2. **任务完成与业务事实变化同一事务，无独立“标记完成”。** — Domain COMPLETE navigates out; mock synthesizes text only.
3. **五档视口 §10.1。** — Structural Tailwind only; no pixel verification; 375 not degraded.

### W05

- None rejected by verify (several confirmed paraphrases skipped as incomplete vs longer §12 lines — see table above).

### W06

1. **刷新、后退和从 W09 返回均恢复销售单、子区与草稿…** — Draft is in-memory only; hard refresh drops; W09 return not wired.
2. **权限收回后客户、地址、附件和本地敏感快照被清理。** — Parent header / React form state incomplete.

### W07

- None rejected by verify.

## Files changed (implement wave 1, unique)

```
erp-client/app/(workspace)/procurement/confirm/page.tsx
erp-client/app/(workspace)/sales/orders/[salesOrderId]/page.tsx
erp-client/app/(workspace)/workspace/page.tsx
erp-client/app/(workspace)/workspace/tasks/page.tsx
erp-client/components/layout/workspace-shell.tsx
erp-client/features/procurement-confirmation/{api,queries,types,procurement-confirmation-page}.ts(x)
erp-client/features/sales-orders/{types,build-order,close-eligibility,api,queries,acceptance,acceptance-types,acceptance-workspace,filter-orders,close-conditions-card,revision-history-card,procurement-rejection-card,card-sales-approval-panel,sales-order-detail-page,sales-orders-list-page,sales-order-preview-panel}
erp-client/features/unified-task-queue/{filter-work-items,queries,queue-url,types,unified-task-queue-page}
erp-client/features/workspace/{destination,freshness,queries,url-state,workspace-home-page}
erp-client/mock/{acceptance-fulfillment,procurement-confirmation,sales-orders,session-state,work-items,workspace,workspace-pages}
```

**Docs touched by Integrate:**

- `docs/ui-workspaces/w01-today-workspace.md`
- `docs/ui-workspaces/w02-unified-task-queue.md`
- `docs/ui-workspaces/w05-sales-orders.md`
- `docs/ui-workspaces/w06-customer-acceptance.md`
- `docs/ui-workspaces/w07-procurement-confirmation-queue.md`
- `docs/ui-workspaces/_wave-1-progress.md` (this file)

## Remaining open counts

| Scope | Open `- [ ]` |
| --- | ---: |
| Wave 1 workspaces (W01/W02/W05/W06/W07) | **48** |
| All `docs/ui-workspaces/w*.md` | **523** |

### High-priority leftovers (wave 1)

| WS | Theme |
| --- | --- |
| W01 | Filter history.push (browser back), TaskTabs focus-or-reuse, live permission revoke scrub, §9/§10 matrix |
| W02 | Continuous same-type after approve/reject, true complete-with-business-fact, multi-user lease, SoD, full §9/§10 |
| W05 | Sales-change full editor + impact/finance, card approval envelope + RESULT_UNKNOWN, TaskTab restore, export server job, low-margin E2E, viewports |
| W06 | Durable draft storage, revoke scrub completeness, W09 return context, CompleteWorkItem when registered, §9/§10 |
| W07 | Server revalidation of vendor/cost, real sales effect / receivable / creation basis, W05→new W07 atomic paths, preferenceScope persistence, full envelope UI, §9/§10 |
| Cross | Unblock `npm run build` (fulfillment-operations `scope` typing) |

## Recommended next_wave

**next_wave: 2**

Suggested focus order:

1. **Unblock build** — fix `FulfillmentQueueFilters.scope` typing in fulfillment-operations (W09 path).
2. **W01 filter history** — `router.push` (or stacked history) for metric/filter changes so browser Back restores filters.
3. **W02 continuous processing honesty** — approve/reject stay-in-queue or document specialized handler split; no fake 驳回 theater.
4. **Shared platform gaps** — TaskTabs identity/focus-or-reuse; durable draft + permission scrub patterns; five-viewport / §9 matrix harness.
5. **Next workspace slice** — continue fill-w-gaps for remaining open W docs (e.g. W03/W04/W08/W09) once wave-1 blockers above are scheduled.

## Summary

Wave 1 integrated **44** verified checklist items across W01/W02/W05/W06/W07. **7** verify-rejected claims stayed open; several additional confirmed paraphrases were **not** flipped because §12 lines demand stronger end-to-end or viewport proof. Full `npm run build` still **fails** on an unrelated fulfillment-operations type error. **~48** open items remain in wave-1 workspace docs; **~523** open across all W docs. Recommend **wave 2** start with build unblock + W01 history.push + W02 continuous-process honesty.
