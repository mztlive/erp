import { beforeEach, describe, expect, it, vi } from "vitest"

import type { BackendServiceFulfillment } from "./documents"
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

const serviceDraft: Extract<FulfillmentDraft, { type: "SERVICE" }> = {
    type: "SERVICE",
    startedAt: "2026-08-14T09:00",
    endedAt: "2026-08-14T11:00",
    serviceLocation: "客户现场",
    result: "SUCCESS",
    completionNote: "已完成安装",
    lines: [
        {
            salesOrderLineId: "sol_1",
            purchaseLineSalesAllocationId: "alloc_1",
            quantity: "2",
        },
    ],
}

function serviceDto(
    overrides: Partial<BackendServiceFulfillment> & { approval?: unknown } = {},
): BackendServiceFulfillment & { approval?: unknown } {
    return {
        id: "sf-1",
        fulfillment_no: "SF-1",
        sales_order_line_id: "sol_1",
        purchase_order_id: "po-1",
        purchase_line_sales_allocation_id: "alloc_1",
        quantity: "2",
        result: "SUCCESS",
        status: "DRAFT",
        occurred_at: 1_700_000_000,
        recorded_at: 1_700_000_000,
        version: 1,
        ...overrides,
    }
}

function makeServiceOperation() {
    const base = makeOperation({
        operationId: "sf-1",
        operationType: "SERVICE",
    })
    return {
        ...base,
        draft: serviceDraft,
    }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("postFulfillmentOperation service fulfillment", () => {
    it("confirms the service fulfillment without binding or starting approval", async () => {
        apiMocks.apiPost.mockResolvedValueOnce(
            serviceDto({
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
            operationId: "sf-1",
            expectedDocumentVersion: 1,
            expectedSourceVersion: "1",
            idempotencyKey: "k-sf-1",
            draft: serviceDraft,
        })

        expect(result).toMatchObject({
            status: "succeeded",
        })
        if (result.status === "succeeded") {
            expect("approval" in result.outcome).toBe(false)
            expect(result.outcome.factType).toBe("SERVICE_FULFILLMENT")
            expect(result.outcome.factNo).toBe("SF-1")
            expect(result.outcome.operationType).toBe("SERVICE")
        }
        expect(apiMocks.apiPost.mock.calls.map(([path]) => path)).toEqual([
            "/admin/service-fulfillments/sf-1/confirm",
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

    it("rejects an empty service fulfillment line without an approval write path", async () => {
        const result = await postFulfillmentOperation({
            operationId: "sf-1",
            expectedDocumentVersion: 1,
            expectedSourceVersion: "1",
            idempotencyKey: "k-sf-empty",
            draft: { ...serviceDraft, lines: [] },
        })
        expect(result.status).toBe("failed")
        if (result.status === "failed") {
            expect(result.code).toBe("VALIDATION_BLOCKED")
        }
        expect(apiMocks.apiPost).not.toHaveBeenCalled()
        expect(apiMocks.apiGet).not.toHaveBeenCalled()
    })
})

describe("saveFulfillmentOperation service fulfillment", () => {
    it("does not save a service fulfillment draft and does not open an approval write path", async () => {
        await expect(
            saveFulfillmentOperation({
                operationId: "sf-1",
                expectedDocumentVersion: 1,
                expectedSourceVersion: "1",
                idempotencyKey: "k-save-sf-1",
                draft: serviceDraft,
            }),
        ).rejects.toThrow("电子交付与服务履约草稿不支持保存")
        expect(apiMocks.apiPut).not.toHaveBeenCalled()
        expect(apiMocks.apiPost).not.toHaveBeenCalled()
        expect(apiMocks.apiGet).not.toHaveBeenCalled()
    })
})

describe("hydrateOperationDetail service fulfillment", () => {
    it("keeps the service fulfillment projection without fetching an approval binding", async () => {
        const hydrated = await hydrateOperationDetail(makeServiceOperation())
        expect(hydrated.operationType).toBe("SERVICE")
        expect("approval" in hydrated).toBe(false)
        expect(hydrated.draft.type).toBe("SERVICE")
        expect(apiMocks.apiGet).not.toHaveBeenCalled()
        expect(apiMocks.apiPost).not.toHaveBeenCalled()
    })
})
