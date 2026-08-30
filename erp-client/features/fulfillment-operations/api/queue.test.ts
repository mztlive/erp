import { beforeEach, describe, expect, it, vi } from "vitest"

import { apiGet } from "@/lib/api"

import { fetchFulfillmentQueue } from "./queue"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
}))

const apiGetMock = vi.mocked(apiGet)

describe("fetchFulfillmentQueue W01 exact object query", () => {
    beforeEach(() => {
        apiGetMock.mockReset()
    })

    it("loads one receipt detail by frozen id without scanning lists", async () => {
        apiGetMock.mockResolvedValue({
            receipt: {
                id: "receipt/1",
                receipt_no: "PR-001",
                purchase_order_id: "po-1",
                warehouse_id: "warehouse-1",
                status: "DRAFT",
                version: 3,
                created_at: 1_700_000_000,
            },
            lines: [
                {
                    id: "receipt-line-1",
                    line_no: 1,
                    purchase_order_revision_line_id: "po-line-1",
                    received_quantity: "2",
                    qualified_quantity: "2",
                    rejected_quantity: "0",
                    quality_result: "QUALIFIED",
                },
            ],
        })

        const view = await fetchFulfillmentQueue({
            role: "warehouse",
            operationTypes: ["RECEIPT"],
            operationId: "receipt/1",
            currentOperationId: "receipt/1",
        })

        expect(apiGetMock).toHaveBeenCalledWith(
            "/admin/purchase-receipts/receipt%2F1",
        )
        expect(view.operations).toHaveLength(1)
        expect(view.current?.operationId).toBe("receipt/1")
        expect(view.current?.lines).toHaveLength(1)
        expect(view.emptyReason).toBeUndefined()
    })

    it("fails closed when a delivery does not match the frozen operation type", async () => {
        apiGetMock.mockResolvedValue({
            delivery: {
                id: "delivery-1",
                delivery_no: "DV-001",
                delivery_type: "WAREHOUSE_SHIP",
                sales_order_id: "so-1",
                warehouse_id: "warehouse-1",
                status: "DRAFT",
                version: 1,
                created_at: 1_700_000_000,
            },
            lines: [],
        })

        const view = await fetchFulfillmentQueue({
            role: "procurement",
            operationTypes: ["SUPPLIER_DIRECT"],
            operationId: "delivery-1",
        })

        expect(apiGetMock).toHaveBeenCalledWith("/admin/deliveries/delivery-1")
        expect(view.operations).toHaveLength(0)
        expect(view.emptyReason).toBe("FILTER_NO_RESULT")
    })
})

describe("fetchFulfillmentQueue server pagination", () => {
    beforeEach(() => {
        apiGetMock.mockReset()
    })

    it("requests one WorkItem-owned page instead of scanning document lists", async () => {
        apiGetMock.mockImplementation(async (path) => {
            if (path === "/admin/work-items/fulfillment-queue") {
                return {
                    items: [
                        {
                            work_item_id: "work-1",
                            task_version: "2",
                            source_version: "3",
                            owner_role: "warehouse_inbound_handler",
                            owner_organization_id: "warehouse-1",
                            priority: "normal",
                            reason_code: "PURCHASE_RECEIPT_READY",
                            impact_summary: "采购入库待确认",
                            operation_id: "receipt-1",
                            operation_type: "RECEIPT",
                            business_object_type: "purchase_receipt",
                            summary: "PR-001",
                            edit_version: 3,
                            due_at: 1_700_000_000,
                            sales_order_id: "sales-1",
                            sales_order_no: "SO-001",
                            purchase_order_id: "purchase-1",
                            purchase_order_no: "PO-001",
                            warehouse_id: "warehouse-1",
                            warehouse_label: "WH-001",
                            gate_state: "NOT_APPLICABLE",
                        },
                    ],
                    total: 41,
                    page: 2,
                    page_size: 20,
                    queue_context_id: "queue-context-1",
                    visible_types: ["RECEIPT", "WAREHOUSE_SHIP"],
                    metrics: [
                        { operation_type: "RECEIPT", count: 21 },
                        { operation_type: "WAREHOUSE_SHIP", count: 20 },
                    ],
                    warehouse_options: [{ id: "warehouse-1", label: "WH-001" }],
                    as_of: 1_700_000_100,
                }
            }
            if (path === "/admin/purchase-receipts/receipt-1") {
                return {
                    receipt: {
                        id: "receipt-1",
                        receipt_no: "PR-001",
                        purchase_order_id: "purchase-1",
                        warehouse_id: "warehouse-1",
                        status: "DRAFT",
                        version: 3,
                        created_at: 1_700_000_000,
                    },
                    lines: [],
                }
            }
            if (path === "/admin/sales-orders/sales-1") {
                return { id: "sales-1", order_no: "SO-001" }
            }
            if (path === "/admin/purchase-orders/purchase-1") {
                return {
                    id: "purchase-1",
                    purchase_no: "PO-001",
                    sales_order_id: "sales-1",
                    sales_order_no: "SO-001",
                }
            }
            if (path === "/admin/warehouses/warehouse-1") {
                return { id: "warehouse-1", warehouse_code: "WH-001" }
            }
            throw new Error(`unexpected path: ${path}`)
        })

        const view = await fetchFulfillmentQueue({
            role: "warehouse",
            page: 2,
            pageSize: 20,
        })

        expect(apiGetMock).toHaveBeenCalledWith(
            "/admin/work-items/fulfillment-queue",
            expect.objectContaining({
                operation_types: "RECEIPT,WAREHOUSE_SHIP",
                page: 2,
                page_size: 20,
            }),
        )
        expect(
            apiGetMock.mock.calls.filter(
                ([path]) =>
                    path === "/admin/purchase-receipts" ||
                    path === "/admin/deliveries" ||
                    path === "/admin/electronic-deliveries" ||
                    path === "/admin/service-fulfillments" ||
                    path === "/admin/warehouses",
            ),
        ).toHaveLength(0)
        expect(view.context).toMatchObject({
            page: 2,
            pageSize: 20,
            totalPages: 3,
            total: 41,
            position: 21,
        })
        expect(view.operations).toHaveLength(1)
    })
})
