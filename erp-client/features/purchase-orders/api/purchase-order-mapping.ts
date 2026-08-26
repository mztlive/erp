/**
 * W08 采购单 · 后端 wire 对象 → 前端契约视图映射。
 * 契约形状保持 features/purchase-orders/types.ts 与 queries.ts 不变；
 * 缺口登记见 docs/dev-plan/p4-evidence/F4.md。
 */

import type {
    PurchaseChangeOrderSummary,
    PurchaseCreationBasis,
    PurchaseOrderCenterView,
    PurchaseOrderListItem,
} from "@/features/purchase-orders/types"
import {
    PO_STATUS_LABEL,
    PO_STATUS_TONE,
    REVIEW_STATUS_LABEL,
} from "@/features/purchase-orders/types"
import {
    mapPurchaseChangeOrderApproval,
    purchaseChangeOrderStatusLabel,
    purchaseChangeOrderStatusTone,
} from "@/features/purchase-orders/lib/purchase-change-order-approval"
import { mapPurchaseOrderApproval } from "@/features/purchase-orders/lib/purchase-order-approval"
import {
    deriveAllowedActions,
    fromBackendReviewStatus,
    fromBackendStatus,
    mapFulfillment,
    mapPurchaseType,
    parsePaymentTermSnapshot,
    paymentTermLabel,
    progressDisplay,
    secsToIso,
} from "./purchase-order-status"
import type {
    BackendBasis,
    BackendCenter,
    BackendListItem,
    BackendPurchaseChangeOrder,
} from "./purchase-order-wire-types"

export function mapListItem(row: BackendListItem): PurchaseOrderListItem {
    const status = fromBackendStatus(row.status)
    const reviewStatus = fromBackendReviewStatus(row.review_status, status)
    return {
        purchaseOrderId: row.id,
        purchaseNo: row.purchase_no || undefined,
        draftLabel:
            status === "DRAFT"
                ? `草稿 · ${row.purchase_no || row.id.slice(0, 8)}`
                : undefined,
        revisionNo: undefined,
        status,
        statusLabel: PO_STATUS_LABEL[status],
        statusTone: PO_STATUS_TONE[status],
        reviewStatus,
        reviewLabel: REVIEW_STATUS_LABEL[reviewStatus],
        salesOrderId: row.sales_order_id,
        salesOrderNo: row.sales_order_no,
        supplierId: row.supplier_id,
        supplierName: row.supplier_name,
        purchaseType: mapPurchaseType(String(row.purchase_type)),
        // 缺口：列表无履约责任
        fulfillmentResponsibility: "WAREHOUSE",
        paymentTermCode: row.payment_term_code ?? "",
        paymentTermLabel: paymentTermLabel(row.payment_term_code ?? ""),
        ownerName: row.owner_name?.trim() || "—",
        grossAmount: row.gross_amount ?? "0",
        netAmount: row.net_amount ?? "0",
        taxAmount: row.tax_amount ?? "0",
        costMasked: false,
        paymentProgress: progressDisplay(row.payment_progress, "payment"),
        invoiceProgress: progressDisplay(row.invoice_progress, "invoice"),
        fulfillmentProgress: progressDisplay(
            row.fulfillment_progress,
            "fulfillment",
        ),
        // 缺口：后端列表无先款门禁
        paymentGate: "NOT_APPLICABLE" as const,
        expectedDate: undefined,
        updatedAt: secsToIso(row.created_at),
        allowedActions: deriveAllowedActions(status),
        actionBlockers: [],
    }
}

/**
 * 把对象中心 wire 转成详情视图，并映射只读审批投影。
 *
 * 缺省审批结构保持 undefined，不得补默认审批人或节点。
 */
