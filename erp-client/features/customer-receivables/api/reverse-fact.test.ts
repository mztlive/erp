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
    ensureCustomerRefundDraft,
    refundDrafts,
    reverseFact,
    reverseIdempotency,
    submitCustomerRefund,
} from "./reverse-fact"

const receipt = {
    id: "cr-1",
    receipt_no: "SK-1",
    status: "posted",
    customer_id: "c1",
    amount: "80.00",
}

const draftRefund = {
    id: "crf-1",
    refund_no: "TK-abcd",
    status: "draft",
    customer_id: "c1",
    original_receipt_id: "cr-1",
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
            id: "def-crf-1",
            name: "客户退款审批",
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
            id: "inst-crf-1",
            status: "RUNNING",
            current_round_no: 1,
            current_node: "退款复核",
            current_assignee: "张三",
        },
        allowed_actions: ["CANCEL"],
    },
}

describe("ensureCustomerRefundDraft", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        reverseIdempotency.clear()
        refundDrafts.clear()
    })

    it("creates a draft and maps the server binding", async () => {
        apiMocks.apiGet.mockResolvedValueOnce(receipt)
        apiMocks.apiPost.mockResolvedValueOnce(draftRefund)

        const result = await ensureCustomerRefundDraft({
            sourceFactId: "cr-1",
            reason: "退差额",
            idempotencyKey: "k-crf-ensure",
        })

        expect(result.status).toBe("succeeded")
        if (result.status !== "succeeded") return
        expect(result.refund.refundNo).toBe("TK-abcd")
        expect(result.refund.approval?.definition?.name).toBe("客户退款审批")
        expect(apiMocks.apiPost).toHaveBeenCalledWith(
            "/admin/customer-refunds",
            expect.objectContaining({
                original_receipt_id: "cr-1",
                reason_text: "退差额",
            }),
        )
        expect(
            apiMocks.apiPost.mock.calls.some(
                ([path]) =>
                    typeof path === "string" && path.endsWith("/post"),
            ),
        ).toBe(false)
    })
})

describe("submitCustomerRefund", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        reverseIdempotency.clear()
        refundDrafts.clear()
    })

    it("only posts version and idempotency key to the submit port", async () => {
        apiMocks.apiPost.mockResolvedValueOnce(submittedRefund)
        const result = await submitCustomerRefund({
            refundId: "crf-1",
            expectedVersion: 1,
            idempotencyKey: "k-crf-submit",
        })
        expect(result.status).toBe("succeeded")
        expect(apiMocks.apiPost).toHaveBeenCalledWith(
            "/admin/customer-refunds/crf-1/submit",
            {
                expected_version: 1,
                idempotency_key: "k-crf-submit",
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

describe("reverseFact refund path", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        reverseIdempotency.clear()
        refundDrafts.clear()
    })

    it("creates then submits approval and never posts", async () => {
        apiMocks.apiGet.mockResolvedValue(receipt)
        apiMocks.apiPost
            .mockResolvedValueOnce(draftRefund)
            .mockResolvedValueOnce(submittedRefund)

        const result = await reverseFact({
            kind: "refund",
            sourceFactId: "cr-1",
            reason: "退差额",
            idempotencyKey: "k-crf-full",
        })

        expect(result.status).toBe("succeeded")
        if (result.status !== "succeeded") return
        expect(result.message).toContain("提交客户退款审批")
        expect(result.approval?.instance?.currentAssigneeName).toBe("张三")
        expect(
            apiMocks.apiPost.mock.calls.map(([path]) => path),
        ).toEqual([
            "/admin/customer-refunds",
            "/admin/customer-refunds/crf-1/submit",
        ])
    })
})
