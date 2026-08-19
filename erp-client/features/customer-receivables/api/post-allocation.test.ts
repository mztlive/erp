import { beforeEach, describe, expect, it, vi } from "vitest"

import type { AllocationSessionView } from "@/features/customer-receivables/types"

const apiMocks = vi.hoisted(() => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
}))

vi.mock("@/lib/api", () => ({
    apiGet: apiMocks.apiGet,
    apiPost: apiMocks.apiPost,
}))

import { postAllocation, postIdempotency } from "./post-allocation"
import { sessions } from "./session"

const session = (): AllocationSessionView => ({
    draftSessionId: "alloc_cust_1",
    mode: "receipt",
    counterpartyPartyId: "p1",
    counterpartyPartyName: "主体甲",
    customerId: "c1",
    customerName: "客户甲",
    status: "draft",
    fact: { receivedAt: "2026-01-01T10:00", amount: "100", bankReference: "ref" },
    pool: [],
    allocations: [
        {
            lineKey: "line_e1",
            targetId: "e1",
            targetKind: "receivable_entry",
            label: "SO-1",
            salesOrderNo: "SO-1",
            openAmount: "60",
            amount: "60",
            baselineVersion: 1,
        },
    ],
    proposedAllocatedTotal: "60.00",
    proposedUnallocated: "40.00",
    factAmount: "100",
    submitPolicy: {
        allowUnallocatedRemainder: true,
        label: "允许保留未分配余额",
    },
    leaseValid: true,
    editVersion: 1,
    note: "",
})

describe("postAllocation receipt submit", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        sessions.clear()
        postIdempotency.clear()
        sessions.set("alloc_cust_1", session())
    })

    it("creates the draft then submits approval instead of posting", async () => {
        apiMocks.apiPost
            .mockResolvedValueOnce({
                id: "cr-1",
                receipt_no: "SK-1",
                status: "draft",
                version: 1,
                allocated_total: "0.00",
                unallocated_amount: "100.00",
                approval: {
                    requirement: "PROCESS_REQUIRED",
                    definition: {
                        id: "def-cr-1",
                        name: "客户回款审批",
                        version: 2,
                        nodes: [{ key: "n1", name: "回款复核" }],
                    },
                    allowed_actions: ["SUBMIT"],
                },
            })
            .mockResolvedValueOnce({
                id: "cr-1",
                receipt_no: "SK-1",
                status: "IN_APPROVAL",
                version: 2,
                allocated_total: "60.00",
                unallocated_amount: "40.00",
                approval: {
                    requirement: "PROCESS_REQUIRED",
                    instance: {
                        id: "inst-cr-1",
                        status: "RUNNING",
                        current_round_no: 1,
                        current_assignee: "张三",
                    },
                    allowed_actions: ["CANCEL"],
                },
            })

        const result = await postAllocation({
            draftSessionId: "alloc_cust_1",
            editVersion: 1,
            idempotencyKey: "k-submit-1",
        })

        expect(apiMocks.apiPost.mock.calls[0][0]).toBe(
            "/admin/customer-receipts",
        )
        expect(apiMocks.apiPost.mock.calls[1][0]).toBe(
            "/admin/customer-receipts/cr-1/submit",
        )
        expect(apiMocks.apiPost.mock.calls[1][1]).toEqual({
            expected_version: 1,
            idempotency_key: "k-submit-1",
            allocations: [
                {
                    receivable_entry_id: "e1",
                    allocated_amount: "60",
                },
            ],
        })
        expect(result).toMatchObject({
            status: "succeeded",
            mode: "receipt",
            factId: "cr-1",
            factNo: "SK-1",
            subjectStatus: "IN_APPROVAL",
        })
        if (result.status === "succeeded") {
            expect(result.approval?.instance?.currentAssignee).toBe("张三")
        }
    })

    it("does not call the closed post bypass", async () => {
        apiMocks.apiPost.mockResolvedValue({
            id: "cr-1",
            receipt_no: "SK-1",
            status: "IN_APPROVAL",
            version: 2,
            allocated_total: "60.00",
            unallocated_amount: "40.00",
            approval: {
                requirement: "PROCESS_REQUIRED",
                allowed_actions: ["CANCEL"],
            },
        })
        sessions.set("alloc_cust_1", {
            ...session(),
            existingFactId: "cr-1",
            existingFactNo: "SK-1",
            existingFactVersion: 1,
        })
        apiMocks.apiGet.mockResolvedValue({
            id: "cr-1",
            receipt_no: "SK-1",
            status: "draft",
            version: 1,
            allocated_total: "0.00",
            unallocated_amount: "100.00",
            approval: {
                requirement: "PROCESS_REQUIRED",
                allowed_actions: ["SUBMIT"],
            },
        })

        await postAllocation({
            draftSessionId: "alloc_cust_1",
            editVersion: 1,
            idempotencyKey: "k-submit-2",
        })

        expect(
            apiMocks.apiPost.mock.calls.every(
                ([path]) =>
                    typeof path === "string" && !path.endsWith("/post"),
            ),
        ).toBe(true)
    })
})

