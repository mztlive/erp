import { act, renderHook } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { useAcceptanceSelection } from "./use-acceptance-selection"
import { emptyLineResult } from "@/features/sales-orders/lib/acceptance-model"
import type {
    AcceptanceDraftLine,
    AcceptanceEligibleFact,
    AcceptanceSalesLineGroup,
} from "@/features/sales-orders/lib/acceptance-types"

const makeFact = (
    overrides: Partial<AcceptanceEligibleFact> = {},
): AcceptanceEligibleFact => ({
    fulfillmentLineId: "fl_1",
    fulfillmentFactType: "WAREHOUSE_SHIP",
    fulfillmentNo: "FH-1",
    salesOrderLineId: "sl_1",
    lineNo: 10,
    itemSnapshot: "测试品项",
    unitCode: "件",
    occurredAt: "2026-08-01T08:00:00.000Z",
    netSuccessfulQuantity: "100",
    netAcceptedAllocatedQuantity: "0",
    eligibleQuantity: "100",
    ...overrides,
})

const makeGroup = (
    facts: AcceptanceEligibleFact[],
): AcceptanceSalesLineGroup => ({
    salesOrderLineId: facts[0]?.salesOrderLineId ?? "sl_1",
    lineNo: facts[0]?.lineNo ?? 10,
    itemSnapshot: facts[0]?.itemSnapshot ?? "测试品项",
    unitCode: facts[0]?.unitCode ?? "件",
    requiredQuantity: "100",
    netAcceptedQuantity: "0",
    fulfillmentFacts: facts,
})

