import { beforeEach, describe, expect, it, vi } from "vitest"

import type {
    BackendContractDetail,
    BackendSalesOrderDetail,
} from "@/features/sales-orders/api/contracts"
import { fetchSalesOrderDetail } from "@/features/sales-orders/api/sales-orders-detail"

const apiMocks = vi.hoisted(() => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
}))

vi.mock("@/lib/api", () => apiMocks)

const detail: BackendSalesOrderDetail = {
    id: "so-1",
    order_no: "XS202608230001",
    business_type: "VOUCHER",
    origin_system: "ERP",
    customer_id: "customer-1",
    contract_id: "contract-1",
    settlement_party_id: "party-1",
    commercial_status: "PENDING_REVIEW",
    review_status: "IN_APPROVAL",
    fulfillment_progress: "NOT_STARTED",
    collection_progress: "NOT_COLLECTED",
    invoice_progress: "NOT_INVOICED",
    close_status: "OPEN",
    version: 4,
    created_at: 1_700_000_000,
    owner_user_id: "sales-1",
    owner_user_name: "销售甲",
    purchase_order_count: 0,
    settled_total: "0.00",
    invoiced_total: "0.00",
    lines: [{ id: "sol-1", line_no: 1, line_status: "ACTIVE" }],
    working_copy: null,
    submissions: [
        {
            id: "submission-1",
            submission_no: 1,
            status: "IN_REVIEW",
            business_type: "VOUCHER",
            customer_name: "下单客户快照",
            contract_no: "HT-2026-001",
            contract_revision_id: "contract-revision-2",
            settlement_party_name: "下单结算主体",
            payment_term_code: "POSTPAY_NET30",
            payment_term_name: "货到 30 天",
            tax_point: "6",
            project_name: "年节礼包",
            business_remark: "客户要求分批发放",
            voucher_category_sku_id: "voucher-sku-1",
            voucher_expiry_at: 1_800_000_000,
            target_mall_id: "mall-1",
            receivable_due_date: "2026-09-30",
            gross_amount: "180.00",
            net_amount: "169.81",
            tax_amount: "10.19",
            submitted_by: "sales-1",
            submitted_at: 1_700_000_100,
            created_at: 1_700_000_100,
            lines: [
                {
                    id: "submission-line-1",
                    sales_order_line_id: "sol-1",
                    line_no: 1,
                    line_type: "VOUCHER",
                    gross_amount: "180.00",
                    net_amount: "169.81",
                    tax_amount: "10.19",
                    sales_tax_rate: "0.060000",
                    item_name_snapshot: "节日卡券",
                    spec_snapshot: "voucher-sku-1",
                    sku_id: "voucher-sku-1",
                    card_count: 2,
                    unit_price_gross: "90.00",
                    face_value: "100.00",
                    card_form: "ELECTRONIC",
                },
            ],
        },
    ],
    revisions: [],
    stage: {
        code: "in_approval",
        label: "审批中",
        tone: "warning",
    },
    close_eligibility: {
        fulfillment_complete: false,
        receivable_settled: false,
        invoice_complete: false,
        eligible_to_close: false,
        blockers: ["履约未完成", "应收未结清"],
        note: "履约完成且应收结清后关闭",
    },
    can_start_sales_change_order: false,
}

const contract: BackendContractDetail = {
    id: "contract-1",
    contract_no: "HT-2026-001",
    customer_id: "customer-1",
    settlement_party_id: "party-1",
    status: "EFFECTIVE",
    current_revision_id: "contract-revision-3",
    created_at: 1,
    version: 3,
    revisions: [
        {
            id: "contract-revision-2",
            revision_no: 2,
            customer_name: "合同 v2 客户",
            settlement_party_name: "结算主体 v2",
            payment_term_code: "POSTPAY_NET30",
            payment_term_name: "货到 30 天",
            invoice_type: "SPECIAL",
            tax_point: "6",
            valid_from: "2026-01-01",
            signed_at: "2026-01-01",
            created_at: 2,
        },
        {
            id: "contract-revision-3",
            revision_no: 3,
            customer_name: "合同当前客户",
            settlement_party_name: "结算主体 v3",
            payment_term_code: "PREPAY_100",
            payment_term_name: "先款 100%",
            invoice_type: "SPECIAL",
            tax_point: "13",
            valid_from: "2026-06-01",
            signed_at: "2026-06-01",
            created_at: 3,
        },
    ],
}

beforeEach(() => {
    vi.clearAllMocks()
    apiMocks.apiGet.mockImplementation(async (path: string) => {
        switch (path) {
            case "/admin/sales-orders/so-1":
                return detail
            case "/admin/customers/customer-1":
                return {
                    id: "customer-1",
                    party_id: "customer-party-1",
                    customer_no: "KH-001",
                    legal_name: "客户当前名称",
                }
            case "/admin/parties/customer-party-1/contacts":
                return {
                    items: [
                        {
                            id: "contact-1",
                            contact_name: "联系人甲",
                            is_default: true,
                            status: "ACTIVE",
                        },
                    ],
                    total: 1,
                    page: 1,
                    page_size: 100,
                }
            case "/admin/contracts/contract-1":
                return contract
            case "/admin/source-systems":
                return {
                    items: [
                        {
                            id: "mall-1",
                            name: "员工福利商城",
                            system_type: "MALL",
                        },
                    ],
                    total: 1,
                    page: 1,
                    page_size: 100,
                }
            case "/admin/sales-change-orders":
            case "/admin/customer-acceptances":
                return {
                    items: [],
                    total: 0,
                    page: 1,
                    page_size: 10,
                }
            default:
                throw new Error(`unexpected GET ${path}`)
        }
    })
})

describe("fetchSalesOrderDetail", () => {
    it("keeps the creation snapshot and exposes every persisted input for display", async () => {
        const order = await fetchSalesOrderDetail("so-1")

        expect(order).not.toBeNull()
        expect(order?.contractRevisionLabel).toBe("HT-2026-001@v2")
        expect(order?.customerName).toBe("下单客户快照")
        expect(order?.settlementEntity).toBe("下单结算主体")
        expect(order?.paymentTerms).toBe("货到 30 天")
        expect(order?.taxRatePercent).toBe("6.00")
        expect(order?.targetMallName).toBe("员工福利商城")
        expect(order?.receivableDueDate).toBe("2026-09-30")
        expect(order?.remark).toBe("客户要求分批发放")
        expect(order?.customerContact).toBe("联系人甲")
        expect(order?.lineItems[0]?.unitPriceGross).toBe("90.00")
        expect(order?.lineItems[0]?.giftRate).toBe("11.11")
        expect(order?.lineItems[0]?.sku).toBeUndefined()
        expect(order?.currentRevisionNo).toBeNull()
    })
})