const invoiceSession = (): AllocationSessionView => ({
    draftSessionId: "alloc_inv_1",
    mode: "invoice",
    counterpartyPartyId: "p1",
    counterpartyPartyName: "主体甲",
    customerId: "c1",
    customerName: "客户甲",
    status: "draft",
    fact: {
        invoiceNo: "FP-1",
        invoiceDate: "2026-01-15",
        grossAmount: "113.00",
        netAmount: "100.00",
        taxAmount: "13.00",
        invoiceKind: "blue",
    },
    pool: [],
    allocations: [
        {
            lineKey: "line_a1",
            targetId: "acc-1",
            targetKind: "receivable_account",
            label: "应收子账 #1",
            salesOrderNo: "SO-1",
            openAmount: "113.00",
            amount: "113.00",
            baselineVersion: 1,
        },
    ],
    proposedAllocatedTotal: "113.00",
    proposedUnallocated: "0.00",
    factAmount: "113.00",
    submitPolicy: {
        allowUnallocatedRemainder: true,
        label: "允许保留未分配余额",
    },
    leaseValid: true,
    editVersion: 1,
    note: "",
})

describe("postAllocation invoice register", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        sessions.clear()
        postIdempotency.clear()
        sessions.set("alloc_inv_1", invoiceSession())
    })

    it("creates and posts the invoice without binding or starting approval", async () => {
        apiMocks.apiPost
            .mockResolvedValueOnce({
                id: "inv-1",
                invoice_no: "FP-1",
                status: "draft",
                version: 1,
                allocated_total: "0.00",
                unallocated_amount: "113.00",
            })
            .mockResolvedValueOnce({
                id: "inv-1",
                invoice_no: "FP-1",
                status: "registered",
                version: 2,
                allocated_total: "113.00",
                unallocated_amount: "0.00",
            })

        const result = await postAllocation({
            draftSessionId: "alloc_inv_1",
            editVersion: 1,
            idempotencyKey: "k-inv-1",
        })

        expect(result).toMatchObject({
            status: "succeeded",
            mode: "invoice",
            factId: "inv-1",
            factNo: "FP-1",
        })
        if (result.status === "succeeded") {
            expect(result.approval).toBeUndefined()
            expect(result.subjectStatus).toBeUndefined()
        }
        expect(apiMocks.apiPost.mock.calls.map(([path]) => path)).toEqual([
            "/admin/invoices",
            "/admin/invoices/inv-1/post",
        ])
        expect(
            apiMocks.apiPost.mock.calls.every(
                ([path]) =>
                    typeof path === "string" &&
                    !path.includes("approval") &&
                    !path.includes("submit"),
            ),
        ).toBe(true)
    })
})
