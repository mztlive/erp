import { beforeEach, describe, expect, it, vi } from "vitest"

import type { BackendPurchaseReceipt } from "./documents"
import { hydrateOperationDetail } from "./hydrate"
import { makeOperation } from "@/features/fulfillment-operations/pages/hooks/test-data"

const apiMocks = vi.hoisted(() => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
    apiPut: vi.fn(),
}))

vi.mock("@/lib/api", () => ({
    apiGet: apiMocks.apiGet,
    apiPost: apiMocks.apiPost,
    apiPut: apiMocks.apiPut,
}))

import { postFulfillmentOperation, saveFulfillmentOperation } from "./commands"

const receiptDraft = {
    type: "RECEIPT" as const,
    warehouseId: "wh_1",
    warehouseLabel: "中心仓",
    occurredAt: "2026-08-14T09:00",
    lines: [
        {
            purchaseRevisionLineId: "prl_1",
            receivedQuantity: "10",
            qualifiedQuantity: "10",
            rejectedQuantity: "0",
            qualityResult: "PASS",
        },
    ],
}

function receiptDto(
    overrides: Partial<BackendPurchaseReceipt> & { approval?: unknown } = {},
): BackendPurchaseReceipt & { approval?: unknown } {
    return {
        id: "pr-1",
        receipt_no: "RK-1",
        purchase_order_id: "po-1",
        warehouse_id: "wh_1",
        status: "DRAFT",
        version: 1,
        created_at: 1_700_000_000,
        ...overrides,
    }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("postFulfillmentOperation purchase receipt", () => {
    it("creates and posts the receipt without binding or starting approval", async () => {
        apiMocks.apiGet.mockResolvedValueOnce({
            receipt: receiptDto({
                approval: {
                    requirement: "PROCESS_REQUIRED",
                    allowed_actions: ["SUBMIT"],
                },
            }),
            lines: [],
        })
        apiMocks.apiPost.mockResolvedValueOnce(
            receiptDto({
                status: "POSTED",
                version: 2,
                posted_at: 1_700_000_100,
                approval: {
                    requirement: "PROCESS_REQUIRED",
                    allowed_actions: ["APPROVE"],
                },
            }),
        )

        const result = await postFulfillmentOperation({
            operationId: "pr-1",
            expectedDocumentVersion: 1,
            expectedSourceVersion: "1",
            idempotencyKey: "k-pr-1",
            draft: receiptDraft,
        })

        expect(result).toMatchObject({
            status: "succeeded",
        })
        if (result.status === "succeeded") {
            expect("approval" in result.outcome).toBe(false)
            expect(result.outcome.factType).toBe("PURCHASE_RECEIPT")
            expect(result.outcome.factNo).toBe("RK-1")
        }
        expect(apiMocks.apiPost.mock.calls.map(([path]) => path)).toEqual([
            "/admin/purchase-receipts/pr-1/post",
        ])
        expect(
            [
                ...apiMocks.apiGet.mock.calls,
                ...apiMocks.apiPost.mock.calls,
            ].every(
                ([path]) =>
                    typeof path === "string" &&
                    !path.includes("approval") &&
                    !path.includes("submit"),
            ),
        ).toBe(true)
        expect(apiMocks.apiPut).not.toHaveBeenCalled()
    })
})

describe("saveFulfillmentOperation purchase receipt", () => {
    it("saves the draft without an approval write path", async () => {
        apiMocks.apiPut.mockResolvedValueOnce(
            receiptDto({
                version: 2,
                approval: { requirement: "PROCESS_REQUIRED" },
            }),
        )
        const result = await saveFulfillmentOperation({
            operationId: "pr-1",
            expectedDocumentVersion: 1,
            expectedSourceVersion: "1",
            idempotencyKey: "k-save-1",
            draft: receiptDraft,
        })
        expect(result).toEqual({ editVersion: 2 })
        expect(apiMocks.apiPut.mock.calls.map(([path]) => path)).toEqual([
            "/admin/purchase-receipts/pr-1",
        ])
        expect(
            apiMocks.apiPut.mock.calls.every(
                ([path]) =>
                    typeof path === "string" && !path.includes("approval"),
            ),
        ).toBe(true)
    })
})

describe("hydrateOperationDetail purchase receipt", () => {
    it("hydrates receipt lines without carrying an approval projection", async () => {
        apiMocks.apiGet.mockResolvedValueOnce({
            receipt: receiptDto({
                version: 4,
                approval: {
                    requirement: "PROCESS_REQUIRED",
                    allowed_actions: ["SUBMIT"],
                },
            }),
            lines: [
                {
                    id: "line-1",
                    line_no: 1,
                    purchase_order_revision_line_id: "prl_1",
                    received_quantity: "10",
                    qualified_quantity: "10",
                    rejected_quantity: "0",
                    quality_result: "PASS",
                },
            ],
        })
        const hydrated = await hydrateOperationDetail(makeOperation())
        expect(hydrated.operationType).toBe("RECEIPT")
        expect(hydrated.editVersion).toBe(4)
        expect("approval" in hydrated).toBe(false)
        expect(hydrated.draft.type).toBe("RECEIPT")
        if (hydrated.draft.type === "RECEIPT") {
            expect(hydrated.draft.lines).toHaveLength(1)
        }
        expect(apiMocks.apiGet.mock.calls.map(([path]) => path)).toEqual([
            "/admin/purchase-receipts/op_1",
        ])
    })
})
