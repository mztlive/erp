import { describe, expect, it } from "vitest"

import { buildAllocationIssues } from "@/features/supplier-payables/lib/allocation-validation"
import type { AllocationIssueInput } from "@/features/supplier-payables/lib/allocation-validation"
import type { AllocationSessionView } from "@/features/supplier-payables/types"

const pool: AllocationSessionView["pool"] = [
    {
        payableAccountId: "pa-1",
        primaryEntryId: "pe-1",
        entryLockVersion: 1,
        accountLockVersion: 1,
        sourceType: "PURCHASE_ORDER",
        sourceTypeLabel: "采购单",
        sourceDocumentNo: "PO-1001",
        sourceDocumentId: "po-1",
        openTotal: "100.00",
        openInvoiceableTotal: "80.00",
        dueDate: "2026-08-20",
        dueStateLabel: "未到期",
        statusLabel: "未结",
    },
]

function build(
    overrides: Partial<AllocationIssueInput> = {},
): ReturnType<typeof buildAllocationIssues> {
    return buildAllocationIssues({
        track: "payment",
        selected: new Set<string>(),
        amounts: {},
        pool,
        allocatedHint: "0.00",
        factAmount: "0.00",
        ...overrides,
    })
}

describe("buildAllocationIssues", () => {
    it("requires at least one target", () => {
        const issues = build()
        expect(issues).toContainEqual({
            id: "no-target",
            label: "核销目标",
            message: "请至少选择一笔同供应商应付",
            targetId: "alloc-pool",
        })
    })

    it("requires a positive fact amount for new records", () => {
        const payment = build({ factAmount: "0.00" })
        expect(payment).toContainEqual(
            expect.objectContaining({ id: "amount", label: "付款金额" }),
        )

        const invoice = build({
            track: "purchase_invoice",
            factAmount: "",
        })
        expect(invoice).toContainEqual(
            expect.objectContaining({ id: "amount", label: "发票金额" }),
        )
    })

    it("skips the positive-amount rule when continuing an existing record", () => {
        const issues = build({
            existingPaymentId: "pmt-1",
            existingAmount: "100.00",
            factAmount: "0",
        })
        expect(issues.some((i) => i.id === "amount")).toBe(false)
    })

    it("flags allocation exceeding the record amount", () => {
        const issues = build({
            factAmount: "100.00",
            allocatedHint: "120.00",
        })
        expect(issues).toContainEqual(
            expect.objectContaining({
                id: "over",
                message: "拟分配合计超过本次记录金额，最终以系统校验为准",
            }),
        )
    })

    it("caps the record amount by existing unallocated first", () => {
        const issues = build({
            existingPaymentId: "pmt-1",
            existingAmount: "500.00",
            existingUnallocated: "40.00",
            allocatedHint: "45.00",
        })
        expect(issues).toContainEqual(
            expect.objectContaining({ id: "over" }),
        )
    })

    it("flags per-target over-allocation against open total", () => {
        const issues = build({
            selected: new Set(["pa-1"]),
            amounts: { "pa-1": "150.00" },
        })
        expect(issues).toContainEqual({
            id: "over-pa-1",
            label: "PO-1001",
            message: "拟分配超过开放余额 100.00",
        })
    })

    it("uses open invoiceable total as the cap for the invoice track", () => {
        const issues = build({
            track: "purchase_invoice",
            selected: new Set(["pa-1"]),
            amounts: { "pa-1": "90.00" },
        })
        expect(issues).toContainEqual({
            id: "over-pa-1",
            label: "PO-1001",
            message: "拟分配超过开放余额 80.00",
        })
    })

    it("flags non-positive per-target amounts", () => {
        const zero = build({
            selected: new Set(["pa-1"]),
            amounts: { "pa-1": "0" },
        })
        expect(zero).toContainEqual(
            expect.objectContaining({ id: "zero-pa-1" }),
        )

        const negative = build({
            selected: new Set(["pa-1"]),
            amounts: { "pa-1": "-5" },
        })
        expect(negative).toContainEqual(
            expect.objectContaining({ id: "zero-pa-1" }),
        )

        const blank = build({
            selected: new Set(["pa-1"]),
            amounts: {},
        })
        expect(blank).toContainEqual(
            expect.objectContaining({ id: "zero-pa-1" }),
        )
    })

    it("ignores selected ids that are not in the pool", () => {
        const issues = build({
            selected: new Set(["pa-404"]),
            amounts: { "pa-404": "999.00" },
        })
        expect(issues.some((i) => i.id === "over-pa-404")).toBe(false)
        expect(issues.some((i) => i.id === "zero-pa-404")).toBe(false)
        expect(issues.some((i) => i.id === "no-target")).toBe(false)
    })

    it("returns no issues for a consistent allocation", () => {
        const issues = build({
            selected: new Set(["pa-1"]),
            amounts: { "pa-1": "60.00" },
            allocatedHint: "60.00",
            factAmount: "60.00",
        })
        expect(issues).toEqual([])
    })

    it("tolerates a missing pool (session not loaded yet)", () => {
        const issues = build({
            pool: undefined,
            selected: new Set(["pa-1"]),
            amounts: { "pa-1": "60.00" },
        })
        // 池未加载时仅做整体校验，不逐项检查
        expect(issues.some((i) => i.id.startsWith("over-pa-"))).toBe(false)
        expect(issues.some((i) => i.id.startsWith("zero-pa-"))).toBe(false)
    })
})
