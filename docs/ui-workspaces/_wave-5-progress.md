# Wave 5 Progress — fill-w-gaps Integrate

**Date:** 2026-08-01  
**Agent:** INTEGRATE (fill-w-gaps wave 5)  
**Workspaces:** W20, W21, W22, W23, W24

## Build status

| Check | Result |
| --- | --- |
| `cd erp-client && npm run build` | **OK** |
| Compile (Turbopack) | OK (~8.2s) |
| TypeScript | OK (~6.1s) |
| Static generation | OK (33 routes) |

Implement JSON had per-workspace `build_ok=false` for W20/W21/W22/W24 (claimed blocked by other waves) and `build_ok=true` for W23. **Full-repo `npm run build` succeeds** on this integrate pass (prior wave-4 profit-loss Suspense issue no longer blocking).

## Checkbox integration

| Workspace | Confirmed (verify) | Flipped `[ ]`→`[x]` | Rejected (left open) | Open remaining in §12 |
| --- | ---: | ---: | ---: | ---: |
| W20 | 12 | **10** | 8 | 10 |
| W21 | 13 | **9** | 0 | 14 |
| W22 | 12 | **15** | 0 | 14 |
| W23 | 12 | **14** | 0 | 8 |
| W24 | 12 | **17** | 0 | 5 |
| **Total** | 61 | **65** | 8 | **51** |

Rules applied:

- Only **verify.confirmed** items flipped.
- **verify.rejected** never flipped.
- Prefer **exact** checklist string match; skip confirmed paraphrases that are strict subsets of a longer §12 line or that fail full acceptance wording.
- Confirmed items with **no matching §12 line** were not invented as flips.
- Several confirmed claims map to **multiple** §12 lines (e.g. W22 MOQ + sales-price independence; W23 ESCALATE / W29 / no-lease; W24 baseline + atomic fail + T immutability).

### Not flipped despite partial confirm / paraphrase

| WS | Confirmed (verify) | Checklist status | Reason |
| --- | --- | --- | --- |
| W20 | 1440×900 首屏 6–8 有效行；连接身份列与主动作列固定 | left open | **verify.rejected** — default PRODUCTION filter yields ~5 rows; no viewport measurement |
| W20 | 连接身份列与主动作列固定 (pinning only) | left open | Compound §12 line still requires proven 6–8 first-screen rows |
| W20 | §9 空/无权/无范围/筛选无结果等状态可区分 | left open | §12 requires **full** §9 matrix; conflict dialog / revoke / stale-cache still open (**verify.rejected** full matrix) |
| W20 | 密钥只显示绑定/别名/版本… | partial | Flipped opaque Select line; broader “页面/URL/审计/**导出**/缓存均不出现密钥” left open (CSV export rejected) |
| W20 | — | rejected rows | 冲突 diff Dialog、双角色启用、CSV 脱敏、跨模块不直连 API、全仓 build 宣称 — not flipped |
| W21 | 正常映射/供给未登记时无正式写按钮且不可聚焦 | left open | Subset of longer 登记后 W02 原子完成 / 键盘全路径 lines; post-registration still_open |
| W21 | STOPPED 安全暂停 + RECOVERY_RESPONSIBILITY_UNCONFIRMED | left open | Subset of compound 无人领取/租约失败 + 恢复链 line |
| W21 | 价格/税率/费用字段权限掩码 | left open | §12 also requires 提示/审计/导出/缓存不泄露 |
| W21 | 对象中心 M4 路由 | no §12 line | Shipped center route; acceptance list has no dedicated M4 checkbox |
| W22 | SystemSafetyPause **view** variants | partial | Flipped `SystemSafetyPauseOperationView` structure; atomic outbox/task-same-tx + paid-order drill left open |
| W23 | 只读边界 Alert … 变更须走销售变更单 | partial | Flipped 接收失败不回退; “形成新销售版本后**自动产生新投影**” still open |
| W23 | fieldPermissions returned | not flipped | Notes: returned but unused by WhitelistContentGrid |
| W24 | 总览批次表列 + 筛选 | partial | Flipped 单客户批次 scope rule; no separate §12 “总览表列” row |
| W24 | 手机隐藏高风险执行动作 | left open | Compound with 五档视口 §10 line; only mobile hide proven |