describe("useAcceptanceSelection", () => {
    it("starts empty with a PASS preview", () => {
        const { result } = renderHook(() => useAcceptanceSelection())

        expect(result.current.selected.size).toBe(0)
        expect(result.current.lineResults.size).toBe(0)
        expect(result.current.overallPreview).toBe("PASS")
        expect(result.current.hasExceptionResult).toBe(false)
    })

    it("selects a fact with its full eligible quantity and derives the line result", () => {
        const { result } = renderHook(() => useAcceptanceSelection())
        const fact = makeFact()

        act(() => result.current.toggleFact(fact, true))

        expect(result.current.selected.get("fl_1")).toEqual({
            fact,
            qty: "100",
        })
        expect(result.current.lineResults.get("sl_1")).toEqual({
            ...emptyLineResult(),
            acceptedQuantity: "100",
        })
        expect(result.current.overallPreview).toBe("PASS")
        expect([...result.current.selectedLines.keys()]).toEqual(["sl_1"])
    })

    it("deselecting the last fact clears the orphaned line result", () => {
        const { result } = renderHook(() => useAcceptanceSelection())
        const fact = makeFact()

        act(() => result.current.toggleFact(fact, true))
        act(() => result.current.toggleFact(fact, false))

        expect(result.current.selected.size).toBe(0)
        expect(result.current.lineResults.size).toBe(0)
    })

    it("refills the accepted quantity when the allocation changes", () => {
        const { result } = renderHook(() => useAcceptanceSelection())
        const fact = makeFact()

        act(() => result.current.toggleFact(fact, true))
        act(() => result.current.setAllocQty("fl_1", "40"))

        expect(result.current.selected.get("fl_1")?.qty).toBe("40")
        expect(result.current.lineResults.get("sl_1")?.acceptedQuantity).toBe(
            "40",
        )
    })

    it("does not overwrite a manually edited accepted quantity (acceptedManual)", () => {
        const { result } = renderHook(() => useAcceptanceSelection())
        const fact = makeFact()

        act(() => result.current.toggleFact(fact, true))
        act(() =>
            result.current.updateLineResult("sl_1", {
                acceptedQuantity: "30",
            }),
        )
        expect(result.current.lineResults.get("sl_1")?.acceptedManual).toBe(
            true,
        )

        act(() => result.current.setAllocQty("fl_1", "60"))

        expect(result.current.lineResults.get("sl_1")?.acceptedQuantity).toBe(
            "30",
        )
    })

    it("keeps a service-fail line result while another allocation remains", () => {
        const { result } = renderHook(() => useAcceptanceSelection())
        const first = makeFact()
        const second = makeFact({
            fulfillmentLineId: "fl_2",
            fulfillmentNo: "FH-2",
            eligibleQuantity: "50",
        })

        act(() => result.current.toggleFact(first, true))
        act(() => result.current.toggleFact(second, true))
        act(() =>
            result.current.updateLineResult("sl_1", { serviceFail: true }),
        )
        act(() => result.current.toggleFact(second, false))

        expect(result.current.lineResults.get("sl_1")?.serviceFail).toBe(true)
        expect(result.current.overallPreview).toBe("SERVICE_FAIL")
    })

    it("derives the overall preview precedence: service fail > reject > short", () => {
        const { result } = renderHook(() => useAcceptanceSelection())

        act(() =>
            result.current.updateLineResult("sl_a", { shortQuantity: "1" }),
        )
        expect(result.current.overallPreview).toBe("SHORT")

        act(() =>
            result.current.updateLineResult("sl_a", { rejectedQuantity: "1" }),
        )
        expect(result.current.overallPreview).toBe("REJECT")

        act(() =>
            result.current.updateLineResult("sl_a", { serviceFail: true }),
        )
        expect(result.current.overallPreview).toBe("SERVICE_FAIL")
    })

    it("updating an unknown line creates it with defaults", () => {
        const { result } = renderHook(() => useAcceptanceSelection())

        act(() => result.current.updateLineResult("sl_new", { reason: "少发" }))

        expect(result.current.lineResults.get("sl_new")).toEqual({
            ...emptyLineResult(),
            reason: "少发",
        })
    })

    it("restores the draft into selection and line results via the fact index", () => {
        const { result } = renderHook(() => useAcceptanceSelection())
        const kept = makeFact()
        const draftLines: AcceptanceDraftLine[] = [
            {
                salesOrderLineId: "sl_1",
                acceptedQuantity: "80",
                shortQuantity: "20",
                rejectedQuantity: "0",
                reason: "运输损耗",
                serviceFail: false,
                allocations: [
                    { fulfillmentLineId: "fl_1", allocatedQuantity: "80" },
                    { fulfillmentLineId: "fl_hidden", allocatedQuantity: "20" },
                ],
            },
        ]

        act(() => result.current.restoreDraft(draftLines, [makeGroup([kept])]))

        expect(result.current.selected.size).toBe(1)
        expect(result.current.selected.get("fl_1")).toEqual({
            fact: kept,
            qty: "80",
        })
        expect(result.current.lineResults.get("sl_1")).toEqual({
            acceptedQuantity: "80",
            shortQuantity: "20",
            rejectedQuantity: "0",
            reason: "运输损耗",
            serviceFail: false,
            acceptedManual: true,
        })
    })

    it("restores draft lines whose facts are all hidden as results only", () => {
        const { result } = renderHook(() => useAcceptanceSelection())
        const draftLines: AcceptanceDraftLine[] = [
            {
                salesOrderLineId: "sl_1",
                acceptedQuantity: "10",
                shortQuantity: "0",
                rejectedQuantity: "0",
                reason: "",
                serviceFail: false,
                allocations: [
                    { fulfillmentLineId: "fl_gone", allocatedQuantity: "10" },
                ],
            },
        ]

        act(() => result.current.restoreDraft(draftLines, []))

        expect(result.current.selected.size).toBe(0)
        expect(result.current.lineResults.get("sl_1")?.acceptedQuantity).toBe(
            "10",
        )
    })

    it("reset clears both maps", () => {
        const { result } = renderHook(() => useAcceptanceSelection())
        const fact = makeFact()

        act(() => result.current.toggleFact(fact, true))
        act(() => result.current.reset())

        expect(result.current.selected.size).toBe(0)
        expect(result.current.lineResults.size).toBe(0)
    })
})
