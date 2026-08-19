import { beforeEach, describe, expect, it, vi } from "vitest"

const apiMocks = vi.hoisted(() => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
}))

vi.mock("@/lib/api", () => ({
    apiGet: apiMocks.apiGet,
    apiPost: apiMocks.apiPost,
}))

import {
    ensureSupplierRefundDraft,
    fetchSupplierRefund,
    refundDrafts,
    submitSupplierRefund,
} from "./refunds"
import { projectSupplierRefund } from "./mappers"

const payment = {
    id: "sp-1",
    payment_no: "FK-1",
    status: "POSTED",
    supplier_id: "sup-1",
    amount: "80.00",
}

const draftRefund = {
    id: "srf-1",
    refund_no: "GTK-abcd",
    status: "draft",
    supplier_id: "sup-1",
    original_payment_id: "sp-1",
    reason_text: "退差额",
    amount: "80.00",
    handled_by: "finance_handler",
    reviewed_by: "finance_reviewer",
    occurred_at: 1_700_000_000,
    version: 1,
    created_at: 1_700_000_000,
    approval: {
        requirement: "PROCESS_REQUIRED",
        definition: {
            id: "def-srf-1",
            name: "供应商退款审批",
            version: 1,
            nodes: [{ key: "n1", name: "退款复核", assignee_name: "张三" }],
        },
        allowed_actions: ["SUBMIT"],
    },
}

const submittedRefund = {
    ...draftRefund,
    status: "IN_APPROVAL",
    version: 2,
    approval: {
        ...draftRefund.approval,
        instance: {
            id: "inst-srf-1",
            status: "RUNNING",
            current_round_no: 1,
            current_node: "退款复核",
            current_assignee: "张三",
        },
        allowed_actions: ["CANCEL"],
    },
}

describe("projectSupplierRefund", () => {
    it("maps the created binding without turning it into a work item", () => {
        const row = projectSupplierRefund(draftRefund)
        expect(row.status).toBe("draft")
        expect(row.statusLabel).toBe("草稿")
        expect(row.approval?.instance).toBeUndefined()
        expect(row.approval?.definition?.name).toBe("供应商退款审批")
        expect(row.approval?.allowedActions).toEqual(["SUBMIT"])
    })

    it("converges pending review into in-approval", () => {
        const row = projectSupplierRefund(submittedRefund)
        expect(row.status).toBe("in_approval")
        expect(row.statusLabel).toBe("审批中")
        expect(row.approval?.instance?.currentAssigneeName).toBe("张三")
    })
})

describe("ensureSupplierRefundDraft", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        refundDrafts.clear()
    })

    it("creates a draft and maps the server binding", async () => {
        apiMocks.apiPost.mockResolvedValueOnce(draftRefund)

        const result = await ensureSupplierRefundDraft({
            sourcePaymentId: "sp-1",
            supplierId: "sup-1",
            amount: payment.amount,
            reason: "退差额",
            idempotencyKey: "k-srf-ensure",
        })

        expect(result.status).toBe("succeeded")
        if (result.status !== "succeeded") return
        expect(result.refund.refundNo).toBe("GTK-abcd")
        expect(result.refund.approval?.definition?.name).toBe("供应商退款审批")
        expect(apiMocks.apiPost).toHaveBeenCalledWith(
            "/admin/supplier-refunds",
            expect.objectContaining({
                original_payment_id: "sp-1",
                reason_text: "退差额",
            }),
        )
        expect(
            apiMocks.apiPost.mock.calls.some(
                ([path]) => typeof path === "string" && path.endsWith("/post"),
            ),
        ).toBe(false)
    })

    it("does not reuse a cached draft for another payment or reason", async () => {
        apiMocks.apiPost.mockResolvedValueOnce(draftRefund)

        const first = await ensureSupplierRefundDraft({
            sourcePaymentId: "sp-1",
            supplierId: "sup-1",
            reason: "退差额",
            idempotencyKey: "k-srf-shared",
        })
        expect(first.status).toBe("succeeded")

        const otherSource = await ensureSupplierRefundDraft({
            sourcePaymentId: "sp-2",
            supplierId: "sup-1",
            reason: "退差额",
            idempotencyKey: "k-srf-shared",
        })
        expect(otherSource.status).toBe("failed")
        expect(otherSource).toMatchObject({ code: "REFUND_INTENT_MISMATCH" })
    })
})

describe("submitSupplierRefund", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        refundDrafts.clear()
    })

    it("only posts version and idempotency key to the submit port", async () => {
        apiMocks.apiPost.mockResolvedValueOnce(submittedRefund)
        const result = await submitSupplierRefund({
            refundId: "srf-1",
            expectedVersion: 1,
            idempotencyKey: "k-srf-submit",
        })
        expect(result.status).toBe("succeeded")
        expect(apiMocks.apiPost).toHaveBeenCalledWith(
            "/admin/supplier-refunds/srf-1/submit",
            {
                expected_version: 1,
                idempotency_key: "k-srf-submit",
            },
        )
        expect(apiMocks.apiPost.mock.calls[0][1]).not.toHaveProperty(
            "next_node",
        )
        expect(apiMocks.apiPost.mock.calls[0][1]).not.toHaveProperty(
            "reviewed_by",
        )
    })
})

describe("fetchSupplierRefund", () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it("returns the projected row or null when missing", async () => {
        apiMocks.apiGet.mockResolvedValueOnce(draftRefund)
        const row = await fetchSupplierRefund("srf-1")
        expect(row?.refundNo).toBe("GTK-abcd")
        apiMocks.apiGet.mockRejectedValueOnce(new Error("missing"))
        expect(await fetchSupplierRefund("missing")).toBeNull()
    })
})