## Confirmed items (flipped)

### W20 (`w20-supplier-api-connections.md`)

- [x] 列表能在一行看清连接代码、供应商、环境、状态、能力、健康和下一步。
- [x] 连接中心在一屏级内容内解释业务身份、技术就绪、能力、健康和目录水位。
- [x] URL/TaskTabs 可恢复连接、子区和筛选；重复打开相同连接不创建副本。
- [x] 采购、研发运维和系统管理员的业务/技术/治理动作明确分离。
- [x] 能力声明不会被展示成每个商品都可用；商品级能力进入 W21/W22。
- [x] 停用连接前显示发布、订单和同步影响，不删除历史版本或业务快照。
- [x] 绑定/轮换只接受密钥管理系统的不透明引用；无明文输入兜底路径。
- [x] 鉴权/签名失败停止自动重试并显著告警。
- [x] 健康检查与目录同步显示后台任务号和固定结果，不把请求返回当作任务完成。
- [x] 生产环境、故障、结果未知和引用状态均有文字与读屏语义。

### W21 (`w21-external-product-supply.md`)

- [x] 已注册来源 `ERROR` 和 `STOPPED` 异常可以连续处理；新增、映射、正常供给复核及其它安全暂停原因在类型登记前可连续浏览但始终显示 fail-closed blocker。
- [x] 1440×900 下外部身份、关键 diff、SKU 映射、供给摘要、发布影响和决策同屏可见。
- [x] 队列筛选、位置、当前项和自动下一项可刷新恢复；打开 W14/W22 后队列上下文不丢。
- [x] 任务内动作成功后仍显示 `PENDING` / `IN_PROGRESS` 且不自动下一项；异常终结成功先显示固定结果，再按偏好或由用户继续。
- [x] 外部修订先暂存，未经审核不直接修改 ERP SKU 或商城商品。
- [x] 同一外部商品同一时点只有一个有效 SKU 映射；一个 SKU 可有多个外部供给。
- [x] 供货价和关键供给变化形成不可变新修订，不覆盖旧版本。
- [x] 采购负责映射和供给；运营负责 W22 发布；管理员/运维只处理技术异常。
- [x] 供货价变化不自动修改商城销售价；`minimumOrderQuantity` 不自动复制为商城最小购买量。

### W22 (`w22-product-publication.md`)

- [x] 运营能在列表一次筛选出待商城确认、失败、转人工和已暂停发布。
- [x] 1440×900 下列表露出 6–8 条有效数据行，SKU 身份列和操作列固定。
- [x] 对象中心一屏可同时识别稳定发布、当前商城生效版本和最新待确认版本。
- [x] 选中任一历史修订可看到当时完整发布内容、媒体、唯一固定供给和投递结果。
- [x] 每个发布修订恰好绑定一条固定供给修订。
- [x] 最小购买量不会从供应商最小订购量自动复制。
- [x] 供货价变化不会自动修改商城销售价。
- [x] 图片、固定供给、价格或销售状态变化均形成新修订，不覆盖历史。
- [x] 发布工作副本策略未确认时只有 TaskTab 会话内编辑；无草稿保存 mutation、无自动保存/本地持久化，刷新或关闭前明确提示输入将丢失。
- [x] 多商城/唯一性规则未确认时，列表固定返回 `PUBLICATION_IDENTITY_POLICY_UNCONFIRMED` 并禁止新建；已有对象仍可按服务端 `publicationId` 查看/维护。
- [x] 提交发布形成不可变修订和 outbox，正式结果固定显示版本号与投递编号。
- [x] `SystemSafetyPauseOperationView` 是列表/对象/操作结果的唯一结构：`SUPPLIER_STOPPED + COMMITTED/ALREADY_SAFE` 强制唯一 `followUpWorkItem`，其它已落库原因强制唯一 `followUpBlocker`，`UNKNOWN` 二者均禁止且保持 fail-closed。
- [x] 销售价/销项税率变化且复核政策未配置时，`PUBLISH` 固定被 `REVIEW_POLICY_UNCONFIGURED` 阻断；无变化也只使用服务端 `publishGate` 结论。
- [x] 恢复责任未确认时，任何安全暂停到 `ON_SALE` 的提交都被 `RECOVERY_RESPONSIBILITY_UNCONFIRMED` 阻断；只允许安全暂停或人工暂停。
- [x] 商城成功确认前不显示为“商城已生效”。

