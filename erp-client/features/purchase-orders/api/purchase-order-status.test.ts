import { describe, expect, it } from "vitest"

import { parsePaymentTermSnapshot } from "./purchase-order-status"

describe("parsePaymentTermSnapshot", () => {
    it("splits settlement from business category in supplier snapshots", () => {
        expect(parsePaymentTermSnapshot("先用后付 | 经营类目：礼盒")).toEqual({
            paymentTerm: "先用后付",
            businessCategory: "礼盒",
        })
        expect(parsePaymentTermSnapshot("现结｜经营类目：礼盒")).toEqual({
            paymentTerm: "现结",
            businessCategory: "礼盒",
        })
    })

    it("maps known payment term codes without a category", () => {
        expect(parsePaymentTermSnapshot("POSTPAY_NET30")).toEqual({
            paymentTerm: "货到 30 天",
            businessCategory: "",
        })
    })
})
