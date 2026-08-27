import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { SupplierPaymentSubmitConfirmDialog } from "@/features/supplier-payables/components/supplier-payment-submit-confirm-dialog"

describe("SupplierPaymentSubmitConfirmDialog", () => {
    it("confirms direct posting against the locked payment recipient", () => {
        render(
            <SupplierPaymentSubmitConfirmDialog
                open
                pending={false}
                supplierName="上海示例供应商"
                paymentAmount="1280.00"
                allocatedAmount="1280.00"
                recipient={{
                    bankAccountId: "bank-account-1",
                    version: 3,
                    accountName: "上海示例供应商有限公司",
                    bankName: "招商银行",
                    bankBranchName: "上海分行营业部",
                    accountNumberMasked: "********1234",
                }}
                onOpenChange={() => undefined}
                onConfirm={() => undefined}
            />,
        )

        expect(
            screen.getByRole("heading", {
                name: "确认登记付款并核销",
            }),
        ).toBeTruthy()
        expect(screen.getByText("收款户名 上海示例供应商有限公司")).toBeTruthy()
        expect(
            screen.getByText("开户行 招商银行 · 上海分行营业部"),
        ).toBeTruthy()
        expect(screen.getByText("收款账号 ********1234")).toBeTruthy()
        expect(screen.getByText("付款金额 1280.00")).toBeTruthy()
        expect(screen.getByText("已过账")).toBeTruthy()
        expect(
            screen.getByText(
                "形成已过账付款记录；纠错须走付款冲正或供应商退款",
            ),
        ).toBeTruthy()
        expect(screen.queryByText(/正式付款事实/)).toBeNull()
        expect(screen.getByRole("button", { name: "确认付款" })).toBeTruthy()
        expect(screen.queryByRole("button", { name: "确认提交" })).toBeNull()
    })
})
