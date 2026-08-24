import { beforeEach, describe, expect, it, vi } from "vitest"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
}))

vi.mock("./hydrate", () => ({
    hydrateOperationDetail: vi.fn(async (operation) => operation),
}))

import { apiGet } from "@/lib/api"
import { fetchFulfillmentQueue } from "./queue"

const mockedApiGet = vi.mocked(apiGet)

function page<T>(items: T[]) {
    return {
        items,
        total: items.length,
        page: 1,
        page_size: 100,
    }
}

describe("fetchFulfillmentQueue sales-order scope", () => {
    beforeEach(() => {
        mockedApiGet.mockReset()
        mockedApiGet.mockImplementation(async (path) => {
            if (path === "/admin/purchase-orders") {
                return page([{ id: "po-linked" }])
            }
            if (path === "/admin/purchase-receipts") {
                return page([
                    {
                        id: "receipt-linked",
                        receipt_no: "RK-LINKED",
                        purchase_order_id: "po-linked",
                        warehouse_id: "wh-1",
                        status: "DRAFT",
                        version: 1,
                        created_at: 1_700_000_000,
                    },
                    {
                        id: "receipt-other",
                        receipt_no: "RK-OTHER",
                        purchase_order_id: "po-other",
                        warehouse_id: "wh-1",
                        status: "DRAFT",
                        version: 1,
                        created_at: 1_700_000_001,
                    },
                ])
            }
            if (
                path === "/admin/deliveries" ||
                path === "/admin/electronic-deliveries" ||
                path === "/admin/service-fulfillments"
            ) {
                return page([])
            }
            if (path === "/admin/warehouses") {
                return page([{ id: "wh-1", warehouse_code: "WH-1" }])
            }
            throw new Error(`Unexpected request: ${path}`)
        })
    })

    it("keeps only receipts from purchase orders linked to the current sales order", async () => {
        const result = await fetchFulfillmentQueue({
            role: "sales_order",
            salesOrderId: "so-1",
        })

        expect(
            result.operations.map((operation) => operation.operationId),
        ).toEqual(["receipt-linked"])
        expect(result.operations[0]?.source.salesOrderId).toBe("so-1")
        expect(result.operations[0]?.source.purchaseOrderId).toBe("po-linked")
    })

    it("reports permission denial instead of an empty completed queue", async () => {
        mockedApiGet.mockImplementation(async (path) => {
            if (path === "/admin/purchase-receipts") {
                throw { kind: "Http", message: "denied", status: 403 }
            }
            if (path === "/admin/deliveries" || path === "/admin/warehouses") {
                return page([])
            }
            throw new Error(`Unexpected request: ${path}`)
        })

        const result = await fetchFulfillmentQueue({
            role: "warehouse",
            operationTypes: ["RECEIPT"],
        })

        expect(result.emptyReason).toBe("NO_PERMISSION")
        expect(result.context.visibleTypes).not.toContain("RECEIPT")
    })

    it("loads later pages before composing the queue", async () => {
        mockedApiGet.mockImplementation(async (path, query) => {
            if (path === "/admin/electronic-deliveries") {
                const currentPage = Number(query?.page ?? 1)
                return {
                    items: [
                        {
                            id: `electronic-${currentPage}`,
                            fulfillment_no: `E-${currentPage}`,
                            purchase_order_id: "po-1",
                            sales_order_line_id: `line-${currentPage}`,
                            purchase_line_sales_allocation_id: `allocation-${currentPage}`,
                            quantity: "1",
                            result: "SUCCESS",
                            status: "DRAFT",
                            version: 1,
                            occurred_at: 1_700_000_000 + currentPage,
                        },
                    ],
                    total: 2,
                    page: currentPage,
                    page_size: 100,
                }
            }
            if (
                path === "/admin/deliveries" ||
                path === "/admin/service-fulfillments" ||
                path === "/admin/warehouses"
            ) {
                return page([])
            }
            throw new Error(`Unexpected request: ${path}`)
        })

        const result = await fetchFulfillmentQueue({
            role: "procurement",
            operationTypes: ["ELECTRONIC"],
        })

        expect(
            result.operations.map((operation) => operation.operationId),
        ).toEqual(["electronic-1", "electronic-2"])
    })
})
