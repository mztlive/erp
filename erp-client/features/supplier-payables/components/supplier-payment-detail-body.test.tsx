import type { ReactElement } from "react"
import { QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { SupplierPaymentDetailBody } from "@/features/supplier-payables/components/supplier-payment-detail-body"
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
    allocations: [
        {
            allocationId: "alloc-1",
            action: "APPLY",
            payableAccountId: "pa-1",
            payableEntryId: "pe-1",
            sourceType: "PURCHASE_ORDER",
            sourceDocumentId: "po-1",
            sourceDocumentNo: "PO-1001",
            sourceHref: "/procurement/orders/po-1",
            payableHref:
                "/finance/supplier-accounts?view=payable&detailId=pa-1&previewKind=payable",
            amount: "10.00",
            occurredAt: "2026-01-01T00:00:00.000Z",
        },
    ],
    allowedActions: [],
    actionBlockers: [],
    paymentRecipient: {
        bankAccountId: "ba-1",
        version: 1,
        accountName: "上海示例供应商有限公司",
        bankName: "招商银行",
        bankBranchName: "上海分行营业部",
        accountNumberMasked: "6222********88881234",
    },
    relatedReversals: [],
}

/**
 * 带 Query 壳渲染付款详情，银行回单查询在无回单时不会发请求。
 */
function renderBody(ui: ReactElement) {
    const client = createFreshQueryClient()
    return render(
        <QueryClientProvider client={client}>{ui}</QueryClientProvider>,
    )
}

afterEach(cleanup)

describe("SupplierPaymentDetailBody", () => {
    it("默认展示基本信息分区，并用业务用语展示金额", () => {
        renderBody(<SupplierPaymentDetailBody row={payment} />)

        expect(screen.getByRole("tab", { name: "基本信息" })).toBeTruthy()
        expect(screen.getByRole("tab", { name: "收款信息" })).toBeTruthy()
        expect(screen.getByRole("tab", { name: "付款去向" })).toBeTruthy()
        expect(screen.getAllByText("已付款").length).toBeGreaterThan(0)
        expect(screen.getAllByText("未付款").length).toBeGreaterThan(0)
        expect(screen.queryByText("净已分配")).toBeNull()
        expect(screen.queryByText("分配明细（新增不覆盖原金额）")).toBeNull()
        expect(screen.queryByText("6222********88881234")).toBeNull()
        expect(
            screen.queryByText(
                "这笔付款分别付给了哪些应付、各付了多少。点来源单进入采购单或结算单。冲正不会改已经记下的行，只会再记一笔冲减。",
            ),
        ).toBeNull()
    })

    it("收款信息分区完整显示收款账号", () => {
        renderBody(<SupplierPaymentDetailBody row={payment} />)

        fireEvent.click(screen.getByRole("tab", { name: "收款信息" }))
        expect(screen.getByDisplayValue("6222********88881234")).toBeTruthy()
    })

    it("付款去向分区展示核销行，并可跳转到来源采购单", () => {
        renderBody(<SupplierPaymentDetailBody row={payment} />)

        fireEvent.click(screen.getByRole("tab", { name: "付款去向" }))
        expect(
            screen.getByText(
                "这笔付款分别付给了哪些应付、各付了多少。点来源单进入采购单或结算单。冲正不会改已经记下的行，只会再记一笔冲减。",
            ),
        ).toBeTruthy()
        expect(screen.getByText(/付给 采购单 PO-1001/)).toBeTruthy()
        const sourceButton = screen.getByRole("button", { name: "打开采购单" })
        expect(sourceButton.getAttribute("href")).toBe(
            "/procurement/orders/po-1",
        )
    })

    it("付款去向在缺少 sourceHref 时仍能按来源身份跳转结算单", () => {
        renderBody(
            <SupplierPaymentDetailBody
                row={{
                    ...payment,
                    allocations: [
                        {
                            allocationId: "alloc-2",
                            action: "APPLY",
                            payableAccountId: "pa-2",
                            payableEntryId: "pe-2",
                            sourceType: "SUPPLIER_SETTLEMENT",
                            sourceDocumentId: "st-9",
                            sourceDocumentNo: "JS-9",
                            amount: "10.00",
                            occurredAt: "2026-01-01T00:00:00.000Z",
                        },
                    ],
                }}
            />,
        )

        fireEvent.click(screen.getByRole("tab", { name: "付款去向" }))
        const sourceButton = screen.getByRole("button", { name: "打开结算单" })
        expect(sourceButton.getAttribute("href")).toBe(
            "/supplier-api/settlements/st-9",
        )
    })

    it("关联单据不重复列出采购单，查看应付在当前页打开", () => {
        const onOpenPayable = vi.fn()
        renderBody(
            <SupplierPaymentDetailBody
                row={payment}
                onOpenPayable={onOpenPayable}
            />,
        )

        fireEvent.click(screen.getByRole("tab", { name: "关联单据" }))
        expect(
            screen.getAllByRole("button", { name: "查看应付" }),
        ).toHaveLength(1)
        const sourceButton = screen
            .getAllByRole("button", { name: "打开采购单" })
            .find(
                (el) => el.getAttribute("href") === "/procurement/orders/po-1",
            )
        expect(sourceButton).toBeTruthy()
        expect(sourceButton?.getAttribute("href") ?? "").not.toContain(
            "view=payable",
        )

        fireEvent.click(screen.getByRole("button", { name: "查看应付" }))
        expect(onOpenPayable).toHaveBeenCalledWith("pa-1")
    })
})
