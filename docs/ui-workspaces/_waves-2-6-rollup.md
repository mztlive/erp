# Waves 2–6 Rollup — fill-w-gaps-all

**Date:** 2026-08-01  
**Source:** per-wave integrate progress (`_wave-2-progress.md` … `_wave-6-progress.md`) + live §12 checkbox counts in `w*.md`  
**Scope:** W03–W04, W08–W30 (waves 2–6 only; wave 1 excluded)

## Executive summary

| Metric | Value |
| --- | ---: |
| Waves completed | **2, 3, 4, 5, 6** (5 waves) |
| Workspaces in scope | **25** |
| Checkboxes flipped this pass (sum of integrate flips) | **280** |
| Live `[x]` (done) in scope | **295** |
| Live `[ ]` (open remaining) in scope | **195** |
| Live total §12 rows in scope | **490** |
| Full-repo build after last wave | **OK** (wave 4 had FAIL; waves 5–6 re-verified OK) |

Integrate rule (all waves): only **verify.confirmed** items flipped; **verify.rejected** left open; no invented §12 rows.

## Per-wave integrate summary

| Wave | Workspaces | Implementers OK | Verifiers OK | Flipped `[ ]`→`[x]` | Open remaining (§12) | `npm run build` |
| ---: | --- | ---: | ---: | ---: | ---: | --- |
| 2 | W03, W04, W08, W09, W10 | 5 | 5 | **49** | **33** | OK |
| 3 | W11, W12, W13, W14 | 4 | 4 | **31** | **15** | OK |
| 4 | W15, W16, W17, W18, W19 | 5 | 5 | **54** | **51** | **FAIL**† |
| 5 | W20, W21, W22, W23, W24 | 5 | 5 | **65** | **51** | OK |
| 6 | W25, W26, W27, W28, W29, W30 | 6 | 6 | **81** | **45** | OK |
| **Total** | **25 WS** | **25** | **25** | **280** | **195** | — |

† Wave 4: static generation failed on `/analytics/profit-loss` (`useSearchParams()` needs Suspense). Integrate flipped docs only. Wave 5 integrate reported the prior profit-loss Suspense issue no longer blocking full-repo build.

## Live checkbox inventory (current `w*.md`)

Counts are live `- [x]` / `- [ ]` under each workspace acceptance list.

### Wave 2 — CRM / PO / fulfillment / inventory

| Workspace | File | Done | Open | Total |
| --- | --- | ---: | ---: | ---: |
| W03 | `w03-customer-center.md` | 12 | 5 | 17 |
| W04 | `w04-contracts.md` | 10 | 7 | 17 |
| W08 | `w08-purchase-orders.md` | 10 | 8 | 18 |
| W09 | `w09-fulfillment-operations.md` | 11 | 12 | 23 |
| W10 | `w10-inventory-ledger.md` | 9 | 1 | 10 |
| **Wave 2** | | **52** | **33** | **85** |

### Wave 3 — AR / AP / card funds / master data

| Workspace | File | Done | Open | Total |
| --- | --- | ---: | ---: | ---: |
| W11 | `w11-customer-receivables.md` | 8 | 1 | 9 |
| W12 | `w12-supplier-payables.md` | 9 | 1 | 10 |
| W13 | `w13-card-funds-review.md` | 9 | 4 | 13 |
| W14 | `w14-master-data.md` | 7 | 9 | 16 |
| **Wave 3** | | **33** | **15** | **48** |

### Wave 4 — analytics / sync / import / permissions

| Workspace | File | Done | Open | Total |
| --- | --- | ---: | ---: | ---: |
| W15 | `w15-customer-business-quality.md` | 11 | 6 | 17 |
| W16 | `w16-actual-profit-loss.md` | 14 | 2 | 16 |
| W17 | `w17-mall-sync-mapping.md` | 8 | 18 | 26 |
| W18 | `w18-import-opening.md` | 13 | 11 | 24 |
| W19 | `w19-permissions-audit.md` | 12 | 14 | 26 |
| **Wave 4** | | **58** | **51** | **109** |

### Wave 5 — supplier API / supply / publication / projection / migration

| Workspace | File | Done | Open | Total |
| --- | --- | ---: | ---: | ---: |
| W20 | `w20-supplier-api-connections.md` | 10 | 10 | 20 |
| W21 | `w21-external-product-supply.md` | 10 | 14 | 24 |
| W22 | `w22-product-publication.md` | 16 | 14 | 30 |
| W23 | `w23-execution-projection.md` | 15 | 8 | 23 |
| W24 | `w24-ownership-migration.md` | 18 | 5 | 23 |
| **Wave 5** | | **69** | **51** | **120** |

