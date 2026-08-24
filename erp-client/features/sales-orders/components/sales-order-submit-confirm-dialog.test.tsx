import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vitest"

import { SalesOrderSubmitConfirmDialog } from "./sales-order-submit-confirm-dialog"
import { VoucherSalesOrderSubmitConfirmDialog } from "./voucher-sales-order-submit-confirm-dialog"
import type { SalesOrderSubmitSnapshot } from "./sales-order-submit-confirm-summary"

afterEach(() => {
    cleanup()
})

const physicalSnapshot: SalesOrderSubmitSnapshot = {
    customerName: "测试客户",
    contractLabel: "HT001@v1",
    settlementEntity: "上海主体",
    nature: "physical_service",
    welfareScene: "festival_gift",
    paymentTerms: "contract",
    fulfillmentMode: "warehouse_delivery",
    taxRatePercent: "13",
    lineCount: 2,
    amountGross: "1130.00",
    amountNet: "1000.00",
    amountTax: "130.00",
}

const voucherSnapshot: SalesOrderSubmitSnapshot = {
    ...physicalSnapshot,
    nature: "card_voucher",
    fulfillmentMode: "",
}

describe("SalesOrderSubmitConfirmDialog", () => {
    it("opens as a landscape confirm dialog with summary only", () => {
        render(
            <SalesOrderSubmitConfirmDialog
                open
                pending={false}
                snapshot={physicalSnapshot}
                onOpenChange={() => undefined}
                onConfirm={() => undefined}
            />,
        )

        const dialog = screen.getByRole("alertdialog")
        expect(dialog.className).toContain("sm:max-w-4xl")
        expect(screen.getByText("确认提交销售单")).toBeTruthy()
        expect(screen.getByLabelText("状态变化")).toBeTruthy()
        expect(screen.getByText("测试客户")).toBeTruthy()
        expect(screen.getByLabelText("本单摘要")).toBeTruthy()
        expect(screen.queryByText("提交后将启动审批")).toBeNull()
    })
})

describe("VoucherSalesOrderSubmitConfirmDialog", () => {
    it("uses the same landscape confirm shell without the approval card", () => {
        render(
            <VoucherSalesOrderSubmitConfirmDialog
                open
                pending={false}
                snapshot={voucherSnapshot}
                onOpenChange={() => undefined}
                onConfirm={() => undefined}
            />,
        )

        const dialog = screen.getByRole("alertdialog")
        expect(dialog.className).toContain("sm:max-w-4xl")
        expect(screen.getByText("确认提交销售单")).toBeTruthy()
        expect(screen.getByLabelText("本单摘要")).toBeTruthy()
        expect(screen.queryByText("提交后将启动审批")).toBeNull()
    })
})
