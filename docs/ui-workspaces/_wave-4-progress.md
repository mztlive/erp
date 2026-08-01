# Wave 4 Progress — fill-w-gaps Integrate

**Date:** 2026-08-01  
**Agent:** INTEGRATE (fill-w-gaps wave 4)  
**Workspaces:** W15, W16, W17, W18, W19

## Build status

| Check | Result |
| --- | --- |
| `cd erp-client && npm run build` | **FAIL** |
| Compile (Turbopack) | OK (~6.7s) |
| TypeScript | OK (~5.4s) |
| Static generation | **FAIL** on `/analytics/profit-loss` |

**Error:** `useSearchParams() should be wrapped in a suspense boundary at page "/analytics/profit-loss"`.  
Implement JSON already marked all five workspaces `build_ok=false`. Integrate did **not** patch app code; only docs + this report.

## Checkbox integration

| Workspace | Confirmed (verify) | Flipped `[ ]`→`[x]` | Rejected (left open) | Open remaining in §12 |
| --- | ---: | ---: | ---: | ---: |
| W15 | 11 | **10** | 1 | 6 |
| W16 | 12 | **13** | 0 | 2 |
| W17 | 11 | **7** | 1 | 18 |
| W18 | 13 | **13** | 0 | 11 |
| W19 | 12 | **11** | 0 | 14 |
| **Total** | 59 | **54** | 2 | **51** |

Rules applied:

- Only **verify.confirmed** items flipped.
- **verify.rejected** never flipped.
- Prefer **exact** checklist string match; skip confirmed paraphrases that are strict subsets of a longer §12 line or that fail full acceptance wording.
- Confirmed items with **no matching §12 line** were not invented as flips.
- One W16 confirmed claim maps to **two** §12 lines (`ACTUAL`/`REDUCTION` + `EXPECTED`/`CONFIRMED` reference-only).

### Not flipped despite partial confirm / paraphrase

| WS | Confirmed (verify) | Checklist status | Reason |
| --- | --- | --- | --- |
| W15 | 客户、逾期应收、实际盈亏分别下钻到 W03/W11/W16 | left open | §12 requires **销售构成** too: “客户、逾期应收、实际盈亏和销售构成分别下钻到正确 Wxx” — confirmed omits 销售构成 (subset) |
| W15 | 下钻在新任务页签打开，W15 筛选/滚动/焦点可 URL 恢复 | left open | **verify.rejected** — same-shell Link only; no TaskTabs identity; no full scroll restore |
| W17 | URL 子视图 view=overview\|jobs\|… + 对象 id 可恢复 | no §12 line | Shipped in page; acceptance list has no dedicated URL-restore row |
| W17 | 人工治理策略未配置时禁用立即增量/按单补拉；定时增量说明仍可读 | left open | §12 line also requires post-config 单人/双人授权强制 — still_open dual-control UI (subset) |
| W17 | 映射状态与重新归集状态分开展示；结果未知不自动完成/下一项 | left open | §12 also requires 重新归集成功 ERP 销售单/版本/应收 + 不回滚已解决映射 (subset) |
| W17 | 迁移冻结横幅：两侧主责数量 + 导向 W23/W24/W29；封存后 view=history | left open | §12 requires W24 最终同步/全量核对/硬封存路径；notes: soft history only (subset) |
| W17 | 来源快照/候选/指标/导出/历史按角色与数据范围过滤的 mock 演示 | left open | **verify.rejected** — history unfiltered; no export job UI |
| W19 | view=… URL 可恢复视图、筛选、主体与 eventId | left open | §12 also requires “返回恢复原行焦点”；focus restore not in verify.confirmed |

## Confirmed items (flipped)

### W15 (`w15-customer-business-quality.md`)

- [x] 成交金额明确含税；实际盈亏明确不含税，页面不存在混口径相减。
- [x] 卡券收入进入规模和回款，不进入 W15 实际盈亏及利润贡献标签。
- [x] 卡券票款复核进度始终与受影响应收指标同屏。
- [x] 成本覆盖收入、未覆盖收入和覆盖率同屏；缺失成本不显示为 0。
- [x] 经营标签有固定规则版本和解释，不提供人工修改入口。
- [x] 指标、图表、明细和导出使用同一期间、权限范围、口径和投影水位。
- [x] 默认期间来自服务端版本化配置；配置缺失且 URL 无 `from/to` 时首次分析要求显式选择，不静默采用自然年。
- [x] 图表筛选有选中态、摘要和结果数，并有等价数据表。
- [x] 无数据、筛选无结果、无数据范围、字段无权四种状态可区分。
- [x] 陈旧、重建、刷新失败、票款复核不足和成本覆盖不足均可见且不互相替代。

### W16 (`w16-actual-profit-loss.md`)

