import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vitest"

import { SALES_ORDERS_SALES_RETURN_FORBIDDEN_ACTIONS } from "@/app/(workspace)/sales/orders/sales-return-page-proof"
import { mapSalesReturnCase } from "@/features/sales-orders/api/sales-return-cases"
import type { BackendSalesReturnCase } from "@/features/sales-orders/api/sales-return-cases"
import { salesReturnCaseActionsExcludeApproval } from "@/features/sales-orders/lib/sales-return-no-approval"
import { SalesReturnCaseFacts } from "./sales-return-case-facts"

afterEach(() => {
    cleanup()
})

const seed = (
    status: string,
    extras: Partial<BackendSalesReturnCase> = {},
): BackendSalesReturnCase => ({
    id: "src-1",
    return_no: "XT-2026-001",
    sales_order_id: "so-1",
    case_type: "return",
    reason: "客户拒收部分到货",
    discovered_at: 1_700_000_000,
    return_route: "company_warehouse",
    status,
    version: 1,
    created_at: 1_700_000_000,
    lines: [
        {
            id: "srl-1",
            sales_order_line_id: "sol-1",
            requested_quantity: "2",
        },
    ],
    ...extras,
})

function expectNoApprovalUi() {
    expect(screen.queryByText("审批流程")).toBeNull()
    expect(screen.queryByText("尚未绑定审批流程")).toBeNull()
    expect(screen.queryByText("当前审批人")).toBeNull()
    expect(screen.queryByText("审批复核")).toBeNull()
    expect(screen.queryByText("待审批")).toBeNull()
    expect(screen.queryByText("审批中")).toBeNull()
    expect(screen.queryByText("待财务复核")).toBeNull()
    expect(screen.queryByText("待仓储复核")).toBeNull()
    expect(screen.queryByText("待采购复核")).toBeNull()
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
    for (const label of SALES_ORDERS_SALES_RETURN_FORBIDDEN_ACTIONS) {
        expect(screen.queryByRole("button", { name: label })).toBeNull()
    }
}

describe("SalesReturnCaseFacts", () => {
    it("prints return facts and does not render the approval zone", () => {
        render(
            <SalesReturnCaseFacts
                row={mapSalesReturnCase(seed("pending_warehouse_acceptance"))}
            />,
        )
        expect(screen.getByText("XT-2026-001")).toBeTruthy()
        expect(screen.getByText("退货")).toBeTruthy()
        expect(screen.getByText("退公司仓")).toBeTruthy()
        expect(screen.getByText("待仓储验收")).toBeTruthy()
        expect(screen.getByText("客户拒收部分到货")).toBeTruthy()
        expectNoApprovalUi()
        expect(
            salesReturnCaseActionsExcludeApproval(
                mapSalesReturnCase(seed("pending_warehouse_acceptance"))
                    .allowedActions,
            ),
        ).toBe(true)
    })

    it("renders PENDING_PROCUREMENT as fulfillment handling, not approval review", () => {
        render(
            <SalesReturnCaseFacts
                row={mapSalesReturnCase(seed("PENDING_PROCUREMENT"))}
            />,
        )
        expect(screen.getByText("待采购处理")).toBeTruthy()
        expectNoApprovalUi()
    })

    it("renders PENDING_FINANCE as finance handling, not approval review", () => {
        render(
            <SalesReturnCaseFacts
                row={mapSalesReturnCase(seed("PENDING_FINANCE"))}
            />,
        )
        expect(screen.getByText("待财务处理")).toBeTruthy()
        expectNoApprovalUi()
    })
})
