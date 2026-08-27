import { describe, expect, it } from "vitest"

import {
    paymentSchema,
    withLockedPaymentAmount,
} from "@/features/supplier-payables/lib/allocation-model"

const validPayment = {
    paidAt: "2026-08-27T18:11",
    amount: "1280.00",
    bankReference: "",
    bankReceiptAssetId: "asset-receipt-1",
    bankReceipt: null,
    note: "",
}

describe("paymentSchema", () => {
    it("requires a bank receipt while keeping the bank reference optional", () => {
        expect(paymentSchema.safeParse(validPayment).success).toBe(true)

        const missingReceipt = paymentSchema.safeParse({
            ...validPayment,
            bankReceiptAssetId: "",
        })
        expect(missingReceipt.success).toBe(false)
        if (missingReceipt.success) throw new Error("银行回单缺失时必须失败")
        expect(
            missingReceipt.error.issues.some(
                (issue) =>
                    issue.path.join(".") === "bankReceipt" &&
                    issue.message === "请上传银行回单图片",
            ),
        ).toBe(true)
    })

    it("rejects a bank reference longer than the backend contract", () => {
        const result = paymentSchema.safeParse({
            ...validPayment,
            bankReference: "x".repeat(257),
        })
        expect(result.success).toBe(false)
    })
})

describe("withLockedPaymentAmount", () => {
    it("uses the payment amount as the locked payable allocation", () => {
        expect(
            withLockedPaymentAmount(
                { payableA: "1280.00", payableB: "50.00" },
                "payableA",
                "1000.00",
            ),
        ).toEqual({ payableA: "1000.00", payableB: "50.00" })
    })

    it("keeps the original allocation map when no payable is locked", () => {
        const amounts = { payableA: "1280.00" }
        expect(withLockedPaymentAmount(amounts, undefined, "1000.00")).toBe(
            amounts,
        )
    })
})
