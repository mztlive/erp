import { describe, expect, it } from "vitest"

import {
    acceptanceConfirmLines,
    applyResultChange,
    batchDecisionsForFact,
    buildDraftLines,
    buildOrderProgress,
    collectValidationIssues,
    defaultBatchDraft,
    deriveOverall,
    lineAcceptanceHint,
    passQuantity,
    resultDecisionsForFact,
    summarizeLineDecisions,
    type AcceptanceBatchDraft,
} from "@/features/sales-orders/lib/acceptance-model"
import type { AcceptanceEligibleFact } from "@/features/sales-orders/lib/acceptance-types"

function fact(
    overrides: Partial<AcceptanceEligibleFact> &
        Pick<AcceptanceEligibleFact, "fulfillmentLineId" | "salesOrderLineId">,
): AcceptanceEligibleFact {
    return {
        fulfillmentFactType: "SUPPLIER_DIRECT",
        fulfillmentNo: "FH-1",
        lineNo: 1,
        itemSnapshot: "礼盒 A",
        unitCode: "盒",
        occurredAt: "2026-08-01T00:00:00.000Z",
        netSuccessfulQuantity: "10",
        netAcceptedAllocatedQuantity: "0",
        eligibleQuantity: "10",
        ...overrides,
    }
}

function draft(
    source: AcceptanceEligibleFact,
    patch: Partial<AcceptanceBatchDraft> = {},
): AcceptanceBatchDraft {
    return { ...defaultBatchDraft(source), ...patch }
}

describe("buildOrderProgress", () => {
    it("sums delivered, accepted and pending from fulfillment facts", () => {
        const progress = buildOrderProgress([
            {
                salesOrderLineId: "sol-1",
                lineNo: 1,
                itemSnapshot: "礼盒 A",
                unitCode: "盒",
                requiredQuantity: "20",
                netAcceptedQuantity: "0",
                fulfillmentFacts: [
                    fact({
                        fulfillmentLineId: "f-1",
                        salesOrderLineId: "sol-1",
                        netSuccessfulQuantity: "20",
                        netAcceptedAllocatedQuantity: "8",
                        eligibleQuantity: "12",
                    }),
                ],
            },
        ])
        expect(progress.acceptedQuantity).toBe("8")
        expect(progress.deliveredQuantity).toBe("20")
        expect(progress.pendingQuantity).toBe("12")
        expect(progress.lines[0]?.stuckKind).toBe("accept")
        expect(progress.lines[0]?.stuckLabel).toContain("待验")
    })

    it("marks undelivered remainder as waiting for shipment", () => {
        const progress = buildOrderProgress([
            {
                salesOrderLineId: "sol-1",
                lineNo: 1,
                itemSnapshot: "礼盒 B",
                unitCode: "盒",
                requiredQuantity: "50",
                netAcceptedQuantity: "0",
                fulfillmentFacts: [],
            },
        ])
        expect(progress.lines[0]?.stuckKind).toBe("deliver")
        expect(progress.lines[0]?.stuckLabel).toBe("还差 50 未交付")
    })
})

describe("buildDraftLines", () => {
    it("allocates only pass quantity so backend conservation holds", () => {
        const source = fact({
            fulfillmentLineId: "f-1",
            salesOrderLineId: "sol-1",
        })
        const selected = new Map([
            [
                source.fulfillmentLineId,
                draft(source, {
                    result: "SHORT",
                    exceptionQty: "2",
                    reason: "少两件",
                }),
            ],
        ])
        expect(passQuantity(selected.get("f-1")!)).toBe("8")
        const lines = buildDraftLines(selected)
        expect(lines).toEqual([
            {
                salesOrderLineId: "sol-1",
                acceptedQuantity: "8",
                shortQuantity: "2",
                rejectedQuantity: "0",
                reason: "少两件",
                serviceFail: false,
                allocations: [
                    {
                        fulfillmentLineId: "f-1",
                        fulfillmentFactType: "SUPPLIER_DIRECT",
                        allocatedQuantity: "8",
                    },
                ],
            },
        ])
        expect(deriveOverall(selected.values())).toBe("SHORT")
    })
})

describe("applyResultChange", () => {
    it("fills exception quantity with the batch qty when switching to short", () => {
        const source = fact({
            fulfillmentLineId: "f-1",
            salesOrderLineId: "sol-1",
            eligibleQuantity: "1",
        })
        const next = applyResultChange(defaultBatchDraft(source), "SHORT")
        expect(next.result).toBe("SHORT")
        expect(next.exceptionQty).toBe("1")
        expect(passQuantity(next)).toBe("0")
    })

    it("clears exception when switching back to pass", () => {
        const source = fact({
            fulfillmentLineId: "f-1",
            salesOrderLineId: "sol-1",
        })
        const short = applyResultChange(defaultBatchDraft(source), "SHORT")
        const passed = applyResultChange(short, "PASS")
        expect(passed.result).toBe("PASS")
        expect(passed.exceptionQty).toBe("0")
        expect(passed.reason).toBe("")
    })
})

