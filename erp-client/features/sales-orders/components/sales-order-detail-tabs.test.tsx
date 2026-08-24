import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import type { SalesOrderDetailView } from "@/features/sales-orders/api/contracts"
import { mapListItemFromBackend } from "@/features/sales-orders/lib/sales-order-detail-mappers"
import { SalesOrderDetailTabs } from "./sales-order-detail-tabs"

vi.mock(
    "@/features/sales-orders/hooks/use-sales-order-detail-permissions",
    () => ({
        useSalesOrderDetailPermissions: () => ({
            createPurchase: () => ({ enabled: true }),
            openPurchase: { enabled: true },
        }),
    }),
)

vi.mock("@/features/sales-orders/components/sales-order-detail-panels", () => ({
    OverviewPanel: () => <div>销售概览</div>,
    FulfillmentPanel: () => <div>履约</div>,
    CollaborationPanel: () => <div>协同</div>,
    ReceivablePanel: () => <div>票款</div>,
    VersionsPanel: () => <div>版本</div>,
}))

vi.mock(
    "@/features/sales-orders/components/sales-order-detail-approval-panel",
    () => ({ ApprovalPanel: () => <div>审批</div> }),
)

function makeOrder(): SalesOrderDetailView {
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
                covered_quantity: "4",
                remaining_quantity: "6",
                progress: "0.4",
            },
        },
        {
            purchaseOrderCount: 1,
            purchaseCreationAccess: {
                allowed: true,
                taskCount: 1,
            },
        },
    ) as SalesOrderDetailView
}

afterEach(cleanup)

describe("SalesOrderDetailTabs procurement integration", () => {
    it("mounts procurement progress and continue-purchase action in the real overview tree", () => {
        render(
            <SalesOrderDetailTabs
                order={makeOrder()}
                selfReturn="/sales/orders/so-1"
                navSection="overview"
                visibleNav={[
                    {
                        id: "overview",
                        label: "概览",
                        hint: "约定、明细和下一步",
                        show: true,
                    },
                ]}
                canAccept={false}
                acceptanceExpanded={false}
                onSelectSection={vi.fn()}
                onApprovalResult={vi.fn()}
                onDataChanged={vi.fn()}
            />,
        )

        expect(screen.getByText("采购进度")).toBeTruthy()
        expect(
            screen
                .getByTestId("sales-order-procurement-progress")
                .textContent?.includes("销售总数量 10 · 已覆盖 4 · 剩余 6"),
        ).toBe(true)
        expect(
            screen
                .getByTestId("sales-order-continue-purchase")
                .textContent?.includes("继续建单"),
        ).toBe(true)
    })
})
