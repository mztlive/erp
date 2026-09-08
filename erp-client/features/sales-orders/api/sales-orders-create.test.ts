import { beforeEach, expect, test, vi } from "vitest"
import { apiGet, apiPut } from "@/lib/api"
import { createEmptyLine } from "../lib/sales-order-create-model"
import { salesLineSpecification } from "../lib/sales-line-specification"
import {
    fetchSalesOrderDraftForResume,
    saveSalesOrderDraft,
} from "./sales-orders-create"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
    apiPut: vi.fn(),
    apiPost: vi.fn(),
}))
beforeEach(() => vi.clearAllMocks())

const skuId = "afaa5f33a7714dfc84e3f6557c8bcc32"

test("保存规格文字与 SKU 身份到不同字段，缺规格不回填 ID", async () => {
    vi.mocked(apiPut).mockResolvedValue({ version: 2 })
    for (const specification of ["250g 礼盒", undefined, skuId]) {
        await saveSalesOrderDraft({
            salesOrderId: "so-1",
            version: 1,
            nature: "physical_service",
            ownerUserId: "sales",
            ownerName: "销售",
            welfareScene: "",
            paymentTerms: "CONTRACT",
            fulfillmentDeadline: "",
            receivableDueDate: "",
            taxRatePercent: "0",
            remark: "",
            contract: {
                contractId: "contract",
                requestedContractRevisionId: "contract-rev",
            },
            lineItems: [
                {
                    ...createEmptyLine("physical_service"),
                    name: "龙井礼盒",
                    sku: skuId,
                    skuRevisionId: "sku-rev",
                    specification,
                    quantity: "1",
                    unit: "盒",
                    unitPriceGross: "1288.00",
                },
            ],
        })
        const body = vi.mocked(apiPut).mock.lastCall?.[1] as {
            draft: {
                lines: Array<{
                    spec_snapshot: string | null
                    goods: { sku_id: string; sku_revision_id: string }
                }>
            }
        }
        expect(body.draft.lines[0].spec_snapshot).toBe(
            specification === "250g 礼盒" ? specification : null,
        )
        expect(body.draft.lines[0].goods.sku_id).toBe(skuId)
        expect(body.draft.lines[0].goods.sku_revision_id).toBe("sku-rev")
    }
})

test("恢复旧草稿时清除误存为规格的 ID，保留真正规格", async () => {
    for (const spec of [skuId, "250g 礼盒"]) {
        vi.mocked(apiGet).mockResolvedValue({
            id: "so-1",
            order_no: "XS-1",
            business_type: "GOODS_SERVICE",
            commercial_status: "DRAFT",
            working_copy: {
                version: 1,
                lines: [
                    {
                        id: "line",
                        item_name_snapshot: "龙井礼盒",
                        sku_id: skuId,
                        sku_revision_id: "sku-rev",
                        spec_snapshot: spec,
                        line_type: "GOODS_SERVICE",
                    },
                ],
            },
            submissions: [],
        })
        const result = await fetchSalesOrderDraftForResume("so-1")
        expect(result?.lineItems[0].specification).toBe(
            spec === skuId ? "" : spec,
        )
    }
    expect(salesLineSpecification(" 规格 A ", skuId)).toBe("规格 A")
    expect(salesLineSpecification("sku-rev", skuId, "sku-rev")).toBe("")
    expect(salesLineSpecification("AB1234", "other-sku")).toBe("AB1234")
})
