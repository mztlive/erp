import { describe, it, expect } from "vitest"

import {
    decimalAtMost,
    decimalInput,
    validateSalesOrderContractId,
    validateSalesOrderForm,
    type CreateSalesOrderFormValues,
} from "@/features/sales-orders/lib/sales-order-create-validation"

const makeValues = (
    overrides: Partial<CreateSalesOrderFormValues> = {},
): CreateSalesOrderFormValues => ({
    contractId: "ct-1",
    requestedContractRevisionId: "r-1",
    contractRevisionLabel: "CT-1@v1",
    customerId: "cu-1",
    customerName: "客户甲",
    settlementPartyId: "sp-1",
    settlementEntity: "结算主体甲",
    nature: "physical_service",
    ownerUserId: "u-1",
    ownerName: "张三",
    welfareScene: "ANNUAL_GIFT_BAG",
    paymentTerms: "POSTPAY_NET30",
    fulfillmentDeadline: "2026-09-30",
    targetMallId: "",
    receivableDueDate: "",
    taxRatePercent: "13.00",
    remark: "",
    lineItems: [
        {
            rowKey: "l1",
            name: "货物",
            sku: "sku-1",
            skuRevisionId: "sr-1",
            quantity: "1",
            unit: "件",
            unitPriceGross: "100.00",
            fulfillmentMode: "公司仓发",
            dueDate: "2026-09-01",
            faceValue: "",
            giftRate: "",
            cardForm: "",
        },
    ],
    ...overrides,
})

describe("decimalInput", () => {
    const schema = decimalInput("数量", 2, { positive: true })

    it("accepts integers and decimals within the scale", () => {
        expect(schema.safeParse("1").success).toBe(true)
        expect(schema.safeParse("1.5").success).toBe(true)
        expect(schema.safeParse("10.25").success).toBe(true)
    })

    it("rejects more decimals than the allowed scale", () => {
        const result = schema.safeParse("1.234")
        expect(result.success).toBe(false)
    })

    it("rejects non-numeric input", () => {
        expect(schema.safeParse("abc").success).toBe(false)
        expect(schema.safeParse("-1").success).toBe(false)
        expect(schema.safeParse("1e3").success).toBe(false)
    })

    it("rejects zero when positive is required", () => {
        expect(schema.safeParse("0").success).toBe(false)
        expect(schema.safeParse("0.00").success).toBe(false)
        expect(schema.safeParse("0.01").success).toBe(true)
    })

    it("allows zero when positive is not required", () => {
        expect(decimalInput("金额", 2).safeParse("0").success).toBe(true)
    })
})

describe("decimalAtMost", () => {
    it("compares within the scale", () => {
        expect(decimalAtMost("100", "100", 6)).toBe(true)
        expect(decimalAtMost("100.000001", "100", 6)).toBe(false)
        expect(decimalAtMost("99.999999", "100", 6)).toBe(true)
    })

    it("returns false for malformed values", () => {
        expect(decimalAtMost("abc", "100", 6)).toBe(false)
    })
})

describe("validateSalesOrderContractId", () => {
    it("requires a non-empty contract id", () => {
        expect(validateSalesOrderContractId("")).toBe("请选择已有有效合同")
        expect(validateSalesOrderContractId("   ")).toBe("请选择已有有效合同")
        expect(validateSalesOrderContractId("ct-1")).toBeUndefined()
    })
})

describe("validateSalesOrderForm", () => {
    it("accepts a complete submission payload", () => {
        expect(validateSalesOrderForm(makeValues(), "SUBMIT")).toBeUndefined()
    })

    it("validates card vouchers with exactly one line, mall and due date", () => {
        const cardLine = {
            ...makeValues().lineItems[0],
            sku: "voucher-category-1",
            unit: "张",
            faceValue: "100.00",
            cardForm: "电子卡",
            fulfillmentMode: "",
        }
        const valid = makeValues({
            nature: "card_voucher",
            targetMallId: "mall-1",
            receivableDueDate: "2026-09-30",
            taxRatePercent: "6.00",
            lineItems: [cardLine],
        })
        expect(validateSalesOrderForm(valid, "SUBMIT")).toBeUndefined()

        const twoLines = validateSalesOrderForm(
            makeValues({
                nature: "card_voucher",
                targetMallId: "mall-1",
                receivableDueDate: "2026-09-30",
                lineItems: [cardLine, { ...cardLine, rowKey: "l2" }],
            }),
            "SUBMIT",
        )
        expect(twoLines?.fields["lineItems"]).toHaveLength(1)
        expect(twoLines?.fields["lineItems"][0].message).toBe(
            "卡券销售单必须恰好只有一条明细",
        )
    })

    it("maps nested line errors to tanstack field paths", () => {
        const result = validateSalesOrderForm(
            makeValues({
                nature: "card_voucher",
                targetMallId: "mall-1",
                receivableDueDate: "2026-09-30",
                lineItems: [
                    { ...makeValues().lineItems[0], sku: "", unit: "张" },
                ],
            }),
            "SUBMIT",
        )
        expect(result?.fields["lineItems[0].sku"]).toBeDefined()
        expect(result?.fields["lineItems[0].sku"][0].message).toBe(
            "请选择卡券类目",
        )
    })

    it("requires owner, welfare scene and payment terms on submit", () => {
        const result = validateSalesOrderForm(
            makeValues({
                ownerUserId: " ",
                ownerName: " ",
                welfareScene: "",
                paymentTerms: " ",
                taxRatePercent: "100.01",
            }),
            "SUBMIT",
        )
        expect(result?.fields["ownerUserId"]).toBeDefined()
        expect(result?.fields["welfareScene"]).toBeDefined()
        expect(result?.fields["paymentTerms"]).toBeDefined()
        expect(result?.fields["taxRatePercent"][0].message).toBe(
            "税率不能超过 100%",
        )
    })

    it("keeps draft validation lenient: only the contract is required", () => {
        expect(
            validateSalesOrderForm(
                makeValues({
                    customerName: "",
                    settlementEntity: "",
                    ownerUserId: "",
                    ownerName: "",
                    lineItems: [
                        { ...makeValues().lineItems[0], name: "", sku: "" },
                    ],
                }),
                "SAVE_DRAFT",
            ),
        ).toBeUndefined()

        const missingContract = validateSalesOrderForm(
            makeValues({ contractId: "" }),
            "SAVE_DRAFT",
        )
        expect(missingContract?.fields["contractId"][0].message).toBe(
            "请选择已有有效合同",
        )
    })

    it("requires at least one line item", () => {
        const result = validateSalesOrderForm(
            makeValues({ lineItems: [] }),
            "SUBMIT",
        )
        expect(result?.fields["lineItems"][0].message).toBe(
            "至少需要一条销售明细",
        )
    })
})
