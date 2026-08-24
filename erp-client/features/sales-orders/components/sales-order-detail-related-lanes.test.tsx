import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import type { SalesOrderDetailView } from "@/features/sales-orders/api/contracts"
import { RelatedLanes } from "@/features/sales-orders/components/sales-order-detail-related-lanes"
import { mapListItemFromBackend } from "@/features/sales-orders/lib/sales-order-detail-mappers"

vi.mock(
    "@/features/sales-orders/hooks/use-sales-order-detail-permissions",
    () => ({
        useSalesOrderDetailPermissions: () => ({
            createPurchase: () => ({ enabled: true }),
            openPurchase: { enabled: true },
            openFulfillment: { enabled: true },
            openReceivable: { enabled: true },
        }),
    }),
)

function makeOrder(
    purchaseOrders: number,
    purchaseCreationAllowed = true,
): SalesOrderDetailView {
    return mapListItemFromBackend(
        {
            id: "so-1",
            order_no: "XS202608240001",
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
            stage: { code: "effective", label: "已生效", tone: "success" },
            purchase_coverage: {
                total_quantity: "10",
                covered_quantity: purchaseOrders > 0 ? "4" : "0",
                remaining_quantity: purchaseOrders > 0 ? "6" : "10",
                progress: purchaseOrders > 0 ? "0.4" : "0",
            },
        },
        {
            purchaseOrderCount: purchaseOrders,
            purchaseCreationAccess: {
                allowed: purchaseCreationAllowed,
                taskCount: purchaseCreationAllowed ? 1 : 0,
                blocker: purchaseCreationAllowed
                    ? undefined
                    : "当前账号不是该销售单待采购任务负责人",
            },
        },
    ) as SalesOrderDetailView
}

afterEach(cleanup)

describe("RelatedLanes procurement progress", () => {
    it("shows progress and the first purchase action", () => {
        render(
            <RelatedLanes
                order={makeOrder(0)}
                selfReturn="/sales/orders/so-1"
                lanes={["purchase"]}
            />,
        )

        expect(
            screen
                .getByTestId("sales-order-procurement-progress")
                .textContent?.includes("销售总数量 10 · 已覆盖 0 · 剩余 10"),
        ).toBe(true)
        expect(
            screen
                .getByTestId("sales-order-continue-purchase")
                .textContent?.includes("去建单"),
        ).toBe(true)
    })

    it("shows continue purchase when prior purchase orders exist", () => {
        render(
            <RelatedLanes
                order={makeOrder(2)}
                selfReturn="/sales/orders/so-1"
                lanes={["purchase"]}
            />,
        )

        expect(
            screen
                .getByTestId("sales-order-continue-purchase")
                .textContent?.includes("继续建单"),
        ).toBe(true)
    })

    it("does not show the create action to a procurement user who does not own the task", () => {
        render(
            <RelatedLanes
                order={makeOrder(0, false)}
                selfReturn="/sales/orders/so-1"
                lanes={["purchase"]}
            />,
        )

        expect(screen.queryByTestId("sales-order-continue-purchase")).toBeNull()
        expect(screen.getByRole("button", { name: "打开采购" })).not.toBeNull()
    })
})
