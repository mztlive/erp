import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"

import { PROCUREMENT_PURCHASE_RETURN_FORBIDDEN_ACTIONS } from "@/app/(workspace)/procurement/orders/purchase-return-page-proof"
import { purchaseReturnActionsExcludeApproval } from "@/features/purchase-orders/lib/purchase-return-order-no-approval"
import type { PurchaseReturnOrderRow } from "@/features/purchase-orders/types"
import { PurchaseOrderDetailChangesSection } from "./purchase-order-detail-changes-section"
import {
    PurchaseReturnOrderRelatedSection,
    PurchaseReturnOrderSection,
} from "./purchase-return-order-section"
import { makePurchaseOrderCenter } from "@/features/purchase-orders/hooks/use-purchase-order-detail-fixtures"

vi.mock(
    "@/features/purchase-orders/hooks/use-purchase-return-orders-query",
    () => ({
        usePurchaseReturnOrdersQuery: () => ({
            data: [
                {
                    purchaseReturnOrderId: "pro-1",
                    purchaseReturnNo: "TH-2026-001",
                    purchaseOrderId: "po-1",
                    returnMode: "company_warehouse_to_supplier",
                    returnModeLabel: "公司仓退供应商",
                    status: "PENDING_EXECUTION",
                    statusLabel: "待执行",
                    statusTone: "warning",
                    version: 1,
                    createdAt: "2026-08-01T08:00:00.000Z",
                    allowedActions: ["VIEW_DETAIL"],
                },
            ],
            isPending: false,
            isError: false,
            isSuccess: true,
            refetch: vi.fn(),
        }),
    }),
)

afterEach(() => {
    cleanup()
})

const pendingExecutionRow = (): PurchaseReturnOrderRow => ({
    purchaseReturnOrderId: "pro-1",
    purchaseReturnNo: "TH-2026-001",
    purchaseOrderId: "po-1",
    returnMode: "company_warehouse_to_supplier",
    returnModeLabel: "公司仓退供应商",
    status: "PENDING_EXECUTION",
    statusLabel: "待执行",
    statusTone: "warning",
    version: 1,
    createdAt: "2026-08-01T08:00:00.000Z",
    allowedActions: ["VIEW_DETAIL"],
})

function expectNoApprovalUi() {
    expect(screen.queryByText("审批流程")).toBeNull()
    expect(screen.queryByText("尚未绑定审批流程")).toBeNull()
    expect(screen.queryByText("当前审批人")).toBeNull()
    expect(screen.queryByText("审批中")).toBeNull()
    expect(screen.queryByText("审批复核")).toBeNull()
    expect(screen.queryByRole("button", { name: "选择流程" })).toBeNull()
    expect(screen.queryByRole("button", { name: "通过" })).toBeNull()
    expect(screen.queryByRole("button", { name: "驳回" })).toBeNull()
    expect(screen.queryByRole("button", { name: "撤回审批" })).toBeNull()
    expect(screen.queryByRole("button", { name: "改派当前审批人" })).toBeNull()
    expect(screen.queryByRole("button", { name: "恢复当前审批人" })).toBeNull()
    expect(screen.queryByRole("button", { name: "取消受阻审批" })).toBeNull()
    expect(
        screen.queryByRole("button", { name: "更新审批流程版本" }),
    ).toBeNull()
    for (const label of PROCUREMENT_PURCHASE_RETURN_FORBIDDEN_ACTIONS) {
        expect(screen.queryByRole("button", { name: label })).toBeNull()
    }
}

describe("PurchaseReturnOrderSection", () => {
    it("prints return facts and does not render PENDING_EXECUTION as review", () => {
        render(<PurchaseReturnOrderSection returns={[pendingExecutionRow()]} />)
        expect(screen.getByText(/TH-2026-001/)).toBeTruthy()
        expect(screen.getByText("待执行")).toBeTruthy()
        expect(screen.getByText(/公司仓退供应商/)).toBeTruthy()
        expectNoApprovalUi()
        expect(
            purchaseReturnActionsExcludeApproval(
                pendingExecutionRow().allowedActions,
            ),
        ).toBe(true)
    })

    it("shows an empty business state without process selection", () => {
        render(<PurchaseReturnOrderSection returns={[]} />)
        expect(screen.getByText("暂无采购退货。")).toBeTruthy()
        expectNoApprovalUi()
    })
})

describe("purchase return detail and related section", () => {
    it("loads related returns on the purchase order changes section", () => {
        const client = new QueryClient({
            defaultOptions: { queries: { retry: false } },
        })
        render(
            <QueryClientProvider client={client}>
                <PurchaseOrderDetailChangesSection
                    order={makePurchaseOrderCenter()}
                    canChange={false}
                    changeBlocker={undefined}
                    onRequestChange={vi.fn()}
                />
            </QueryClientProvider>,
        )
        expect(screen.getByText("采购退货")).toBeTruthy()
        expect(screen.getByText(/TH-2026-001/)).toBeTruthy()
        expect(screen.getByText("待执行")).toBeTruthy()
        expectNoApprovalUi()
    })

    it("keeps the related section on the no-approval path", () => {
        const client = new QueryClient({
            defaultOptions: { queries: { retry: false } },
        })
        render(
            <QueryClientProvider client={client}>
                <PurchaseReturnOrderRelatedSection purchaseOrderId="po-1" />
            </QueryClientProvider>,
        )
        expect(screen.getByText("采购退货")).toBeTruthy()
        expect(screen.getByText("待执行")).toBeTruthy()
        expectNoApprovalUi()
    })
})
