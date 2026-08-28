import type { ReactElement } from "react"
import { QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { SupplierPaymentDetailDialog } from "@/features/supplier-payables/components/supplier-payment-detail-dialog"
import { createFreshQueryClient } from "@/features/test-utils"
import type { PaymentRow } from "@/features/supplier-payables/types"

const payment: PaymentRow = {
    paymentId: "pay-1",
    paymentNo: "FK-1",
    supplierId: "sup-1",
    supplierName: "华东供应商",
    paidAt: "2026-01-01T00:00:00.000Z",
    amount: "10.00",
    bankReferenceMasked: "****1234",
    allocatedTotal: "10.00",
    unallocatedAmount: "0.00",
    status: "POSTED",
    statusLabel: "已过账",
    statusTone: "success",
    baselineVersion: 1,
    allocations: [],
    allowedActions: [],
    actionBlockers: [],
    relatedReversals: [],
}

function renderDialog(ui: ReactElement) {
    const client = createFreshQueryClient()
    return render(
        <QueryClientProvider client={client}>{ui}</QueryClientProvider>,
    )
}

afterEach(cleanup)

describe("SupplierPaymentDetailDialog", () => {
    it("以分区 Dialog 展示付款详情，取消会通知上层", () => {
        const onOpenChange = vi.fn()
        renderDialog(
            <SupplierPaymentDetailDialog
                open
                onOpenChange={onOpenChange}
                isPending={false}
                isError={false}
                error={null}
                onRetry={() => undefined}
                row={payment}
            />,
        )

        expect(screen.getByRole("heading", { name: "付款详情" })).toBeTruthy()
        expect(
            screen.getByText("查看付款记录、收款信息、银行回单与核销明细。"),
        ).toBeTruthy()
        expect(screen.getByRole("tab", { name: "基本信息" })).toBeTruthy()
        expect(screen.getByDisplayValue("FK-1")).toBeTruthy()

        fireEvent.click(screen.getByRole("button", { name: "取消" }))
        expect(onOpenChange).toHaveBeenCalledWith(false)
    })
})