### Wave 6 — mall consumption / supplier orders / settlement / analytics / errors / backfill

| Workspace | File | Done | Open | Total |
| --- | --- | ---: | ---: | ---: |
| W25 | `w25-mall-consumption-orders.md` | 14 | 10 | 24 |
| W26 | `w26-supplier-orders.md` | 12 | 9 | 21 |
| W27 | `w27-api-settlement.md` | 12 | 10 | 22 |
| W28 | `w28-card-consumption-analytics.md` | 15 | 5 | 20 |
| W29 | `w29-integration-error-reconciliation.md` | 13 | 9 | 22 |
| W30 | `w30-historical-consumption-backfill.md` | 17 | 2 | 19 |
| **Wave 6** | | **83** | **45** | **128** |

### Grand total (waves 2–6)

| | Done `[x]` | Open `[ ]` | Total |
| --- | ---: | ---: | ---: |
| **Waves 2–6** | **295** | **195** | **490** |
| Integrate flips (2–6) | **280** | — | — |

Note: live `done` (295) ≥ integrate flip sum (280) because some rows were already `[x]` before a wave’s integrate pass, or multi-line confirmations / prior partial work accumulated outside the flip counters.

## Cross-cutting remaining gaps

Themes that dominate the **195** open rows (not exhaustive):

1. **Viewport / a11y acceptance (§9 / §10)**  
   Nearly every workspace still has combined “§9 all states” and/or “1440/1280/1024/768/375 five breakpoints” open. Keyboard/focus-restore lines often share that bucket.

2. **W02 task envelopes (complete / action / close / transfer)**  
   Heavy open concentration: W08 audit/hang, W09 Q1 task model, W13 claim/transfer same-tx, W17 mapping tasks, W18 import complete, W21/W22 supply+publication, W24 migration confirm, W26 query/replay/hang, W27 reject/confirm, W29 resolve/close.

3. **Server-true domain rules still mock-bound**  
   W08 split-merge / W07 line refs; W09 payment gate + reverse facts; W14 overlap/SKU/unit/stop guards; W17 sync freeze/watermark/remap; W25 refund tri-track & fact-key dedupe; W28 cost-by-consumption rebuild; W30 reattribute after map fix.

4. **Field permission / revoke / export re-auth**  
   Open on W04 attachments, W14 sensitive fields, W18/W19/W20/W21/W22/W23/W24/W25/W26/W27/W28 — especially short-lived reveal, cache clear on revoke, export download re-auth.

5. **Highest open counts (priority backlog)**  
   | WS | Open | Wave |
   | --- | ---: | ---: |
   | W17 mall-sync-mapping | 18 | 4 |
   | W19 permissions-audit | 14 | 4 |
   | W21 external-product-supply | 14 | 5 |
   | W22 product-publication | 14 | 5 |
   | W09 fulfillment-operations | 12 | 2 |
   | W18 import-opening | 11 | 4 |
   | W20 / W25 / W27 | 10 each | 5–6 |

6. **Nearly closed workspaces (≤2 open)**  
   W10 (1), W11 (1), W12 (1), W16 (2), W30 (2) — mostly viewport/keyboard only.

## Wave-4 build note (resolved later)

At wave-4 integrate time:

```
useSearchParams() should be wrapped in a suspense boundary at page "/analytics/profit-loss"
```

Wave-5/6 full-repo `npm run build` reported **OK** (33 routes). Treat wave-4 `build_ok=false` as historical for that integrate slice; current tree build status is green per wave-6 progress.

## Reference progress files

- [`_wave-2-progress.md`](./_wave-2-progress.md) — 49 flipped, 33 open  
- [`_wave-3-progress.md`](./_wave-3-progress.md) — 31 flipped, 15 open  
- [`_wave-4-progress.md`](./_wave-4-progress.md) — 54 flipped, 51 open (build FAIL at integrate)  
- [`_wave-5-progress.md`](./_wave-5-progress.md) — 65 flipped, 51 open  
- [`_wave-6-progress.md`](./_wave-6-progress.md) — 81 flipped, 45 open  

## Suggested next focus (not part of this rollup’s work)

1. Close **§9/§10 + keyboard** matrices for near-done WS (W10–W12, W16, W30).  
2. Finish **W02 envelope** gaps that block formal queue processing (W08/W09/W13/W26/W29).  
3. Attack **W17 / W19 / W21 / W22** open clusters (largest remaining surface).  
4. Confirm full-repo build stays green after any profit-loss Suspense / concurrent TS fixes.