describe("collectValidationIssues", () => {
    it("rejects a full-batch short because pass quantity would be zero", () => {
        const source = fact({
            fulfillmentLineId: "f-1",
            salesOrderLineId: "sol-1",
        })
        const selected = new Map([
            [
                source.fulfillmentLineId,
                draft(source, {
                    result: "SHORT",
                    exceptionQty: "10",
                    reason: "整批短少",
                }),
            ],
        ])
        const issues = collectValidationIssues(selected)
        expect(issues.some((issue) => issue.id.startsWith("line-pass-"))).toBe(
            true,
        )
    })
})

describe("resultDecisionsForFact", () => {
    it("lets service lines pass or fail, not shortage or reject", () => {
        const service = fact({
            fulfillmentLineId: "f-s",
            salesOrderLineId: "sol-s",
            fulfillmentFactType: "SERVICE",
        })
        expect(resultDecisionsForFact(service)).toEqual([
            "PASS",
            "SERVICE_FAIL",
        ])
        expect(batchDecisionsForFact(service)).toEqual([
            "SKIP",
            "PASS",
            "SERVICE_FAIL",
        ])
        expect(lineAcceptanceHint([service])).toBe(
            "服务明细只能记通过或不通过。",
        )
    })

    it("lets goods lines pass, short or reject, not service fail", () => {
        const goods = fact({
            fulfillmentLineId: "f-g",
            salesOrderLineId: "sol-g",
            fulfillmentFactType: "SUPPLIER_DIRECT",
        })
        expect(resultDecisionsForFact(goods)).toEqual([
            "PASS",
            "SHORT",
            "REJECT",
        ])
        expect(batchDecisionsForFact(goods)).toEqual([
            "SKIP",
            "PASS",
            "SHORT",
            "REJECT",
        ])
        expect(lineAcceptanceHint([goods])).toBe(
            "商品明细可记通过、短少或拒收。",
        )
    })
})

describe("acceptanceConfirmLines", () => {
    it("splits item, fulfillment and result so the confirm dialog can align them", () => {
        const passFact = fact({
            fulfillmentLineId: "f-1",
            salesOrderLineId: "sol-1",
            fulfillmentNo: "FH-1",
            lineNo: 1,
            itemSnapshot: "礼盒 A",
        })
        const shortFact = fact({
            fulfillmentLineId: "f-2",
            salesOrderLineId: "sol-1",
            fulfillmentNo: "FH-2",
            lineNo: 1,
            itemSnapshot: "礼盒 A",
        })
        const selected = new Map([
            [passFact.fulfillmentLineId, defaultBatchDraft(passFact)],
            [
                shortFact.fulfillmentLineId,
                draft(shortFact, {
                    result: "SHORT",
                    exceptionQty: "2",
                    reason: "少两件",
                }),
            ],
        ])
        expect(acceptanceConfirmLines(selected)).toEqual([
            {
                fulfillmentLineId: "f-1",
                itemLabel: "礼盒 A",
                fulfillmentLabel: "代发 FH-1",
                resultText: "通过 10 盒",
                resultTone: "success",
            },
            {
                fulfillmentLineId: "f-2",
                itemLabel: "礼盒 A",
                fulfillmentLabel: "代发 FH-2",
                resultText: "短少 2 盒、通过 8 盒",
                resultTone: "warning",
                reason: "少两件",
            },
        ])
    })

    it("hides opaque fulfillment ids and keeps service fail wording", () => {
        const skipped = fact({
            fulfillmentLineId: "f-skip",
            salesOrderLineId: "sol-1",
            fulfillmentNo: "FH-0",
        })
        const service = fact({
            fulfillmentLineId: "f-s",
            salesOrderLineId: "sol-2",
            fulfillmentFactType: "SERVICE",
            fulfillmentNo: "SF-fcc911d964bc4dc89b9ac67815522ea2",
            lineNo: 2,
            itemSnapshot: "安装服务",
            unitCode: "次",
            eligibleQuantity: "1",
        })
        const selected = new Map([
            [
                service.fulfillmentLineId,
                draft(service, {
                    result: "SERVICE_FAIL",
                    qty: "1",
                    exceptionQty: "1",
                    reason: "未完成安装",
                }),
            ],
        ])
        expect(acceptanceConfirmLines(selected)).toEqual([
            {
                fulfillmentLineId: "f-s",
                itemLabel: "安装服务",
                fulfillmentLabel: "服务履约",
                resultText: "不通过 1 次",
                resultTone: "destructive",
                reason: "未完成安装",
            },
        ])
        expect(
            acceptanceConfirmLines(selected).some(
                (line) => line.fulfillmentLineId === skipped.fulfillmentLineId,
            ),
        ).toBe(false)
    })
})

describe("summarizeLineDecisions", () => {
    it("reports skip when the batch is not in this acceptance", () => {
        const source = fact({
            fulfillmentLineId: "f-1",
            salesOrderLineId: "sol-1",
        })
        expect(summarizeLineDecisions([source], new Map())).toBe("本次不验")
    })

    it("reports all pass when every pending batch is accepted", () => {
        const source = fact({
            fulfillmentLineId: "f-1",
            salesOrderLineId: "sol-1",
        })
        expect(
            summarizeLineDecisions(
                [source],
                new Map([
                    [source.fulfillmentLineId, defaultBatchDraft(source)],
                ]),
            ),
        ).toBe("全部通过")
    })
})
