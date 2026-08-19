import { beforeEach, describe, expect, it, vi } from "vitest"

import type { BackendDelivery } from "./documents"
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

const shipDraft = {
    type: "WAREHOUSE_SHIP" as const,
    warehouseId: "wh_1",
    warehouseLabel: "中心仓",
    carrier: "顺丰",
    trackingNo: "SF-1",
    shippedAt: "2026-08-14T09:00",
    lines: [
        {
            salesOrderLineId: "sol_1",
            stockReservationId: "rsv_1",
            quantity: "10",
        },
    ],
}

const directDraft = {
    type: "SUPPLIER_DIRECT" as const,
    carrier: "顺丰",
    trackingNo: "SF-2",
    shippedAt: "2026-08-14T09:00",
    lines: [
        {
            salesOrderLineId: "sol_2",
            purchaseLineSalesAllocationId: "alloc_2",
            quantity: "4",
        },
    ],
}

function deliveryDto(
    overrides: Partial<BackendDelivery> & { approval?: unknown } = {},
): BackendDelivery & { approval?: unknown } {
    return {
        id: "dl-1",
        delivery_no: "FH-1",
        delivery_type: "WAREHOUSE_SHIP",
        sales_order_id: "so-1",
        warehouse_id: "wh_1",
        status: "DRAFT",
        version: 1,
        created_at: 1_700_000_000,
        ...overrides,
    }
}

function makeShipOperation() {
    const base = makeOperation({
        operationId: "dl-1",
        operationType: "WAREHOUSE_SHIP",
    })
    return {
        ...base,
        draft: shipDraft,
    }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("postFulfillmentOperation delivery", () => {
    it("creates and posts the warehouse delivery without binding or starting approval", async () => {
        apiMocks.apiGet.mockResolvedValueOnce({
            delivery: deliveryDto({
                approval: {
                    requirement: "PROCESS_REQUIRED",
                    allowed_actions: ["SUBMIT"],
                },
            }),
            lines: [],
        })
        apiMocks.apiPut.mockResolvedValueOnce(
            deliveryDto({
                version: 2,
                carrier: "顺丰",
                tracking_no: "SF-1",
                approval: { requirement: "PROCESS_REQUIRED" },
            }),
        )
        apiMocks.apiPost.mockResolvedValueOnce(
            deliveryDto({
                status: "SHIPPED",
                version: 3,
                shipped_at: 1_700_000_100,
                approval: {
                    requirement: "PROCESS_REQUIRED",
                    allowed_actions: ["APPROVE"],
                },
            }),
        )

        const result = await postFulfillmentOperation({
            operationId: "dl-1",
            expectedDocumentVersion: 1,
            expectedSourceVersion: "1",
            idempotencyKey: "k-dl-1",
            draft: shipDraft,
        })

        expect(result).toMatchObject({
            status: "succeeded",
        })
        if (result.status === "succeeded") {
            expect("approval" in result.outcome).toBe(false)
            expect(result.outcome.factType).toBe("DELIVERY")
            expect(result.outcome.factNo).toBe("FH-1")
        }
        expect(apiMocks.apiPut.mock.calls.map(([path]) => path)).toEqual([
            "/admin/deliveries/dl-1",
        ])
        expect(apiMocks.apiPost.mock.calls.map(([path]) => path)).toEqual([
            "/admin/deliveries/dl-1/post",
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
    })

    it("posts a supplier-direct delivery without an approval write path", async () => {
        apiMocks.apiGet.mockResolvedValueOnce({
            delivery: deliveryDto({
                id: "dl-2",
                delivery_no: "FH-2",
                delivery_type: "SUPPLIER_DIRECT",
                purchase_order_id: "po-2",
                warehouse_id: null,
                approval: { requirement: "PROCESS_REQUIRED" },
            }),
            lines: [],
        })
        apiMocks.apiPut.mockResolvedValueOnce(
            deliveryDto({
                id: "dl-2",
                delivery_no: "FH-2",
                delivery_type: "SUPPLIER_DIRECT",
                version: 2,
            }),
        )
        apiMocks.apiPost.mockResolvedValueOnce(
            deliveryDto({
                id: "dl-2",
                delivery_no: "FH-2",
                delivery_type: "SUPPLIER_DIRECT",
                status: "SHIPPED",
                version: 3,
                shipped_at: 1_700_000_100,
            }),
        )

        const result = await postFulfillmentOperation({
            operationId: "dl-2",
            expectedDocumentVersion: 1,
            expectedSourceVersion: "1",
            idempotencyKey: "k-dl-2",
            draft: directDraft,
        })

        expect(result.status).toBe("succeeded")
        if (result.status === "succeeded") {
            expect("approval" in result.outcome).toBe(false)
            expect(result.outcome.operationType).toBe("SUPPLIER_DIRECT")
        }
        expect(apiMocks.apiPost.mock.calls.map(([path]) => path)).toEqual([
            "/admin/deliveries/dl-2/post",
        ])
    })
})

describe("saveFulfillmentOperation delivery", () => {
    it("saves the warehouse-ship draft without an approval write path", async () => {
        apiMocks.apiPut.mockResolvedValueOnce(
            deliveryDto({
                version: 2,
                approval: { requirement: "PROCESS_REQUIRED" },
            }),
        )
        const result = await saveFulfillmentOperation({
            operationId: "dl-1",
            expectedDocumentVersion: 1,
            expectedSourceVersion: "1",
            idempotencyKey: "k-save-dl-1",
            draft: shipDraft,
        })
        expect(result).toEqual({ editVersion: 2 })
        expect(apiMocks.apiPut.mock.calls.map(([path]) => path)).toEqual([
            "/admin/deliveries/dl-1",
        ])
        expect(
            apiMocks.apiPut.mock.calls.every(
                ([path]) =>
                    typeof path === "string" && !path.includes("approval"),
            ),
        ).toBe(true)
    })
})

describe("hydrateOperationDetail delivery", () => {
    it("hydrates warehouse-ship lines without carrying an approval projection", async () => {
        apiMocks.apiGet.mockResolvedValueOnce({
            delivery: deliveryDto({
                version: 4,
                carrier: "顺丰",
                tracking_no: "SF-1",
                approval: {
                    requirement: "PROCESS_REQUIRED",
                    allowed_actions: ["SUBMIT"],
                },
            }),
            lines: [
                {
                    id: "line-1",
                    line_no: 1,
                    sales_order_line_id: "sol_1",
                    quantity: "10",
                    stock_reservation_id: "rsv_1",
                },
            ],
        })
        const hydrated = await hydrateOperationDetail(makeShipOperation())
        expect(hydrated.operationType).toBe("WAREHOUSE_SHIP")
        expect(hydrated.editVersion).toBe(4)
        expect("approval" in hydrated).toBe(false)
        expect(hydrated.draft.type).toBe("WAREHOUSE_SHIP")
        if (hydrated.draft.type === "WAREHOUSE_SHIP") {
            expect(hydrated.draft.lines).toHaveLength(1)
            expect(hydrated.draft.carrier).toBe("顺丰")
        }
        expect(apiMocks.apiGet.mock.calls.map(([path]) => path)).toEqual([
            "/admin/deliveries/dl-1",
        ])
    })
})
