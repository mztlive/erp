import { describe, expect, it } from "vitest"

import {
    createSupplierEditorDefaults,
    type SupplierEditorFormValues,
    validateSupplierEditorFields,
} from "./supplier-editor-model"

const validValues = (): SupplierEditorFormValues => ({
    ...createSupplierEditorDefaults(true),
    name: "云桦有礼",
    company: "云桦有礼有限公司",
    signingEntity: "party-signing",
    paymentEntity: "party-payment",
    settlement: "pay_after_use",
    paymentTerm: "POSTPAY_NET30",
})

describe("validateSupplierEditorFields", () => {
    it("accepts a concrete term matching its settlement mode", () => {
        expect(validateSupplierEditorFields(validValues())).toBeNull()
    })

    it("requires a concrete payment term", () => {
        expect(
            validateSupplierEditorFields({
                ...validValues(),
                paymentTerm: "先用后付",
            }),
        ).toBe("请选择具体付款条件")
    })

    it("rejects a payment term from another settlement mode", () => {
        expect(
            validateSupplierEditorFields({
                ...validValues(),
                paymentTerm: "PREPAY_30",
            }),
        ).toBe("结算方式与付款条件不一致，请重新选择")
    })
})
