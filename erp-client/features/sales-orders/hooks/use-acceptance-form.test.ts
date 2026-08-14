import { act, renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { useAcceptanceForm } from "./use-acceptance-form"
import { emptyLineResult } from "@/features/sales-orders/lib/acceptance-model"
import type { AcceptanceEligibleFact } from "@/features/sales-orders/lib/acceptance-types"

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

const initialProps = {
    selected: new Map<string, { fact: AcceptanceEligibleFact; qty: string }>(),
    lineResults: new Map(),
    onValidSubmit: vi.fn(),
}

function submit(form: { handleSubmit: () => Promise<void> }) {
    return act(async () => {
        await form.handleSubmit()
    })
}

describe("useAcceptanceForm", () => {
    it("initializes the header defaults", () => {
        const { result } = renderHook((props) => useAcceptanceForm(props), {
            initialProps,
        })

        expect(result.current.form.state.values.acceptedAt).toMatch(
            /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/,
        )
        expect(result.current.form.state.values.comment).toBe("")
        expect(result.current.formDirty).toBe(false)
        expect(result.current.clientIssues).toEqual([])
    })

    it("blocks submission without any selected source", async () => {
        const onValidSubmit = vi.fn()
        const { result } = renderHook((props) => useAcceptanceForm(props), {
            initialProps: { ...initialProps, onValidSubmit },
        })

        await submit(result.current.form)

        expect(onValidSubmit).not.toHaveBeenCalled()
        expect(result.current.clientIssues).toEqual([
            expect.objectContaining({ id: "no-source" }),
        ])
    })

    it("reports balance and reason issues for incomplete line results", async () => {
        const fact = makeFact()
        const onValidSubmit = vi.fn()
        const { result } = renderHook((props) => useAcceptanceForm(props), {
            initialProps: {
                selected: new Map([["fl_1", { fact, qty: "10" }]]),
                lineResults: new Map([
                    [
                        "sl_1",
                        { ...emptyLineResult(), acceptedQuantity: "5" },
                    ],
                ]),
                onValidSubmit,
            },
        })

        await submit(result.current.form)

        expect(onValidSubmit).not.toHaveBeenCalled()
        expect(result.current.clientIssues.map((i) => i.id)).toEqual([
            "line-balance-sl_1",
        ])
    })

    it("requires a reason when short or rejected quantities are entered", async () => {
        const fact = makeFact()
        const onValidSubmit = vi.fn()
        const { result } = renderHook((props) => useAcceptanceForm(props), {
            initialProps: {
                selected: new Map([["fl_1", { fact, qty: "10" }]]),
                lineResults: new Map([
                    [
                        "sl_1",
                        {
                            ...emptyLineResult(),
                            acceptedQuantity: "5",
                            shortQuantity: "5",
                        },
                    ],
                ]),
                onValidSubmit,
            },
        })

        await submit(result.current.form)

        expect(onValidSubmit).not.toHaveBeenCalled()
        expect(result.current.clientIssues.map((i) => i.id)).toEqual([
            "line-reason-sl_1",
        ])
    })

    it("calls onValidSubmit when the entry is complete and balanced", async () => {
        const fact = makeFact()
        const onValidSubmit = vi.fn()
        const { result } = renderHook((props) => useAcceptanceForm(props), {
            initialProps: {
                selected: new Map([["fl_1", { fact, qty: "10" }]]),
                lineResults: new Map([
                    [
                        "sl_1",
                        { ...emptyLineResult(), acceptedQuantity: "10" },
                    ],
                ]),
                onValidSubmit,
            },
        })

        await submit(result.current.form)

        expect(onValidSubmit).toHaveBeenCalledTimes(1)
        expect(result.current.clientIssues).toEqual([])
    })

    it("uses the latest selection values when they change between renders", async () => {
        const fact = makeFact()
        const onValidSubmit = vi.fn()
        const { result, rerender } = renderHook(
            (props) => useAcceptanceForm(props),
            {
                initialProps: {
                    selected: new Map(),
                    lineResults: new Map(),
                    onValidSubmit,
                },
            },
        )

        rerender({
            selected: new Map([["fl_1", { fact, qty: "10" }]]),
            lineResults: new Map([
                ["sl_1", { ...emptyLineResult(), acceptedQuantity: "10" }],
            ]),
            onValidSubmit,
        })

        await submit(result.current.form)

        expect(onValidSubmit).toHaveBeenCalledTimes(1)
        expect(result.current.clientIssues).toEqual([])
    })
})
