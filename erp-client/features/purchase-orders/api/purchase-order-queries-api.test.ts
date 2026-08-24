import { beforeEach, describe, expect, it, vi } from "vitest"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
}))

import { apiGet } from "@/lib/api"
import {
    fetchCreationBases,
    fetchPurchaseOrderCenter,
    fetchPurchaseOrders,
} from "./purchase-order-queries-api"
import type { BackendListItem } from "./purchase-order-wire-types"

const mockedApiGet = vi.mocked(apiGet)

function makeBackendListItem(
    overrides: Partial<BackendListItem> = {},
): BackendListItem {
    return {
        id: "po_1",
        purchase_no: "PO-1",
        sales_order_id: "so_1",
        sales_order_no: "XS202608230001",
        supplier_id: "sup_1",
        supplier_name: "供应商A",
        purchase_type: "PHYSICAL",
        status: "DRAFT",
        review_status: "NONE",
        gross_amount: "10",
        net_amount: "9",
        tax_amount: "1",
        payment_progress: "NONE",
        invoice_progress: "NONE",
        fulfillment_progress: "NONE",
        version: 1,
        created_at: 1_700_000_000,
        ...overrides,
    }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("fetchPurchaseOrders", () => {
    it("只请求一次采购单列表，不再附带统计或建单依据", async () => {
        mockedApiGet.mockResolvedValue({
            items: [makeBackendListItem()],
            total: 1,
            page: 1,
            page_size: 20,
        })

        const result = await fetchPurchaseOrders({
            q: "钢",
            salesOrderId: "so_1",
            status: "DRAFT",
            metric: "all",
            page: 1,
            pageSize: 20,
        })

        expect(mockedApiGet).toHaveBeenCalledTimes(1)
        expect(mockedApiGet).toHaveBeenCalledWith("/admin/purchase-orders", {
            q: "钢",
            sales_order_id: "so_1",
            status: "DRAFT",
            page: 1,
            page_size: 20,
            sort_by: undefined,
            sort_dir: undefined,
        })
        expect(result.rows).toHaveLength(1)
        expect(result.rows[0]?.purchaseOrderId).toBe("po_1")
        expect(result.rows[0]?.salesOrderNo).toBe("XS202608230001")
        expect(result.rows[0]?.paymentTermLabel).toBe("—")
        expect(result.rows[0]?.ownerName).toBe("—")
        expect(result.total).toBe(1)
        expect(result.metrics).toEqual([])
    })

    it("把付款条件与负责人映射到列表行", async () => {
        mockedApiGet.mockResolvedValue({
            items: [
                makeBackendListItem({
                    payment_term_code: "POSTPAY_NET30",
                    owner_name: "张三",
                }),
            ],
            total: 1,
            page: 1,
            page_size: 20,
        })

        const result = await fetchPurchaseOrders({
            page: 1,
            pageSize: 20,
        })

        expect(result.rows[0]?.paymentTermCode).toBe("POSTPAY_NET30")
        expect(result.rows[0]?.paymentTermLabel).toBe("货到 30 天")
        expect(result.rows[0]?.ownerName).toBe("张三")
    })
})

describe("fetchPurchaseOrderCenter", () => {
    it("maps the created approval binding without inventing a work item", async () => {
        mockedApiGet.mockResolvedValue({
            id: "po_1",
            purchase_no: "PO-1",
            status: "DRAFT",
            review_status: "NONE",
            version: 1,
            sales_order_id: "so_1",
            sales_order_no: "XS202608230001",
            supplier_id: "sup_1",
            supplier_name: "供应商A",
            purchase_type: "PHYSICAL",
            payment_term_code: "POSTPAY_NET30",
            fulfillment_responsibility: "WAREHOUSE",
            payment_progress: "NONE",
            invoice_progress: "NONE",
            fulfillment_progress: "NONE",
            content_source: "DRAFT",
            lines: [
                {
                    line_id: "po-line-1",
                    line_no: 1,
                    line_type: "ITEM_SERVICE",
                    sku_id: "internal-sku-id",
                    product_name: "测试商品",
                    specification: "标准规格",
                    sales_order_submission_line_id:
                        "internal-sales-submission-line-id",
                    gross_amount: "10",
                    net_amount: "9",
                    tax_amount: "1",
                },
            ],
            totals: { gross: "0", net: "0", tax: "0" },
            allocations: [],
            changes: [],
            approval: {
                requirement: "PROCESS_REQUIRED",
                definition: {
                    id: "def-po-1",
                    name: "采购单审批",
                    version: 2,
                    nodes: [
                        { key: "n1", name: "采购审核", assignee_name: "张三" },
                    ],
                },
                instance: null,
                recent_history: [],
                allowed_actions: ["SUBMIT", "UPGRADE_BINDING"],
            },
            created_at: 1_700_000_000,
        })

        const center = await fetchPurchaseOrderCenter("po_1")
        expect(center?.approval?.instance).toBeUndefined()
        expect(center?.approval?.definition?.name).toBe("采购单审批")
        expect(center?.reviewWorkItem).toBeUndefined()
        expect(center?.allowedActions).toEqual(
            expect.arrayContaining(["SUBMIT", "UPGRADE_BINDING"]),
        )
        expect(center?.identity.statusLabel).toBe("草稿")
        expect(center?.header.salesOrderNo).toBe("XS202608230001")
        expect(center?.currentContent.lines[0]).toMatchObject({
            itemSku: "标准规格",
            salesAllocationLabel: "已关联销售明细",
        })
        expect(JSON.stringify(center?.currentContent.lines[0])).not.toContain(
            "internal-sales-submission-line-id",
        )
        expect(JSON.stringify(center?.currentContent.lines[0])).not.toContain(
            "internal-sku-id",
        )
    })

    it("maps IN_APPROVAL to the runtime review status without inventing assignees", async () => {
        mockedApiGet.mockResolvedValue({
            id: "po_2",
            purchase_no: "PO-2",
            status: "IN_APPROVAL",
            review_status: "PENDING",
            version: 2,
            sales_order_id: "so_1",
            sales_order_no: "XS202608230001",
            supplier_id: "sup_1",
            supplier_name: "供应商A",
            purchase_type: "PHYSICAL",
            payment_term_code: "POSTPAY_NET30",
            fulfillment_responsibility: "WAREHOUSE",
            payment_progress: "NONE",
            invoice_progress: "NONE",
            fulfillment_progress: "NONE",
            content_source: "SUBMISSION",
            lines: [],
            totals: { gross: "0", net: "0", tax: "0" },
            allocations: [],
            changes: [],
            approval: {
                requirement: "PROCESS_REQUIRED",
                definition: {
                    id: "def-po-1",
                    name: "采购单审批",
                    version: 2,
                    nodes: [],
                },
                instance: {
                    id: "inst-po-1",
                    status: "RUNNING",
                    current_round_no: 1,
                    current_node_name: "采购审核",
                    current_assignee_name: "张三",
                },
                recent_history: [],
                allowed_actions: ["CANCEL"],
            },
            created_at: 1_700_000_000,
        })

        const center = await fetchPurchaseOrderCenter("po_2")
        expect(center?.identity.status).toBe("PENDING_REVIEW")
        expect(center?.identity.statusLabel).toBe("审批中")
        expect(center?.approval?.instance?.currentAssigneeName).toBe("张三")
        expect(center?.reviewWorkItem).toBeUndefined()
    })

    it("maps the purchase change order binding without inventing a work item", async () => {
        mockedApiGet.mockImplementation(async (path: string) => {
            if (path === "/admin/purchase-change-orders") {
                return {
                    items: [
                        {
                            id: "pco_1",
                            purchase_order_id: "po_3",
                            base_revision_id: "rev_1",
                            reason: "采购变更",
                            status: "DRAFT",
                            version: 2,
                            created_at: 1_700_000_000,
                        },
                    ],
                    total: 1,
                    page: 1,
                    page_size: 10,
                }
            }
            if (path === "/admin/purchase-change-orders/pco_1") {
                return {
                    id: "pco_1",
                    purchase_order_id: "po_3",
                    base_revision_id: "rev_1",
                    reason: "采购变更",
                    status: "DRAFT",
                    version: 2,
                    created_at: 1_700_000_000,
                    approval: {
                        requirement: "PROCESS_REQUIRED",
                        definition: {
                            id: "def-pco-1",
                            name: "采购变更审批",
                            version: 2,
                            nodes: [
                                {
                                    key: "n1",
                                    name: "仓配影响确认",
                                    assignee_name: "张三",
                                },
                            ],
                        },
                        instance: null,
                        recent_history: [],
                        allowed_actions: ["SUBMIT", "UPGRADE_BINDING"],
                    },
                }
            }
            return {
                id: "po_3",
                purchase_no: "PO-3",
                status: "EFFECTIVE",
                review_status: "APPROVED",
                version: 3,
                sales_order_id: "so_1",
                sales_order_no: "XS202608230001",
                supplier_id: "sup_1",
                supplier_name: "供应商A",
                purchase_type: "PHYSICAL",
                payment_term_code: "POSTPAY_NET30",
                fulfillment_responsibility: "WAREHOUSE",
                payment_progress: "NONE",
                invoice_progress: "NONE",
                fulfillment_progress: "NONE",
                content_source: "REVISION",
                lines: [],
                totals: { gross: "0", net: "0", tax: "0" },
                allocations: [],
                changes: [
                    {
                        change_id: "pco_1",
                        status: "DRAFT",
                        base_revision_id: "rev_1",
                        reason: "采购变更",
                        created_at: 1_700_000_000,
                    },
                ],
                created_at: 1_700_000_000,
            }
        })

        const center = await fetchPurchaseOrderCenter("po_3")
        expect(center?.changes[0]?.statusLabel).toBe("草稿")
        expect(center?.activeChangeOrder?.id).toBe("pco_1")
        expect(center?.activeChangeOrder?.approval?.instance).toBeUndefined()
        expect(center?.activeChangeOrder?.approval?.definition?.name).toBe(
            "采购变更审批",
        )
        expect(center?.activeChangeOrder?.approval?.allowedActions).toEqual([
            "SUBMIT",
            "UPGRADE_BINDING",
        ])
    })
})

describe("fetchCreationBases", () => {
    it("按独立接口拉取创建依据", async () => {
        mockedApiGet.mockResolvedValue([
            {
                basis_id: "bas_1",
                sales_order_id: "so_1",
                sales_order_no: "XS202608230001",
                customer_name: "客户甲",
                contract_no: "HT-2026-001",
                sales_owner_name: "张三",
                sales_order_revision_id: "sales-revision-1",
                supplier_id: "sup_1",
                supplier_name: "供应商A",
                payment_term_code: "POSTPAY_NET30",
                lines: [
                    {
                        procurement_confirmation_line_id: "confirmation-line-1",
                        sales_order_line_id: "sales-line-1",
                        sales_order_revision_line_id: "sales-revision-line-1",
                        sales_line_no: 1,
                        supplier_id: "sup_1",
                        confirmed_quantity: "10",
                        sales_quantity: "10",
                        covered_quantity: "4",
                        remaining_quantity: "6",
                        max_create_quantity: "5",
                        latest_cost_gross: "40",
                        input_tax_rate: "0.13",
                        expected_delivery_date: "2026-08-25",
                        product_name: "测试商品",
                        specification: "标准规格",
                        unit: "件",
                        gross_amount: "80",
                    },
                ],
                estimated_gross: "100",
            },
        ])

        const result = await fetchCreationBases({
            salesOrderId: "so_1",
            workItemId: "wi_1",
        })
        expect(mockedApiGet).toHaveBeenCalledTimes(1)
        expect(mockedApiGet).toHaveBeenCalledWith(
            "/admin/purchase-creation-bases",
            {
                sales_order_id: "so_1",
                work_item_id: "wi_1",
            },
        )
        expect(result).toHaveLength(1)
        expect(result[0]?.basisId).toBe("bas_1")
        expect(result[0]).toMatchObject({
            salesOrderNo: "XS202608230001",
            customerName: "客户甲",
            contractNumber: "HT-2026-001",
            salesOwnerName: "张三",
        })
        expect(result[0]?.lines[0]).toMatchObject({
            salesOrderLineId: "sales-line-1",
            salesOrderRevisionLineId: "sales-revision-line-1",
            itemName: "测试商品",
            itemSku: "标准规格",
            salesQuantity: "10",
            coveredQuantity: "4",
            remainingQuantity: "6",
            maxCreateQuantity: "5",
            unit: "件",
            salesAllocationLabel: "销售明细 1",
        })
        expect(result[0]?.lines[0]?.salesAllocationLabel).not.toContain(
            "sales-revision-line-1",
        )
    })
})