export function mapCenter(center: BackendCenter): PurchaseOrderCenterView {
    const status = fromBackendStatus(center.status)
    const reviewStatus = fromBackendReviewStatus(center.review_status, status)
    const contentSource =
        center.content_source === "SUBMISSION" ||
        center.content_source === "REVISION"
            ? center.content_source
            : "DRAFT"

    const lines = (center.lines ?? []).map((line) => ({
        lineId: line.line_id,
        lineType:
            line.line_type === "LOGISTICS_FEE"
                ? ("LOGISTICS_FEE" as const)
                : ("ITEM_SERVICE" as const),
        procurementConfirmationLineId:
            line.procurement_confirmation_line_id ?? undefined,
        itemName:
            line.product_name ??
            (line.line_type === "LOGISTICS_FEE" ? "物流费用" : "采购明细"),
        itemSku: line.specification ?? undefined,
        quantity: line.quantity ?? undefined,
        unit: line.base_unit_code ?? undefined,
        unitCostGross: line.unit_cost_gross ?? "0",
        inputTaxRate: line.input_tax_rate ?? "0",
        grossAmount: line.gross_amount ?? "0",
        netAmount: line.net_amount ?? "0",
        taxAmount: line.tax_amount ?? "0",
        expectedDeliveryDate: line.expected_delivery_date ?? undefined,
        logisticsFeeReason: undefined,
        salesAllocationLabel: line.sales_order_submission_line_id
            ? "已关联销售明细"
            : undefined,
    }))

    const fulfillmentLabel = progressDisplay(
        center.fulfillment_progress,
        "fulfillment",
    )
    const approval = mapPurchaseOrderApproval(center.approval)

    return {
        identity: {
            purchaseOrderId: center.id,
            purchaseNo: center.purchase_no || undefined,
            draftLabel:
                status === "DRAFT"
                    ? `草稿 · ${center.purchase_no || center.id.slice(0, 8)}`
                    : undefined,
            status,
            statusLabel: PO_STATUS_LABEL[status],
            statusTone: PO_STATUS_TONE[status],
            reviewStatus,
            reviewLabel: REVIEW_STATUS_LABEL[reviewStatus],
            lockVersion: center.version,
            currentSubmissionId: center.current_submission_id ?? undefined,
            currentRevisionId: center.current_revision_id ?? undefined,
            revisionNo: center.revision_no ?? undefined,
            subjectHash: center.current_submission_id ?? undefined,
        },
        header: {
            salesOrderId: center.sales_order_id,
            salesOrderNo: center.sales_order_no,
            supplierId: center.supplier_id,
            supplierSnapshot: center.supplier_name,
            purchaseType: mapPurchaseType(String(center.purchase_type)),
            fulfillmentResponsibility: mapFulfillment(
                String(center.fulfillment_responsibility),
            ),
            paymentTermCode: center.payment_term_code,
            paymentTermLabel: paymentTermLabel(center.payment_term_code),
            ownerName: "—",
            submittedBy: undefined,
            submittedAt: undefined,
            expectedDate: lines.find((l) => l.expectedDeliveryDate)
                ?.expectedDeliveryDate,
            creationBasisId: undefined,
        },
        progress: {
            payment: progressDisplay(center.payment_progress, "payment"),
            invoice: progressDisplay(center.invoice_progress, "invoice"),
            fulfillment: fulfillmentLabel,
            // 缺口：对象中心无 prepayment_gate 投影
            prepaymentGate: {
                state: "NOT_APPLICABLE",
                message:
                    "暂无法确认预付款要求，请刷新后重试；如仍失败，请联系财务或支持人员。",
                required: "0",
                allocated: "0",
                gap: "0",
                updatedAt: secsToIso(center.created_at),
            },
        },
        currentContent: {
            source: contentSource,
            version: center.revision_no ?? center.version,
            subjectHash: center.current_submission_id ?? undefined,
            lines,
            totals: {
                gross: center.totals?.gross ?? "0",
                net: center.totals?.net ?? "0",
                tax: center.totals?.tax ?? "0",
            },
            costMasked: false,
        },
        allocations: (center.allocations ?? []).map((a, index) => ({
            lineId: a.purchase_order_revision_line_id,
            salesOrderLineLabel: `销售分配 ${index + 1}`,
            allocatedQuantity: a.allocated_quantity,
        })),
        payableSummary: center.payable_summary
            ? {
                  payableOpenAmount: center.payable_summary.payable_open_amount,
                  paidAllocatedAmount:
                      center.payable_summary.paid_allocated_amount,
                  purchaseInvoiceAllocatedAmount:
                      center.payable_summary.purchase_invoice_allocated_amount,
              }
            : undefined,
        fulfillmentSummary: {
            progressLabel: fulfillmentLabel,
            progressTone:
                center.fulfillment_progress === "COMPLETED"
                    ? "success"
                    : center.fulfillment_progress === "PARTIAL"
                      ? "info"
                      : "neutral",
            inboundQty: "—",
            shippedQty: "—",
            remainingQty: "—",
        },
        changes: (center.changes ?? []).map((c) => ({
            changeId: c.change_id,
            label: c.reason || "采购变更",
            statusLabel: purchaseChangeOrderStatusLabel(c.status),
            tone: purchaseChangeOrderStatusTone(c.status),
            baseRevisionNo: undefined,
        })),
        workflow: [],
        approval,
        allowedActions: Array.from(
            new Set([
                ...deriveAllowedActions(status),
                ...(approval?.allowedActions ?? []),
            ]),
        ),
        actionBlockers: center.review_work_item?.action_blockers ?? [],
        fieldVisibility: {},
        reviewWorkItem:
            center.review_work_item?.work_item_type ===
                "PURCHASE_ORDER_REVIEW" &&
            center.review_work_item.subject_version &&
            center.review_work_item.task_version != null &&
            center.review_work_item.status === "OPEN"
                ? {
                      workItemId: center.review_work_item.work_item_id,
                      workItemType: center.review_work_item.work_item_type,
                      taskVersion: String(center.review_work_item.task_version),
                      subjectVersion: center.review_work_item.subject_version,
                      status: center.review_work_item.status,
                      ownerRole: center.review_work_item.owner_role,
                      ownerOrganizationId:
                          center.review_work_item.owner_organization_id,
                      ownerUserId:
                          center.review_work_item.owner_user_id ?? undefined,
                      processingState: center.review_work_item.processing_state,
                      domainAllowedActions:
                          center.review_work_item.processing_state === "READY"
                              ? (center.review_work_item
                                    .domain_allowed_actions ?? [])
                              : [],
                      actionBlockers:
                          center.review_work_item.action_blockers ?? [],
                  }
                : undefined,
    }
}

