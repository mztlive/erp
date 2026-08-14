import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

/** 测试用采购单中心视图构造器；缺省为一笔可编辑草稿。 */
export function makePurchaseOrderCenter(
    overrides: Partial<PurchaseOrderCenterView> = {},
): PurchaseOrderCenterView {
    return {
        identity: {
            purchaseOrderId: "po-1",
            purchaseNo: "PO-2026-001",
            draftLabel: "草稿采购单",
            status: "DRAFT",
            statusLabel: "草稿",
            statusTone: "neutral",
            reviewStatus: "NONE",
            reviewLabel: "—",
            lockVersion: 3,
            currentSubmissionId: "sub-1",
            revisionNo: 2,
        },
        header: {
            salesOrderId: "so-1",
            salesOrderNo: "SO-001",
            supplierId: "sup-1",
            supplierSnapshot: "示例供应商有限公司",
            purchaseType: "PHYSICAL",
            fulfillmentResponsibility: "WAREHOUSE",
            paymentTermCode: "POSTPAY_NET15",
            paymentTermLabel: "货到 15 天",
            ownerName: "经办人",
            submittedBy: "张三",
            submittedAt: "2026-08-01T08:00:00.000Z",
            expectedDate: "2026-09-01",
        },
        progress: {
            payment: "未付",
            invoice: "未开票",
            fulfillment: "未开始",
            prepaymentGate: {
                state: "NOT_APPLICABLE",
                message: "无需先款",
                required: "0",
                allocated: "0",
                gap: "0",
                updatedAt: "2026-08-01T08:00:00.000Z",
            },
        },
        currentContent: {
            source: "DRAFT",
            version: 3,
            subjectHash: "hash-3",
            lines: [
                {
                    lineId: "line-1",
                    lineType: "ITEM_SERVICE",
                    itemName: "示例商品",
                    quantity: "10",
                    unit: "件",
                    unitCostGross: "100.00",
                    inputTaxRate: "0.13",
                    grossAmount: "1130.00",
                    netAmount: "1000.00",
                    taxAmount: "130.00",
                },
            ],
            totals: { gross: "1130.00", net: "1000.00", tax: "130.00" },
            costMasked: false,
        },
        allocations: [],
        payableSummary: {
            payableOpenAmount: "1130.00",
            paidAllocatedAmount: "0.00",
            purchaseInvoiceAllocatedAmount: "0.00",
        },
        fulfillmentSummary: {
            progressLabel: "未开始",
            progressTone: "neutral",
            inboundQty: "0",
            shippedQty: "0",
            remainingQty: "10",
        },
        changes: [],
        workflow: [],
        allowedActions: ["EDIT", "SUBMIT"],
        actionBlockers: [],
        fieldVisibility: {},
        ...overrides,
    }
}
