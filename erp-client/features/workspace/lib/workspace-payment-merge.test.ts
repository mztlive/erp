import { describe, expect, it } from "vitest"

import {
    additionalPaymentWorkItems,
    canConfirmMergeSelection,
    defaultMergeSelectedIds,
    selectedMergeOpenTotal,
    type PaymentMergeTask,
} from "./workspace-payment-merge"

const items: readonly PaymentMergeTask[] = [
    {
        workItemId: "wi-1",
        taskVersion: "3",
        payableAccountId: "pa-1",
        subjectVersion: "8",
        sourceDocumentId: "po-1",
        openTotal: "10.00",
        sourceDocumentNo: "CG-1",
        isAnchor: true,
    },
    {
        workItemId: "wi-2",
        taskVersion: "1",
        payableAccountId: "pa-2",
        subjectVersion: "2",
        sourceDocumentId: "po-2",
        openTotal: "20.50",
        sourceDocumentNo: "CG-2",
        isAnchor: false,
    },
    {
        workItemId: "wi-3",
        taskVersion: "2",
        payableAccountId: "pa-3",
        subjectVersion: "4",
        sourceDocumentId: "po-3",
        openTotal: "5.00",
        sourceDocumentNo: "CG-3",
        isAnchor: false,
    },
]

describe("workspace payment merge selection", () => {
    it("defaults to selecting every candidate", () => {
        expect([...defaultMergeSelectedIds(items)]).toEqual([
            "pa-1",
            "pa-2",
            "pa-3",
        ])
    })

    it("requires the current task plus at least one more", () => {
        expect(canConfirmMergeSelection(new Set(["pa-1", "pa-2"]), items)).toBe(
            true,
        )
        expect(canConfirmMergeSelection(new Set(["pa-1"]), items)).toBe(false)
        expect(canConfirmMergeSelection(new Set(["pa-2", "pa-3"]), items)).toBe(
            false,
        )
    })

    it("sums selected open balances and omits the current task from additional items", () => {
        const selected = new Set(["pa-1", "pa-3"])
        expect(selectedMergeOpenTotal(selected, items)).toBe("15.00")
        expect(additionalPaymentWorkItems(selected, "pa-1", items)).toEqual([
            {
                workItemId: "wi-3",
                taskVersion: "2",
                payableAccountId: "pa-3",
                openTotal: "5.00",
            },
        ])
    })
})