- [x] 页面标题、指标、图表、明细和导出均明确“非卡券 · 不含税”。
- [x] 实际盈亏只使用 `NON_VOUCHER_FULFILLMENT` 的 `ACTUAL` 和 `REDUCTION`。
- [x] `EXPECTED` 和 `CONFIRMED` 只作为对照，不进入实际盈亏或实际利润率。
- [x] 含税金额不与不含税成本混算；前端不使用浮点重算正式金额。
- [x] 卡券收入、卡券直接履约费用、消费成本和微信成本均不进入 W16。
- [x] 缺失成本显示未覆盖和原因，不生成零成本利润。
- [x] `periodBasis` 未配置时页面阻断分析查询和导出；只有服务端配置值或用户显式选择值可进入正式查询，前端没有静默默认。
- [x] 任一利润金额可下钻到销售单，并按权限查看成本事实与分配依据。
- [x] 成本事实 detail 包含来源类型、单据、明细、版本、发生时间和原成本引用。
- [x] 来源纠错后页面等待投影刷新，不直接本地覆盖金额。
- [x] 导出包含期间基准、公式版本、覆盖口径、权限范围和投影水位。
- [x] 陈旧、重建、刷新失败、部分覆盖、完全未覆盖和分母为零均有不同表现。
- [x] 汇总、图表、明细和导出采用同一数据范围；字段隐藏不通过图表比例泄露。

### W17 (`w17-mall-sync-mapping.md`)

- [x] 页面始终明确当前主责系统、同步方向及 ERP/商城各自可写边界。
- [x] 第一阶段只允许商城 → ERP 商业事实同步，ERP 不向商城回写商业修改。
- [x] 系统管理员只能补拉、重试、指派和排障，不能替业务角色确认映射。
- [x] `MappingTaskView` 按 `ownerRoutingState` 强判别：`MISSING` 不含 `ownerRole/workItem`，`CONFIGURED` 必含唯一 `ownerRole/workItem`。
- [x] 映射处理清楚展示来源事实、ERP 候选、当前谱系、业务影响和确认依据。
- [x] 页面和接口不返回玩法、卡号、卡密、绑定手机号、连接信息或接口密钥。
- [x] §9 全部状态通过组件或浏览器验证，尤其覆盖来源不可用、部分失败、冲突、结果未知和封存。

### W18 (`w18-import-opening.md`)

- [x] 任一批次都能明确看到环境、基准日、对象集、规则版本和六段阶段。
- [x] 上传成功后界面仍明确写“尚未形成正式数据”。
- [x] 问题表能按固定错误码、对象、行列和处理状态筛选，不混入成功长表。
- [x] 刷新或重新打开页签能恢复批次、阶段、问题筛选和后台进度。
- [x] 验证环境校验与业务确认是生产应用前置条件。
- [x] 期初库存只接受统一基准日实盘数量；不导入历史流水。
- [x] 卡券草稿不迁移，期初已收和已开票为 0，后续进入 W13。
- [x] 部分成功正确区分成功、跳过和失败，并支持幂等修复批次。
- [x] 原始 SQL、数据库连接头和禁止字段不会进入长期 `file_asset` 或普通页面。
- [x] 成功资产与失败诊断资产分离，分别执行长期/30 天保留规则；导出 7 天到期。
- [x] 业务确认人只能确认本人责任范围，系统管理员不能代替业务确认。
- [x] 业务确认入口上线前，导入确认的固定 `work_item_type` 已写入权威数据模型并与 W01/W02 展示映射一致；未登记时必须保持实施 blocker。
- [x] 试算或规则变化使旧确认失效，并阻止按旧版本应用。

### W19 (`w19-permissions-audit.md`)

- [x] 页面明确分开模块/动作权限、数据范围、字段权限和对象状态 blocker。
- [x] 无模块权限、无数据范围、范围内无记录和字段掩码四种状态不会混淆。
- [x] 有效权限解释能指出授权或阻塞来源，不由前端合并权限集合。
- [x] 用户角色时间、字段粒度和审计访问/导出策略均展示服务端配置态；缺失时分别执行规定的 fail-closed 行为，不由前端猜默认值。
- [x] 所有授权变更提交前展示变化、影响主体和服务端风险摘要。
- [x] 正式结果包含配置版本、影响数量、审计事件号和下一步。
- [x] Q1 决策前，命中复核要求的动作失败关闭，W19 不创建、领取、移动确认或完成 `work_item`。
- [x] 用户角色时间策略未配置时，只有 `EmergencyRevokeUserRoleCommand` 可提交且立即生效；其它分配/变更阻断，页面没有预约/到期编辑控件。
- [x] 字段粒度策略未配置时字段策略只读；配置后只可提交服务端 `policyTargetId` 与策略版本，不以 `fieldGroup` 或任意字段路径充当写契约。
- [x] 审计可按操作者、角色、动作、对象、结果、请求追踪号和时间查询。
- [x] 敏感字段只显示字段名和“已变更”，不显示完整旧值或新值。

