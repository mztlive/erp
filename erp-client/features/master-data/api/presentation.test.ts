import { describe, expect, it } from "vitest"

import { paymentTermSnapshotOf, settlementToBackend } from "./presentation"

describe("supplier commercial presentation", () => {
    it("maps settlement labels independently from concrete payment terms", () => {
        expect(settlementToBackend("先用后付")).toBe("pay_after_use")
        expect(paymentTermSnapshotOf("货到 30 天")).toBe("POSTPAY_NET30")
    })

    it("rejects missing or ambiguous payment terms", () => {
        expect(() => paymentTermSnapshotOf("先用后付")).toThrow(
            "请选择具体付款条件",
        )
        expect(() => paymentTermSnapshotOf("按合同约定")).toThrow(
            "请选择具体付款条件",
        )
        expect(() => settlementToBackend(undefined)).toThrow("请选择结算方式")
    })
})
