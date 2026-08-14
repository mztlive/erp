import { describe, expect, it } from "vitest"

import {
    availabilitySchema,
    createSchema,
    errorMessage,
    idempotencyKey,
    percentageFromRate,
    rateFromPercentage,
    reviseSchema,
    splitValues,
} from "./offering-forms"

describe("splitValues", () => {
    it("splits on Chinese and ASCII commas and trims each part", () => {
        expect(splitValues("华东，华北、华南,西南")).toEqual([
            "华东",
            "华北",
            "华南",
            "西南",
        ])
    })

    it("drops empty parts", () => {
        expect(splitValues("")).toEqual([])
        expect(splitValues(" ， , ")).toEqual([])
    })
})

describe("rateFromPercentage", () => {
    it("converts percentages to 6-decimal rates", () => {
        expect(rateFromPercentage("13")).toBe("0.130000")
        expect(rateFromPercentage("7.5")).toBe("0.075000")
        expect(rateFromPercentage("0")).toBe("0.000000")
    })
})

describe("percentageFromRate", () => {
    it("converts rates back to percentages", () => {
        expect(percentageFromRate("0.130000")).toBe("13")
        expect(percentageFromRate("0.075000")).toBe("7.5")
    })

    it("returns an empty string for missing or falsy rates", () => {
        expect(percentageFromRate(null)).toBe("")
        expect(percentageFromRate(undefined)).toBe("")
        expect(percentageFromRate("")).toBe("")
    })
})

describe("errorMessage", () => {
    it("reads the message from Error instances and message-like objects", () => {
        expect(errorMessage(new Error("boom"), "fallback")).toBe("boom")
        expect(errorMessage({ message: "wrapped" }, "fallback")).toBe("wrapped")
    })

    it("falls back for non-object errors", () => {
        expect(errorMessage("plain text", "fallback")).toBe("fallback")
        expect(errorMessage(null, "fallback")).toBe("fallback")
        expect(errorMessage(undefined, "fallback")).toBe("fallback")
    })
})

describe("idempotencyKey", () => {
    it("prefixes the key and generates unique values", () => {
        const first = idempotencyKey("create-supplier-offering")
        const second = idempotencyKey("create-supplier-offering")

        expect(first.startsWith("create-supplier-offering-")).toBe(true)
        expect(second.startsWith("create-supplier-offering-")).toBe(true)
        expect(first).not.toBe(second)
    })
})

const validCreateInput = {
    skuId: "sku_1",
    supplierId: "sup_1",
    supplierProductCode: "",
    supplierSkuCode: "V-001",
    dropshipPrice: "10",
    bulkPrice: "8",
    minimumQuantity: "10",
    inputTaxPercentage: "13",
    supplyRegionText: "华东",
    validFrom: "2026-01-01",
    validTo: "",
    dropshipExpress: "",
    freightAmount: "",
    serviceFeeAmount: "",
    availabilityStatus: "AVAILABLE",
    availableQuantity: "100",
    changeReason: "新增供应商供给",
} as const

describe("createSchema", () => {
    it("accepts a valid create input", () => {
        expect(createSchema.safeParse(validCreateInput).success).toBe(true)
    })

    it("requires a positive minimum order quantity", () => {
        const result = createSchema.safeParse({
            ...validCreateInput,
            minimumQuantity: "0",
        })
        expect(result.success).toBe(false)
        if (!result.success) {
            expect(result.error.issues[0]?.message).toBe("起订量必须大于 0")
        }
    })

    it("rejects tax rates above 100", () => {
        const result = createSchema.safeParse({
            ...validCreateInput,
            inputTaxPercentage: "101",
        })
        expect(result.success).toBe(false)
        if (!result.success) {
            expect(result.error.issues[0]?.message).toBe("税率不能超过 100%")
        }
    })

    it("requires sku, supplier, region and reason", () => {
        const result = createSchema.safeParse({
            ...validCreateInput,
            skuId: "",
            supplierId: "",
            supplyRegionText: "",
            changeReason: "",
        })
        expect(result.success).toBe(false)
        if (!result.success) {
            const messages = result.error.issues.map((issue) => issue.message)
            expect(messages).toContain("请选择公司 SKU")
            expect(messages).toContain("请选择供应商")
            expect(messages).toContain("请填写可供区域")
            expect(messages).toContain("请填写登记原因")
        }
    })

    it("allows an empty available quantity", () => {
        expect(
            createSchema.safeParse({ ...validCreateInput, availableQuantity: "" })
                .success,
        ).toBe(true)
    })
})

describe("reviseSchema", () => {
    const validReviseInput = {
        dropshipPrice: "10",
        bulkPrice: "8",
        minimumQuantity: "10",
        inputTaxPercentage: "13",
        supplyRegionText: "华东",
        validFrom: "2026-01-01",
        validTo: "",
        dropshipExpress: "",
        freightAmount: "",
        serviceFeeAmount: "",
        status: "ACTIVE",
        changeReason: "调整供给条款",
    } as const

    it("accepts a valid revise input", () => {
        expect(reviseSchema.safeParse(validReviseInput).success).toBe(true)
    })

    it("rejects unknown statuses and empty reasons", () => {
        const result = reviseSchema.safeParse({
            ...validReviseInput,
            status: "PAUSED_OK",
            changeReason: "",
        })
        expect(result.success).toBe(false)
    })
})

describe("availabilitySchema", () => {
    const validAvailabilityInput = {
        availabilityStatus: "UNAVAILABLE",
        availableQuantity: "",
        changeReason: "更新当前可供情况",
    } as const

    it("accepts a valid availability input", () => {
        expect(
            availabilitySchema.safeParse(validAvailabilityInput).success,
        ).toBe(true)
    })

    it("rejects non-numeric quantities", () => {
        const result = availabilitySchema.safeParse({
            ...validAvailabilityInput,
            availableQuantity: "abc",
        })
        expect(result.success).toBe(false)
    })

    it("rejects unknown availability statuses", () => {
        const result = availabilitySchema.safeParse({
            ...validAvailabilityInput,
            availabilityStatus: "COMING_SOON",
        })
        expect(result.success).toBe(false)
    })
})
