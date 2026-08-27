import { describe, expect, it } from "vitest"

import {
    paymentTermCode,
    paymentTermMatchesSettlement,
    supplierPaymentTermOptionsFor,
} from "@/lib/business-options"

describe("supplier payment term options", () => {
    it("normalizes supported historical aliases to stable codes", () => {
        expect(paymentTermCode(" NET-30 ")).toBe("POSTPAY_NET30")
        expect(paymentTermCode("现结")).toBe("CASH_ON_APPROVAL")
        expect(paymentTermCode("预付款")).toBe("PREPAY_100")
    })

    it("does not treat a broad settlement label as a payment term", () => {
        expect(paymentTermCode("先用后付")).toBeUndefined()
        expect(paymentTermCode("默认付款条件")).toBeUndefined()
    })

    it("limits payment terms to the selected settlement mode", () => {
        expect(
            supplierPaymentTermOptionsFor("pay_after_use").map(
                (option) => option.value,
            ),
        ).toEqual(["POSTPAY_NET15", "POSTPAY_NET30"])
        expect(
            supplierPaymentTermOptionsFor("cash_settlement").map(
                (option) => option.value,
            ),
        ).toEqual(["CASH_ON_APPROVAL"])
        expect(
            paymentTermMatchesSettlement("POSTPAY_NET30", "pay_after_use"),
        ).toBe(true)
        expect(paymentTermMatchesSettlement("NET-30", "先用后付")).toBe(true)
        expect(paymentTermMatchesSettlement("PREPAY_30", "pay_after_use")).toBe(
            false,
        )
    })
})
