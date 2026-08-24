import { beforeEach, describe, expect, it, vi } from "vitest"

const apiMocks = vi.hoisted(() => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
    apiPut: vi.fn(),
}))

vi.mock("@/lib/api", () => apiMocks)

import {
    createSalesOrder,
    fetchSalesOrderDraftForResume,
    saveSalesOrderDraft,
} from "@/features/sales-orders/api/sales-orders-create"
import { prepareStartSalesChangeOrder } from "@/features/sales-orders/api/sales-orders-change"
import { adjustProcurementRejectionDraft } from "@/features/sales-orders/api/sales-orders-procurement"

const goodsLine = {
    id: "working-line-1",
    sales_order_line_id: "sales-line-1",
    line_no: 1,
    line_type: "GOODS_SERVICE",
    gross_amount: "100.00",
    net_amount: "88.50",
    tax_amount: "11.50",
    sales_tax_rate: "0.130000",
    item_name_snapshot: "测试商品",
    spec_snapshot: "标准规格",
    unit_snapshot: "件",
    sku_id: "sku-1",
    sku_revision_id: "sku-revision-1",
    service_region: "EAST",
    fulfillment_mode: "SUPPLIER_DIRECT",
    fulfillment_due_at: 1_800_000_000,
    quantity: "2",
    base_unit_code: "EA",
    unit_price_gross: "50.00",
}

