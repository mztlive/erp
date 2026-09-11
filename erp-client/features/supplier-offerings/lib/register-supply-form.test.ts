import { describe, expect, it } from "vitest"
import {
    registerSupplyDefaults,
    registerSupplySchema,
} from "./register-supply-form"

const valid = () => ({
    ...registerSupplyDefaults("sku-1", new Date(2026, 8, 11)),
    supplierId: "supplier-1",
    supplierSkuCode: "order-code",
    dropshipPrice: "12.3456",
    bulkPrice: "11",
    inputTaxPercentage: "13",
    supplyRegionText: "上海",
})

describe("新增供给录入合同", () => {
    it("使用本地业务日期，长期有效且不代填供货区域或数量", () => {
        expect(
            registerSupplyDefaults("sku-1", new Date(2026, 8, 11, 0, 1)),
        ).toMatchObject({
            skuId: "sku-1",
            validFrom: "2026-09-11",
            validTo: "",
            validityMode: "ongoing",
            supplyRegionText: "",
            availableQuantity: "",
            minimumQuantity: "1",
        })
        expect(registerSupplySchema.safeParse(valid()).success).toBe(true)
    })
    it.each(["", "2026-09-10", "2026-09-11", "2026-02-30"])(
        "指定失效日期时拒绝空值、倒序和无效日期 %s",
        (validTo) => {
            const result = registerSupplySchema.safeParse({
                ...valid(),
                validityMode: "dated",
                validTo,
            })
            expect(result.success).toBe(false)
            if (!result.success)
                expect(
                    result.error.issues.some(
                        (issue) => issue.path[0] === "validTo",
                    ),
                ).toBe(true)
        },
    )
    it("日期有效时允许预约生效，并保留两种价格和自定义区域", () => {
        expect(
            registerSupplySchema.safeParse({
                ...valid(),
                validityMode: "dated",
                validFrom: "2026-10-01",
                validTo: "2026-10-02",
                supplyRegionText: "华东、上海浦东",
            }).success,
        ).toBe(true)
        expect(
            registerSupplySchema.safeParse({ ...valid(), bulkPrice: "" })
                .success,
        ).toBe(false)
    })
    it("纯分隔符不构成可供区域", () => {
        expect(
            registerSupplySchema.safeParse({
                ...valid(),
                supplyRegionText: "，、,",
            }).success,
        ).toBe(false)
    })
    it("保留后端允许的零库存和不可供状态，不擅自改写数量", () => {
        expect(
            registerSupplySchema.safeParse({
                ...valid(),
                availableQuantity: "0",
            }).success,
        ).toBe(true)
        expect(
            registerSupplySchema.safeParse({
                ...valid(),
                availabilityStatus: "UNAVAILABLE",
                availableQuantity: "12",
            }).success,
        ).toBe(true)
    })
})