## Files changed (from implement JSON; not re-touched by Integrate except docs)

### W15

```
erp-client/features/customer-quality/{types,api,queries,customer-quality-page}.ts(x)
erp-client/mock/customer-quality.ts
erp-client/app/(workspace)/analytics/customer-quality/page.tsx
```

### W16

```
erp-client/app/(workspace)/analytics/profit-loss/page.tsx
erp-client/features/actual-profit-loss/{types,api,queries,actual-profit-loss-page}.ts(x)
erp-client/mock/actual-profit-loss.ts
```

### W17

```
erp-client/features/mall-sync/{types,api,queries,mall-sync-page}.ts(x)
erp-client/mock/mall-sync.ts
erp-client/app/(workspace)/governance/mall-sync/page.tsx
erp-client/mock/work-items.ts
```

### W18

```
erp-client/app/(workspace)/governance/imports/page.tsx
erp-client/features/import-opening/{types,url-state,api,queries,import-opening-page}.ts(x)
erp-client/mock/import-opening.ts
```

### W19

```
erp-client/features/access-audit/{types,session,api,queries,access-audit-page}.ts(x)
erp-client/app/(workspace)/system/access-audit/page.tsx
```

**Docs touched by Integrate:**

- `docs/ui-workspaces/w15-customer-business-quality.md`
- `docs/ui-workspaces/w16-actual-profit-loss.md`
- `docs/ui-workspaces/w17-mall-sync-mapping.md`
- `docs/ui-workspaces/w18-import-opening.md`
- `docs/ui-workspaces/w19-permissions-audit.md`
- `docs/ui-workspaces/_wave-4-progress.md` (this file)

## Counts summary

| Metric | Value |
| --- | ---: |
| verify.confirmed | 59 |
| checkboxes_flipped | **54** |
| verify.rejected | 2 |
| Wave-4 §12 open remaining | **51** |
| All `docs/ui-workspaces/w*.md` open `- [ ]` | **389** |
| build_ok | **false** |

### High-priority leftovers (wave 4)

| WS | Theme |
| --- | --- |
| **Build** | Wrap W16 profit-loss page `useSearchParams` in `<Suspense>` so `next build` passes |
| W15 | ≤2-click metric→object; multi-role same-scope demo; TaskTabs `analytics:customer-quality:{scopeId}`; drill TaskTabs + 销售构成; five-viewport + a11y; export reauth 7-day |
| W16 | Five-viewport + full a11y path; multi-role field-permission matrix beyond `fieldHide` |
| W17 | Dual-control TriggerMallSync; RequestSourceFix/Transfer envelopes; freeze final sync; late/A→B→A evidence; role-filtered export; stronger reapply-success / freeze §12 lines |
| W18 | Create-batch + full idempotent pipeline; CompleteWorkItemEnvelope confirm (after work_item_type register); download audit; UNKNOWN apply result; five-viewport/a11y |
| W19 | Export BackgroundJob + audit; mid-session revoke scrub; ConflictResolutionDialog; cross-W permissionVersion invalidation; time-policy schedule form; role-disable pool detail; URL+focus; five-viewport/keyboard |

## Recommended next_wave

**next_wave: 5**

Suggested focus order:

1. **Unblock build** — Suspense boundary on `/analytics/profit-loss` (and audit any other useSearchParams pages).
2. **Shared §9 / §10 harness** — five-viewport + keyboard/a11y acceptance (unblocks W15–W19 combined lines and earlier waves).
3. **W17 honesty** — either ship dual-control + freeze final-sync + export filter, or keep rejected/open and stop over-claiming.
4. **W15 TaskTabs + 2-click** — shell identity + metric drill completeness + 销售构成 drill.
5. **W18/W19 formal envelopes** — work_item_type registration path vs honest blocker; export jobs; conflict re-submit.
6. **Next workspace slice** — W20+ supplier API / supply / publication, or remaining open W docs.

## Summary

Wave 4 integrated **54** verified checklist items across W15–W19 specialized session-mock SPAs (customer quality, actual P/L, mall sync, import opening, access audit). **2** verify-rejected claims stayed open (W15 TaskTabs drill; W17 role-filtered export/history). Several confirmed paraphrases were **not** flipped because §12 lines demand stronger dual-control/freeze/sales-composition/focus proof, or lack a matching checkbox. Full `npm run build` **fails** on W16 profit-loss missing Suspense. **51** open items remain in wave-4 workspace docs; **389** open across all W docs. Recommend **wave 5** for build fix + shared acceptance harness + W15/W17 honesty + next open workspaces.