const workingCopy = {
    id: "working-copy-1",
    version: 3,
    working_purpose: "FIRST_SUBMISSION",
    status: "EDITING",
    draft_version: 3,
    content_hash: "hash-1",
    editor_user_id: "sales-1",
    business_type: "GOODS_SERVICE",
    customer_name: "客户甲",
    contract_no: "HT-1",
    settlement_party_name: "结算主体甲",
    payment_term_code: "POSTPAY_NET30",
    payment_term_name: "货到 30 天",
    invoice_type: "SPECIAL",
    tax_point: "13",
    project_name: "年节礼包",
    business_remark: "备注",
    gross_amount: "100.00",
    net_amount: "88.50",
    tax_amount: "11.50",
    lines: [goodsLine],
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("sales order service region contract", () => {
    it("sends service_region in physical sales working-copy lines", async () => {
        apiMocks.apiGet.mockResolvedValue({
            id: "contract-1",
            contract_no: "HT-1",
            customer_id: "customer-1",
            settlement_party_id: "party-1",
            current_revision_id: "contract-revision-1",
            revisions: [
                {
                    id: "contract-revision-1",
                    customer_name: "客户甲",
                    settlement_party_name: "结算主体甲",
                    payment_term_code: "POSTPAY_NET30",
                    payment_term_name: "货到 30 天",
                    invoice_type: "SPECIAL",
                    tax_point: "13",
                },
            ],
        })
        apiMocks.apiPost.mockResolvedValue({
            id: "sales-order-1",
            order_no: "SO-1",
            created_at: 1,
            stage: { label: "草稿" },
        })

        await createSalesOrder({
            orderNo: "SO-1",
            contract: {
                contractId: "contract-1",
                requestedContractRevisionId: "contract-revision-1",
            },
            nature: "physical_service",
            ownerUserId: "sales-1",
            ownerName: "销售甲",
            welfareScene: "ANNUAL_GIFT_BAG",
            paymentTerms: "POSTPAY_NET30",
            fulfillmentDeadline: "",
            targetMallId: "",
            receivableDueDate: "",
            taxRatePercent: "13.00",
            remark: "",
            lineItems: [
                {
                    rowKey: "line-1",
                    name: "测试商品",
                    sku: "sku-1",
                    skuRevisionId: "sku-revision-1",
                    serviceRegion: " EAST ",
                    quantity: "2",
                    unit: "件",
                    unitPriceGross: "50.00",
                    fulfillmentMode: "供应商直发",
                    dueDate: "2026-09-01",
                    faceValue: "",
                    giftRate: "",
                    cardForm: "",
                },
            ],
            intent: "SAVE_DRAFT",
            idempotencyKey: "create-1",
        })

        expect(apiMocks.apiPost).toHaveBeenCalledWith(
            "/admin/sales-orders",
            expect.objectContaining({
                draft: expect.objectContaining({
                    lines: [
                        expect.objectContaining({
                            goods: expect.objectContaining({
                                service_region: "EAST",
                            }),
                        }),
                    ],
                }),
            }),
        )
    })

    it("sends service_region when saving an existing sales working copy", async () => {
        apiMocks.apiGet.mockResolvedValue({
            id: "contract-1",
            contract_no: "HT-1",
            customer_id: "customer-1",
            settlement_party_id: "party-1",
            current_revision_id: "contract-revision-1",
            revisions: [
                {
                    id: "contract-revision-1",
                    customer_name: "客户甲",
                    settlement_party_name: "结算主体甲",
                    payment_term_code: "POSTPAY_NET30",
                    payment_term_name: "货到 30 天",
                    invoice_type: "SPECIAL",
                    tax_point: "13",
                },
            ],
        })
        apiMocks.apiPut.mockResolvedValue({ version: 4 })

        await saveSalesOrderDraft({
            salesOrderId: "sales-order-1",
            version: 3,
            contract: {
                contractId: "contract-1",
                requestedContractRevisionId: "contract-revision-1",
            },
            nature: "physical_service",
            ownerUserId: "sales-1",
            ownerName: "销售甲",
            welfareScene: "ANNUAL_GIFT_BAG",
            paymentTerms: "POSTPAY_NET30",
            fulfillmentDeadline: "",
            targetMallId: "",
            receivableDueDate: "",
            taxRatePercent: "13.00",
            remark: "",
            lineItems: [
                {
                    rowKey: "line-1",
                    name: "测试商品",
                    sku: "sku-1",
                    skuRevisionId: "sku-revision-1",
                    serviceRegion: "EAST",
                    quantity: "2",
                    unit: "件",
                    unitPriceGross: "50.00",
                    fulfillmentMode: "供应商直发",
                    dueDate: "2026-09-01",
                    faceValue: "",
                    giftRate: "",
                    cardForm: "",
                },
            ],
        })

        expect(apiMocks.apiPut).toHaveBeenCalledWith(
            "/admin/sales-orders/sales-order-1/working-copy",
            expect.objectContaining({
                version: 3,
                draft: expect.objectContaining({
                    lines: [
                        expect.objectContaining({
                            goods: expect.objectContaining({
                                service_region: "EAST",
                            }),
                        }),
                    ],
                }),
            }),
        )
    })

    it("maps service_region back into resume line inputs", async () => {
        apiMocks.apiGet.mockResolvedValue({
            id: "sales-order-1",
            order_no: "SO-1",
            business_type: "GOODS_SERVICE",
            commercial_status: "DRAFT",
            version: 5,
            contract_id: "contract-1",
            working_copy: workingCopy,
            submissions: [],
        })

        const resumed = await fetchSalesOrderDraftForResume("sales-order-1")

        expect(resumed?.lineItems[0]?.serviceRegion).toBe("EAST")
    })

    it("preserves service_region when rebuilding a procurement-rejection draft", async () => {
        apiMocks.apiGet.mockResolvedValue({
            id: "sales-order-1",
            version: 5,
            working_copy: workingCopy,
        })
        apiMocks.apiPut.mockResolvedValue({})

        await adjustProcurementRejectionDraft({
            salesOrderId: "sales-order-1",
            unitPriceGross: "45.00",
            note: "调整价格",
        })

        expect(apiMocks.apiPut).toHaveBeenCalledWith(
            "/admin/sales-orders/sales-order-1/working-copy",
            expect.objectContaining({
                draft: expect.objectContaining({
                    lines: [
                        expect.objectContaining({
                            goods: expect.objectContaining({
                                service_region: "EAST",
                            }),
                        }),
                    ],
                }),
            }),
        )
    })

    it("preserves service_region when preparing a sales change payload", async () => {
        apiMocks.apiGet.mockResolvedValue({
            id: "sales-order-1",
            customer_id: "customer-1",
            working_copy: workingCopy,
            submissions: [],
            revisions: [{ revision_no: 2 }],
        })

        const prepared = await prepareStartSalesChangeOrder({
            salesOrderId: "sales-order-1",
            baseRevisionNo: 2,
            nature: "physical_service",
        })

        expect(prepared.command).toEqual(
            expect.objectContaining({
                draft: expect.objectContaining({
                    lines: [
                        expect.objectContaining({
                            goods: expect.objectContaining({
                                service_region: "EAST",
                            }),
                        }),
                    ],
                }),
            }),
        )
    })
})
