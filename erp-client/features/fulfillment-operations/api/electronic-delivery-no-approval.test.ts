import { beforeEach, describe, expect, it, vi } from "vitest"

import type { BackendElectronicDelivery } from "./documents"
import { hydrateOperationDetail } from "./hydrate"
import { makeOperation } from "@/features/fulfillment-operations/pages/hooks/test-data"
import type { FulfillmentDraft } from "@/features/fulfillment-operations/types"

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

const electronicDraft: Extract<FulfillmentDraft, { type: "ELECTRONIC" }> = {
    type: "ELECTRONIC",
    occurredAt: "2026-08-14T09:00",
    recipientMasked: "138****0001",
    result: "SUCCESS",
    lines: [
        {
            salesOrderLineId: "sol_1",
            purchaseLineSalesAllocationId: "alloc_1",
            quantity: "5",
        },
    ],
}

function electronicDto(
    overrides: Partial<BackendElectronicDelivery> & { approval?: unknown } = {},
): BackendElectronicDelivery & { approval?: unknown } {
    return {
        id: "ed-1",
        fulfillment_no: "DZ-1",
        sales_order_line_id: "sol_1",
        purchase_order_id: "po-1",
        purchase_line_sales_allocation_id: "alloc_1",
        quantity: "5",
        result: "SUCCESS",
        status: "DRAFT",
        occurred_at: 1_700_000_000,
        recorded_at: 1_700_000_000,
        version: 1,
        ...overrides,
    }
}

function makeElectronicOperation() {
    const base = makeOperation({
        operationId: "ed-1",
        operationType: "ELECTRONIC",
    })
    return {
        ...base,
        draft: electronicDraft,
    }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("postFulfillmentOperation electronic delivery", () => {
    it("confirms the electronic delivery without binding or starting approval", async () => {
        apiMocks.apiPost.mockResolvedValueOnce(
            electronicDto({
                status: "CONFIRMED",
                result: "SUCCESS",
                version: 2,
                occurred_at: 1_700_000_100,
                approval: {
                    requirement: "PROCESS_REQUIRED",
                    allowed_actions: ["APPROVE"],
                },
            }),
        )

        const result = await postFulfillmentOperation({
            operationId: "ed-1",
            expectedDocumentVersion: 1,
            expectedSourceVersion: "1",
            idempotencyKey: "k-ed-1",
            draft: electronicDraft,
        })

        expect(result).toMatchObject({
            status: "succeeded",
        })
        if (result.status === "succeeded") {
            expect("approval" in result.outcome).toBe(false)
            expect(result.outcome.factType).toBe("ELECTRONIC_DELIVERY")
            expect(result.outcome.factNo).toBe("DZ-1")
            expect(result.outcome.operationType).toBe("ELECTRONIC")
        }
        expect(apiMocks.apiPost.mock.calls.map(([path]) => path)).toEqual([
            "/admin/electronic-deliveries/ed-1/confirm",
        ])
        expect(
            [
                ...apiMocks.apiGet.mock.calls,
                ...apiMocks.apiPost.mock.calls,
                ...apiMocks.apiPut.mock.calls,
            ].every(
                ([path]) =>
                    typeof path === "string" &&
                    !path.includes("approval") &&
                    !path.includes("submit"),
            ),
        ).toBe(true)
        expect(apiMocks.apiPut).not.toHaveBeenCalled()
    })

    it("rejects an empty electronic delivery line without an approval write path", async () => {
        const result = await postFulfillmentOperation({
            operationId: "ed-1",
            expectedDocumentVersion: 1,
            expectedSourceVersion: "1",
            idempotencyKey: "k-ed-empty",
            draft: { ...electronicDraft, lines: [] },
        })
        expect(result.status).toBe("failed")
        if (result.status === "failed") {
            expect(result.code).toBe("VALIDATION_BLOCKED")
        }
        expect(apiMocks.apiPost).not.toHaveBeenCalled()
        expect(apiMocks.apiGet).not.toHaveBeenCalled()
    })
})

describe("saveFulfillmentOperation electronic delivery", () => {
    it("does not save an electronic delivery draft and does not open an approval write path", async () => {
        await expect(
            saveFulfillmentOperation({
                operationId: "ed-1",
                expectedDocumentVersion: 1,
                expectedSourceVersion: "1",
                idempotencyKey: "k-save-ed-1",
                draft: electronicDraft,
            }),
        ).rejects.toThrow("电子交付与服务履约草稿不支持保存")
        expect(apiMocks.apiPut).not.toHaveBeenCalled()
        expect(apiMocks.apiPost).not.toHaveBeenCalled()
        expect(apiMocks.apiGet).not.toHaveBeenCalled()
    })
})

describe("hydrateOperationDetail electronic delivery", () => {
    it("keeps the electronic delivery projection without fetching an approval binding", async () => {
        const hydrated = await hydrateOperationDetail(makeElectronicOperation())
        expect(hydrated.operationType).toBe("ELECTRONIC")
        expect("approval" in hydrated).toBe(false)
        expect(hydrated.draft.type).toBe("ELECTRONIC")
        expect(apiMocks.apiGet).not.toHaveBeenCalled()
        expect(apiMocks.apiPost).not.toHaveBeenCalled()
    })
})
