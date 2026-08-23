import { describe, expect, it } from "vitest"

import {
    mapFulfillmentModeFromBackend,
    mapListItemFromBackend,
    mapWorkingCopyLines,
} from "@/features/sales-orders/lib/sales-order-detail-mappers"
import {
    canCreatePurchaseFromSalesOrder,
    purchaseOrdersWorkspaceHref,
} from "@/features/sales-orders/lib/sales-order-detail-model"

describe("mapFulfillmentModeFromBackend", () => {
    it("maps backend fulfillment codes to Chinese labels", () => {
        expect(mapFulfillmentModeFromBackend("SUPPLIER_DIRECT")).toBe(
            "供应商直发",
        )
        expect(mapFulfillmentModeFromBackend("COMPANY_WAREHOUSE")).toBe(
            "公司仓发",
        )
        expect(mapFulfillmentModeFromBackend("ELECTRONIC_DELIVERY")).toBe(
            "电子交付",
        )
        expect(mapFulfillmentModeFromBackend("OFFLINE_SERVICE")).toBe(
            "线下服务",
        )
        expect(mapFulfillmentModeFromBackend(null)).toBe("")
    })
})

describe("mapWorkingCopyLines", () => {
    it("maps goods-line fulfillment mode and due date", () => {
        const lines = mapWorkingCopyLines([
            {
                id: "line-1",
                sales_order_line_id: "sol-1",
                line_no: 1,
                line_type: "GOODS_SERVICE",
                gross_amount: "100.00",
                net_amount: "88.50",
                tax_amount: "11.50",
                sales_tax_rate: "0.130000",
                item_name_snapshot: "测试商品",
                quantity: "1",
                base_unit_code: "件",
                unit_price_gross: "100.00",
                fulfillment_mode: "SUPPLIER_DIRECT",
                fulfillment_due_at: 1_800_000_000,
            },
        ])

        expect(lines).toHaveLength(1)
        expect(lines[0]?.fulfillmentMode).toBe("供应商直发")
        expect(lines[0]?.dueDate).toMatch(/^\d{4}-\d{2}-\d{2}/)
    })

    it("derives voucher gift rate and does not expose a stable internal sku id", () => {
        const lines = mapWorkingCopyLines([
            {
                id: "line-2",
                sales_order_line_id: "sol-2",
                line_no: 1,
                line_type: "VOUCHER",
                gross_amount: "180.00",
                net_amount: "169.81",
                tax_amount: "10.19",
                sales_tax_rate: "0.060000",
                item_name_snapshot: "节日卡券",
                spec_snapshot: "sku_internal_1",
                sku_id: "sku_internal_1",
                card_count: 2,
                unit_price_gross: "90.00",
                face_value: "100.00",
                card_form: "ELECTRONIC",
            },
        ])

        expect(lines[0]?.sku).toBeUndefined()
        expect(lines[0]?.unitPriceGross).toBe("90.00")
        expect(lines[0]?.giftRate).toBe("11.11")
        expect(lines[0]?.cardForm).toBe("电子卡")
    })
})

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
                owner_user_id: "u-sales",
                owner_user_name: "张三",
                stage: {
                    code: "effective",
                    label: "已生效",
                    tone: "success",
                },
            },
            { purchaseOrderCount: 2 },
        )

        expect(order.ownerUserId).toBe("u-sales")
        expect(order.ownerName).toBe("张三")
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

    it("does not allow customer acceptance while the order is still pending review", () => {
        const pending = mapListItemFromBackend({
            id: "so-pending",
            order_no: "XS202608230003",
            business_type: "GOODS_SERVICE",
            origin_system: "ERP",
            customer_id: "customer-1",
            commercial_status: "PENDING_REVIEW",
            review_status: "IN_APPROVAL",
            fulfillment_progress: "NOT_STARTED",
            collection_progress: "NOT_STARTED",
            invoice_progress: "NOT_STARTED",
            close_status: "OPEN",
            version: 1,
            created_at: 1,
            updated_at: 1,
            stage: {
                code: "in_approval",
                label: "审批中",
                tone: "warning",
            },
        })
        expect(pending.allowedActions).not.toContain("REGISTER_ACCEPTANCE")

        const effective = mapListItemFromBackend({
            id: "so-effective",
            order_no: "XS202608230004",
            business_type: "GOODS_SERVICE",
            origin_system: "ERP",
            customer_id: "customer-1",
            commercial_status: "EFFECTIVE",
            review_status: "APPROVED",
            fulfillment_progress: "NOT_STARTED",
            collection_progress: "NOT_STARTED",
            invoice_progress: "NOT_STARTED",
            close_status: "OPEN",
            version: 2,
            created_at: 1,
            updated_at: 1,
            stage: {
                code: "effective",
                label: "已生效",
                tone: "success",
            },
        })
        expect(effective.allowedActions).toContain("REGISTER_ACCEPTANCE")
    })
})
