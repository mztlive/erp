import { beforeEach, describe, expect, it, vi } from "vitest"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
    apiPut: vi.fn(),
}))

import { apiGet, apiPost, apiPut } from "@/lib/api"
import {
    fetchProcurementResponsibilityRules,
    saveProcurementResponsibilityRule,
} from "@/features/procurement-responsibilities/api/rules"

const mockedApiGet = vi.mocked(apiGet)
const mockedApiPost = vi.mocked(apiPost)
const mockedApiPut = vi.mocked(apiPut)

const backendRule = {
    id: "rule-1",
    rule_type: "CATEGORY_SERVICE_REGION",
    category_id: "category-1",
    category_name: "企业福利",
    service_region: "华东",
    owner_user_id: "buyer-1",
    owner_name: "采购李四",
    status: "ENABLED",
    version: 3,
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("procurement responsibility rule API", () => {
    it("maps list wire fields in one adapter layer", async () => {
        mockedApiGet.mockResolvedValue({ items: [backendRule] })

        await expect(fetchProcurementResponsibilityRules()).resolves.toEqual([
            {
                ruleId: "rule-1",
                ruleType: "CATEGORY_SERVICE_REGION",
                skuId: undefined,
                skuLabel: undefined,
                categoryId: "category-1",
                categoryLabel: "企业福利",
                serviceRegion: "华东",
                productKind: undefined,
                ownerUserId: "buyer-1",
                ownerName: "采购李四",
                enabled: true,
                version: 3,
            },
        ])
        expect(mockedApiGet).toHaveBeenCalledWith(
            "/admin/procurement-responsibility-rules",
        )
    })

    it("posts a new SKU rule and puts an existing rule", async () => {
        mockedApiPost.mockResolvedValue({
            ...backendRule,
            rule_type: "SKU",
            sku_id: "sku-1",
        })
        await saveProcurementResponsibilityRule({
            ruleType: "SKU",
            skuId: "sku-1",
            ownerUserId: "buyer-1",
            enabled: true,
        })
        expect(mockedApiPost).toHaveBeenCalledWith(
            "/admin/procurement-responsibility-rules",
            expect.objectContaining({
                rule_type: "SKU",
                sku_id: "sku-1",
                owner_user_id: "buyer-1",
                status: "ENABLED",
            }),
        )

        mockedApiPut.mockResolvedValue(backendRule)
        await saveProcurementResponsibilityRule({
            ruleId: "rule-1",
            ruleType: "CATEGORY_SERVICE_REGION",
            categoryId: "category-1",
            serviceRegion: "华东",
            ownerUserId: "buyer-1",
            enabled: false,
            expectedVersion: 3,
        })
        expect(mockedApiPut).toHaveBeenCalledWith(
            "/admin/procurement-responsibility-rules/rule-1",
            expect.objectContaining({
                category_id: "category-1",
                service_region: "华东",
                expected_version: 3,
                status: "DISABLED",
            }),
        )
    })
})