/**
 * 把采购变更单 wire 转成页面摘要，只透传服务端审批投影。
 *
 * @param row 列表行或详情。
 */
export function mapPurchaseChangeOrder(
    row: BackendPurchaseChangeOrder,
): PurchaseChangeOrderSummary {
    return {
        id: row.id,
        purchaseOrderId: row.purchase_order_id,
        statusLabel: purchaseChangeOrderStatusLabel(row.status),
        statusTone: purchaseChangeOrderStatusTone(row.status),
        statusCode: row.status,
        version: row.version,
        reason: row.reason,
        baseRevisionId: row.base_revision_id,
        createdAt: secsToIso(row.created_at),
        approval: mapPurchaseChangeOrderApproval(row.approval),
    }
}

export function mapBasis(basis: BackendBasis): PurchaseCreationBasis {
    const sourceType =
        basis.source_type === "EXISTING_STOCK" ? "EXISTING_STOCK" : "PURCHASE"
    const paymentTerm = parsePaymentTermSnapshot(
        basis.payment_term_code || "POSTPAY_NET30",
    )
    return {
        basisId: basis.basis_id,
        sourceType,
        workItemId: basis.work_item_id,
        salesOrderId: basis.sales_order_id,
        salesOrderNo: basis.sales_order_no,
        customerName: basis.customer_name,
        contractNumber: basis.contract_no ?? undefined,
        salesOwnerName: basis.sales_owner_name ?? undefined,
        salesOrderRevisionId: basis.sales_order_revision_id,
        supplierId: basis.supplier_id,
        supplierName: basis.supplier_name,
        stockBalanceId: basis.stock_balance_id ?? undefined,
        warehouseId: basis.warehouse_id ?? undefined,
        warehouseName: basis.warehouse_name ?? undefined,
        sourceAvailableQuantity: basis.source_available_quantity ?? undefined,
        purchaseType: mapPurchaseType(basis.purchase_type ?? "PHYSICAL"),
        fulfillmentResponsibility: mapFulfillment(
            basis.fulfillment_responsibility ?? "WAREHOUSE",
        ),
        paymentTermCode:
            sourceType === "EXISTING_STOCK"
                ? ""
                : basis.payment_term_code || "POSTPAY_NET30",
        paymentTermLabel:
            sourceType === "EXISTING_STOCK"
                ? "不适用"
                : paymentTerm.paymentTerm,
        businessCategory:
            sourceType === "EXISTING_STOCK"
                ? undefined
                : basis.business_category?.trim() ||
                  paymentTerm.businessCategory ||
                  undefined,
        lines: (basis.lines ?? []).map((line) => {
            const salesQuantity = String(
                line.sales_quantity ?? line.confirmed_quantity ?? "0",
            )
            const coveredQuantity = String(line.covered_quantity ?? "0")
            const remainingQuantity = String(
                line.remaining_quantity ?? line.confirmed_quantity ?? "0",
            )
            return {
                salesOrderLineId: line.sales_order_line_id,
                salesOrderRevisionLineId: line.sales_order_revision_line_id,
                itemName: line.product_name ?? "确认分行",
                itemSku: line.specification ?? undefined,
                salesQuantity,
                coveredQuantity,
                remainingQuantity,
                maxCreateQuantity: String(
                    line.max_create_quantity ?? remainingQuantity,
                ),
                unit: line.unit ?? "",
                unitCostGross: String(line.latest_cost_gross ?? "0"),
                inputTaxRate: String(line.input_tax_rate ?? "0"),
                expectedDeliveryDate: line.expected_delivery_date ?? "",
                salesDeliveryDeadline:
                    line.sales_delivery_deadline ??
                    line.expected_delivery_date ??
                    "",
                salesAllocationLabel: `销售明细 ${line.sales_line_no}`,
            }
        }),
        estimatedGross: basis.estimated_gross ?? "0",
        consumed: false,
    }
}