### W23 (`w23-execution-projection.md`)

- [x] 用户在 W05 协同子区即可看懂单张销售单当前投影和商城接收状态，无需先进入 W23。
- [x] W23 能一次筛选出结果未知、失败、转人工、超过 SLA 和版本差异项。
- [x] 1440×900 下列表露出 6–8 条有效数据行，销售单身份列和操作列固定。
- [x] 对象中心一屏区分销售事实、投影投递和商城确认三条状态轨。
- [x] 历史投影始终显示其来源销售版本，不被 W05 当前版本覆盖。
- [x] 投影字段只来自已生效销售版本的服务端投影修订，前端不重新组装。
- [x] 成交金额、配赠、税率、开票、应收和玩法规则在任何角色下都不进入投影内容。
- [x] 接收失败不会回退销售版本、应收或旧版已完成执行事实。
- [x] 结果未知先查询，未明确前不显示成功、不跳过、不进入已确认统计。
- [x] 批量动作使用服务端选择快照，逐项重验权限、状态和版本。
- [x] 对账差异只创建 / 打开 W29 任务，不覆盖 ERP 或商城事实。
- [x] 正式结果固定显示操作编号、对象、时间和下一步，不只用 toast。
- [x] W01/W02 正式错误待办统一打开 W29；W23 不接收任务租约，也不领取、移动确认或完成任务。
- [x] `ESCALATE` 幂等返回既有或新建的 `workItemId` / `errorTaskId`，后续处理只在 W29 发生。

### W24 (`w24-ownership-migration.md`)

- [x] 迁移范围只包含已生效及之后、未作废的正式存量卡券销售单。
- [x] 商城草稿明确显示为不迁移、不补建，不会进入批次项目或统计。
- [x] 每个批次只含一个客户；同批所有项目属于该客户。
- [x] 销售、财务和最终基线三类确认由各自责任角色独立完成，管理员不能代签。
- [x] `scopeHash` 或相应分面变化会使旧确认失效，并保留审计。
- [x] 冻结期间所有受影响工作面显示不可忽略的维护 Banner。
- [x] 最终基线确认只在冻结、最后同步和全量核对后可用。
- [x] 基线登记不生成新销售版本；迁移执行基线明确标注为第一份投影修订。
- [x] 迁移成功只改变主责标记，不换单号、不复制销售单、不改变应收、回款和发票。
- [x] 批次任一项失败时全批未提交，界面不出现部分成功语义。
- [x] 失败保持冻结并使用原批次续跑；其它完成批次不回退。
- [x] 全部目标客户批次完成、一期轮询封存和固定检查链尾全部通过前，无法登记 `T`。
- [x] 旧、失败、过期或已被替代的检查证据不能被当成当前通过。
- [x] `T` 以商城为粒度原子登记，结果未知时先查询，不创建第二个切换。
- [x] `T` 一经登记不可修改或删除。
- [x] 无模块权限、无客户范围、无批次、筛选无结果、确认失效和字段掩码可区分。
- [x] 执行进度不会被表达为正式项目成功数。

## Files changed (from implement JSON; not re-touched by Integrate except docs)

### W20

```
erp-client/features/supplier-api-connections/{types,url-state,api,queries,supplier-api-connections-page}.ts(x)
erp-client/mock/supplier-api-connections.ts
erp-client/app/(workspace)/supplier-api/connections/page.tsx
erp-client/app/(workspace)/supplier-api/connections/[connectionId]/page.tsx
erp-client/components/layout/workspace-shell.tsx
erp-client/app/(workspace)/layout.tsx
```

### W21

```
erp-client/features/external-product-supply/{types,api,queries,external-product-supply-page,external-product-center-page}.ts(x)
erp-client/mock/external-product-supply.ts
erp-client/app/(workspace)/supplier-api/catalog/page.tsx
erp-client/app/(workspace)/supplier-api/catalog/[externalProductId]/page.tsx
```

### W22

