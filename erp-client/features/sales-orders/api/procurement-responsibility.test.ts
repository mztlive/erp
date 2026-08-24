import { beforeEach, describe, expect, it, vi } from "vitest"

vi.mock("@/lib/api", () => ({ apiPost: vi.fn() }))

import { apiPost } from "@/lib/api"
import { resolveSalesLineProcurementResponsibilities } from "@/features/sales-orders/api/procurement-responsibility"

const mockedApiPost = vi.mocked(apiPost)

beforeEach(() => {
    vi.clearAllMocks()
})

describe("resolveSalesLineProcurementResponsibilities", () => {
    it("posts only the supported responsibility inputs including service region", async () => {
        mockedApiPost.mockResolvedValue({
            lines: [
                {
                    line_key: "line-1",
                    resolved: true,
                    owner_user_id: "buyer-1",
                    owner_name: "采购李四",
                    rule_type: "SKU",
                },
            ],
        })

        const result = await resolveSalesLineProcurementResponsibilities([
            {
                rowKey: "line-1",
                name: "测试商品",
                sku: "sku-1",
                skuRevisionId: "sku-revision-1",
                serviceRegion: " 华东 ",
                quantity: "2",
                unit: "件",
                unitPriceGross: "100",
                fulfillmentMode: "供应商直发",
                dueDate: "2026-09-01",
                faceValue: "",
                giftRate: "",
                cardForm: "",
            },
        ])

        expect(mockedApiPost).toHaveBeenCalledWith(
            "/admin/procurement-responsibility/resolve",
            {
                lines: [
                    {
                        line_key: "line-1",
                        sku_id: "sku-1",
                        service_region: "华东",
                    },
                ],
            },
        )
        const request = JSON.stringify(
            vi.mocked(mockedApiPost).mock.calls[0]?.[1],
        )
        expect(request).not.toContain("sku_revision_id")
        expect(request).not.toContain("fulfillment_mode")
        expect(result).toEqual([
            {
                rowKey: "line-1",
                resolved: true,
                ownerUserId: "buyer-1",
                ownerName: "采购李四",
                matchedRuleType: "SKU",
            },
        ])
    })
})
