import { describe, expect, it } from "vitest"

import { mapListItemFromBackend } from "@/features/sales-orders/lib/sales-order-detail-mappers"
import {
    canCreatePurchaseFromSalesOrder,
    purchaseOrdersWorkspaceHref,
} from "@/features/sales-orders/lib/sales-order-detail-model"

describe("mapListItemFromBackend", () => {
    it("maps the authoritative purchase-order count into the related lane", () => {
        const order = mapListItemFromBackend(
            {
                id: "so-1",
                order_no: "XS202608230001",
                business_type: "GOODS_SERVICE",
                origin_system: "ERP",
                customer_id: "customer-1",
                commercial_status: "EFFECTIVE",
                review_status: "APPROVED",
                fulfillment_progress: "NOT_STARTED",
                collection_progress: "NOT_STARTED",
                invoice_progress: "NOT_STARTED",
                close_status: "OPEN",
                version: 1,
                created_at: 1,
                updated_at: 1,
                stage: {
                    code: "effective",
                    label: "已生效",
                    tone: "success",
                },
            },
            { purchaseOrderCount: 2 },
        )

        expect(order.related.purchaseOrders).toBe(2)
        expect(canCreatePurchaseFromSalesOrder(order)).toBe(false)
        expect(purchaseOrdersWorkspaceHref(order, "/sales/orders/so-1")).toBe(
            "/procurement/orders?salesOrderId=so-1&from=W05&returnTo=%2Fsales%2Forders%2Fso-1",
        )

        const orderWithoutPurchase = {
            ...order,
            related: { ...order.related, purchaseOrders: 0 },
        }
        expect(canCreatePurchaseFromSalesOrder(orderWithoutPurchase)).toBe(true)
        expect(
            purchaseOrdersWorkspaceHref(
                orderWithoutPurchase,
                "/sales/orders/so-1",
            ),
        ).toBe(
            "/procurement/orders?salesOrderId=so-1&action=create&from=W05&returnTo=%2Fsales%2Forders%2Fso-1",
        )
    })

    it("maps finance totals and removes acceptance after fulfillment completes", () => {
        const order = mapListItemFromBackend(
            {
                id: "so-closed",
                order_no: "XS202608230002",
                business_type: "GOODS_SERVICE",
                origin_system: "ERP",
                customer_id: "customer-1",
                commercial_status: "EFFECTIVE",
                review_status: "APPROVED",
                fulfillment_progress: "COMPLETED",
                collection_progress: "SETTLED",
                invoice_progress: "COMPLETED",
                close_status: "CLOSED",
                version: 6,
                created_at: 1,
                updated_at: 1,
                stage: {
                    code: "closed",
                    label: "已关闭",
                    tone: "void",
                },
            },
            {
                amountGross: "100.00",
                receivedAmount: "100.00",
                invoicedAmount: "100.00",
            },
        )

        expect(order.receivedAmount).toBe("100.00")
        expect(order.invoicedAmount).toBe("100.00")
        expect(order.allowedActions).not.toContain("REGISTER_ACCEPTANCE")
    })
})