```
erp-client/app/(workspace)/commerce/publications/page.tsx
erp-client/app/(workspace)/commerce/publications/[publicationId]/page.tsx
erp-client/features/product-publications/{types,api,queries,product-publications-list-page,publication-center-page,safety-pause-panel}.ts(x)
erp-client/mock/product-publications.ts
erp-client/mock/product-publications-session.ts
```

### W23

```
erp-client/app/(workspace)/commerce/execution-projections/page.tsx
erp-client/features/execution-projections/{types,api,queries,execution-projections-page,collaboration-card}.ts(x)
erp-client/mock/execution-projections.ts
erp-client/features/sales-orders/sales-order-detail-page.tsx
```

### W24

```
erp-client/app/(workspace)/governance/ownership-migrations/page.tsx
erp-client/components/layout/workspace-shell.tsx
erp-client/features/ownership-migration/{api,global-freeze-banner,ownership-migration-page,queries,types,url-state}.ts(x)
erp-client/mock/ownership-migration.ts
```

**Docs touched by Integrate:**

- `docs/ui-workspaces/w20-supplier-api-connections.md`
- `docs/ui-workspaces/w21-external-product-supply.md`
- `docs/ui-workspaces/w22-product-publication.md`
- `docs/ui-workspaces/w23-execution-projection.md`
- `docs/ui-workspaces/w24-ownership-migration.md`
- `docs/ui-workspaces/_wave-5-progress.md` (this file)

## Counts summary

| Metric | Value |
| --- | ---: |
| verify.confirmed | 61 |
| checkboxes_flipped | **65** |
| verify.rejected | 8 |
| Wave-5 §12 open remaining | **51** |
| All `docs/ui-workspaces/w*.md` open `- [ ]` | **324** |
| build_ok | **true** |

### High-priority leftovers (wave 5)

| WS | Theme |
| --- | --- |
| W20 | ConflictResolutionDialog field-diff; dual-role enable; §9/§10 browser matrix; CSV redacted export; cross-module API guard scan; density default filter vs 6–8 rows |
| W21 | Full W02 envelopes + registered work_item_type; dirty leave guard; ConflictResolutionDialog; QUERY_ORIGINAL_RESULT/SAVE_EVIDENCE UI; mode=list; recovery compound line; cost mask export/cache proof |
| W22 | Server validation matrix; paid-order revision drill; safety-pause atomic/tx; bulk retry selection snapshot; permission-revoke scrub; §9/§10; W29 deep-link; mall ACK demo timing |
| W23 | §9/§10/keyboard matrix; role data-scope demos; permission-revoke scrub; 1:1 revision write-path enforcement UI; auto new projection after W05 change |
| W24 | Real W02 claim/lease/idempotency confirm UI; nested `:batchId` + `/cutover` routes; T-before ledger vs T-after fulfill cross-surface; permission-revoke; §9/§10 |

## Recommended next_wave

**next_wave: 6**

Suggested focus order:

1. **W25–W30 slice** — mall consumption, supplier orders, API settlement, card analytics, integration errors, history backfill (remaining specialized workspaces).
2. **Shared §9 / §10 harness** — five-viewport + keyboard acceptance across open residual lines (W20–W24 and earlier waves).
3. **W20 honesty** — ConflictResolutionDialog + dual-role enable or keep open; fix list default filter if 6–8 density is required.
4. **W21/W22 formal registration** — work_item_type / recovery responsibility / publish policy when product decisions land.
5. **W24 W02 envelopes** — sales/finance confirm via real task envelopes vs honest blocker copy.
6. **Cross-wave residual** — permission-revoke scrub patterns; export jobs; conflict re-submit dialogs.

## Summary

Wave 5 integrated **65** verified checklist items across W20–W24 specialized session-mock SPAs (supplier API connections, external product supply queue+center, product publications, execution projections with W05 collaboration card, ownership migration + global freeze banner). **8** verify-rejected W20 claims stayed open (viewport density, full §9 matrix, conflict dialog, dual-role enable, CSV export, cross-module guards, build claim). Several confirmed paraphrases were **not** flipped because §12 lines demand stronger envelope/atomic/export/viewport proof, or lack a matching checkbox. Full `npm run build` **passes**. **51** open items remain in wave-5 workspace docs; **324** open across all W docs. Recommend **wave 6** for W25–W30 + shared acceptance residual.
