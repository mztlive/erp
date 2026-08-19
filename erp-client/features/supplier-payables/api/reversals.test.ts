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
    ensurePaymentReversalDraft,
    fetchPaymentReversal,
    paymentReversalDrafts,
    submitPaymentReversal,
} from "./reversals"
import { projectPaymentReversal } from "./mappers"

const payment = {
    id: "sp-1",
    payment_no: "FK-1",
    status: "POSTED",
    supplier_id: "sup-1",
    amount: "80.00",
}

const draftReversal = {
    id: "pr-1",
    reversal_no: "PCZ-abcd",
    status: "draft",
    original_supplier_payment_id: "sp-1",
    reason_text: "录入错误",
    amount: "80.00",
    handled_by: "finance_handler",
    reviewed_by: "finance_reviewer",
    occurred_at: 1_700_000_000,
    version: 1,
    created_at: 1_700_000_000,
    approval: {
        requirement: "PROCESS_REQUIRED",
        definition: {
            id: "def-pr-1",
            name: "付款冲正审批",
            version: 1,
            nodes: [{ key: "n1", name: "冲正复核", assignee_name: "张三" }],
        },
        allowed_actions: ["SUBMIT"],
    },
}

const submittedReversal = {
    ...draftReversal,
    status: "IN_APPROVAL",
    version: 2,
    approval: {
        ...draftReversal.approval,
        instance: {
            id: "inst-pr-1",
            status: "RUNNING",
            current_round_no: 1,
            current_node: "冲正复核",
            current_assignee: "张三",
        },
        allowed_actions: ["CANCEL"],
    },
}

describe("projectPaymentReversal", () => {
    it("maps the created binding without turning it into a work item", () => {
        const row = projectPaymentReversal(draftReversal)
        expect(row.status).toBe("draft")
        expect(row.statusLabel).toBe("草稿")
        expect(row.approval?.instance).toBeUndefined()
        expect(row.approval?.definition?.name).toBe("付款冲正审批")
        expect(row.approval?.allowedActions).toEqual(["SUBMIT"])
    })

    it("converges pending review into in-approval", () => {
        const row = projectPaymentReversal(submittedReversal)
        expect(row.status).toBe("in_approval")
        expect(row.statusLabel).toBe("审批中")
        expect(row.approval?.instance?.currentAssigneeName).toBe("张三")
    })
})

describe("ensurePaymentReversalDraft", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        paymentReversalDrafts.clear()
    })

    it("creates a draft and maps the server binding", async () => {
        apiMocks.apiGet.mockResolvedValueOnce(payment)
        apiMocks.apiPost.mockResolvedValueOnce(draftReversal)

        const result = await ensurePaymentReversalDraft({
            sourcePaymentId: "sp-1",
            amount: payment.amount,
            reason: "录入错误",
            idempotencyKey: "k-pr-ensure",
        })

        expect(result.status).toBe("succeeded")
        if (result.status !== "succeeded") return
        expect(result.reversal.reversalNo).toBe("PCZ-abcd")
        expect(result.reversal.approval?.definition?.name).toBe("付款冲正审批")
        expect(apiMocks.apiPost).toHaveBeenCalledWith(
            "/admin/payment-reversals",
            expect.objectContaining({
                original_supplier_payment_id: "sp-1",
                reason_text: "录入错误",
            }),
        )
        expect(
            apiMocks.apiPost.mock.calls.some(
                ([path]) => typeof path === "string" && path.endsWith("/post"),
            ),
        ).toBe(false)
    })

    it("does not reuse a cached draft for another payment or reason", async () => {
        apiMocks.apiGet.mockResolvedValueOnce(payment)
        apiMocks.apiPost.mockResolvedValueOnce(draftReversal)

        const first = await ensurePaymentReversalDraft({
            sourcePaymentId: "sp-1",
            reason: "录入错误",
            idempotencyKey: "k-pr-shared",
        })
        expect(first.status).toBe("succeeded")

        const otherSource = await ensurePaymentReversalDraft({
            sourcePaymentId: "sp-2",
            reason: "录入错误",
            idempotencyKey: "k-pr-shared",
        })
        expect(otherSource.status).toBe("failed")
        expect(otherSource).toMatchObject({ code: "REVERSAL_INTENT_MISMATCH" })
    })
})

describe("submitPaymentReversal", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        paymentReversalDrafts.clear()
    })

    it("only posts version and idempotency key to the submit port", async () => {
        apiMocks.apiPost.mockResolvedValueOnce(submittedReversal)
        const result = await submitPaymentReversal({
            reversalId: "pr-1",
            expectedVersion: 1,
            idempotencyKey: "k-pr-submit",
        })
        expect(result.status).toBe("succeeded")
        expect(apiMocks.apiPost).toHaveBeenCalledWith(
            "/admin/payment-reversals/pr-1/submit",
            {
                expected_version: 1,
                idempotency_key: "k-pr-submit",
            },
        )
        expect(apiMocks.apiPost.mock.calls[0][1]).not.toHaveProperty(
            "next_node",
        )
        expect(apiMocks.apiPost.mock.calls[0][1]).not.toHaveProperty(
            "reviewed_by",
        )
        expect(
            apiMocks.apiPost.mock.calls.some(
                ([path]) => typeof path === "string" && path.endsWith("/post"),
            ),
        ).toBe(false)
    })
})

describe("fetchPaymentReversal", () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it("returns the projected row or null when missing", async () => {
        apiMocks.apiGet.mockResolvedValueOnce(draftReversal)
        const row = await fetchPaymentReversal("pr-1")
        expect(row?.reversalNo).toBe("PCZ-abcd")
        apiMocks.apiGet.mockRejectedValueOnce(new Error("missing"))
        expect(await fetchPaymentReversal("missing")).toBeNull()
    })
})
