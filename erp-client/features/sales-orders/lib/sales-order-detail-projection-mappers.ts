import type {
    BackendSalesChangeOrder,
    BackendSalesOrderDetail,
    BackendSubmission,
    BackendWorkingCopy,
    BackendWorkingCopyLine,
} from "@/features/sales-orders/api/contracts"
import type {
    SalesChangeOrderSummary,
    SalesOrderListItem,
    SalesOrderNature,
} from "@/features/sales-orders/types"
import {
    mapSalesChangeOrderApproval,
    salesChangeOrderStatusLabel,
} from "@/features/sales-orders/lib/sales-change-order-approval"
import { mapSalesOrderApproval } from "@/features/sales-orders/lib/sales-order-approval"
import { mapVoucherSalesOrderApproval } from "@/features/sales-orders/lib/voucher-sales-order-approval"
import { multiplyFixed } from "@/lib/fixed-decimal"
import {
    formatEpochDate,
    mapListItemFromBackend,
    mapRevisions,
    mapWorkingCopyLines,
    toneForStatus,
} from "@/features/sales-orders/lib/sales-order-detail-mappers"

/**
 * 表头履约期限：卡券读 `voucher_expiry_at`；实物/服务从明细 `fulfillment_due_at` 汇总。
 * 多行不同日期时展示最早~最晚。
 */
function resolveFulfillmentDeadline(
    voucherExpiryAt: number | null | undefined,
    lines: BackendWorkingCopyLine[],
): string {
    const fromVoucher = formatEpochDate(voucherExpiryAt)
    if (fromVoucher) return fromVoucher

    const dates = Array.from(
        new Set(
            lines
                .map((line) => formatEpochDate(line.fulfillment_due_at))
                .filter(Boolean),
        ),
    ).sort()
    if (dates.length === 0) return ""
    if (dates.length === 1) return dates[0] ?? ""
    return `${dates[0]} ~ ${dates[dates.length - 1]}`
}

/** 将后端小数税率转换为建单页使用的百分数展示；缺失或非法值保持为空。 */
function taxRatePercent(rate: string | null | undefined): string {
    if (!rate?.trim()) return ""
    try {
        return multiplyFixed(rate, "100", {
            leftMaxScale: 6,
            rightMaxScale: 0,
            outputScale: 2,
        })
    } catch {
        return ""
    }
}

function latestSubmission(
    detail: BackendSalesOrderDetail,
): BackendSubmission | undefined {
    return [...(detail.submissions ?? [])].sort(
        (a, b) => (b.submission_no ?? 0) - (a.submission_no ?? 0),
    )[0]
}

/**
 * 选择详情当前应展示的商业快照：可编辑工作副本优先，否则取最新提交。
 * 合同精确修订等附属显示必须复用同一来源，避免字段跨版本拼接。
 */
export function pickSalesOrderCommercialSource(
    detail: BackendSalesOrderDetail,
): BackendWorkingCopy | BackendSubmission | undefined {
    if (detail.working_copy?.lines?.length) return detail.working_copy
    return latestSubmission(detail)
}

/**
 * 详情商业内容优先：可编辑草稿 → 最新提交快照。
 * 提交后 working_copy 为空时必须从 submission 回填明细与表头。
 */
function pickCommercialContent(detail: BackendSalesOrderDetail): {
    lines: BackendWorkingCopyLine[]
    amountGross?: string
    amountNet?: string
    taxAmount?: string
    ownerUserId: string
    customerName?: string
    contractNo?: string
    settlementPartyName?: string
    paymentTerms: string
    taxRatePercent: string
    welfareScene: string
    fulfillmentDeadline: string
    receivableDueDate?: string
    remark?: string
} {
    const source = pickSalesOrderCommercialSource(detail)
    const submitted = latestSubmission(detail)
    if (source) {
        const lines = source.lines ?? []
        const ownerUserId =
            "editor_user_id" in source
                ? source.editor_user_id || submitted?.submitted_by || ""
                : source.submitted_by || ""
        return {
            lines,
            amountGross: source.gross_amount,
            amountNet: source.net_amount,
            taxAmount: source.tax_amount,
            ownerUserId,
            customerName: source.customer_name || undefined,
            contractNo: source.contract_no || undefined,
            settlementPartyName: source.settlement_party_name || undefined,
            paymentTerms:
                source.payment_term_name || source.payment_term_code || "",
            taxRatePercent: taxRatePercent(lines[0]?.sales_tax_rate),
            welfareScene: source.project_name || "",
            fulfillmentDeadline: resolveFulfillmentDeadline(
                source.voucher_expiry_at,
                lines,
            ),
            receivableDueDate: source.receivable_due_date || undefined,
            remark: source.business_remark || undefined,
        }
    }

    return {
        lines: [],
        ownerUserId: "",
        paymentTerms: "",
        taxRatePercent: "",
        welfareScene: "",
        fulfillmentDeadline: "",
    }
}

export function mapDetailToListItem(
    detail: BackendSalesOrderDetail,
    extras?: {
        customerName?: string
        contractNumber?: string
        contractRevisionLabel?: string
        ownerUserId?: string
        ownerName?: string
        activeChangeOrder?: SalesChangeOrderSummary | null
        customerContact?: string
    },
): SalesOrderListItem {
    const commercial = pickCommercialContent(detail)
    // 正式销售版本只认 `current_revision_id` 指向的不可变修订。
    // 实体乐观锁 `version` 会随保存、提交和状态流转递增，绝不能作为业务版本兜底。
    const currentRevisionNo = detail.current_revision_id
        ? (detail.revisions?.find(
              (revision) => revision.id === detail.current_revision_id,
          )?.revision_no ?? null)
        : null
    return mapListItemFromBackend(
        {
            id: detail.id,
            order_no: detail.order_no,
            business_type: detail.business_type,
            origin_system: detail.origin_system,
            customer_id: detail.customer_id,
            contract_id: detail.contract_id,
            commercial_status: detail.commercial_status,
            review_status: detail.review_status,
            fulfillment_progress: detail.fulfillment_progress,
            collection_progress: detail.collection_progress,
            invoice_progress: detail.invoice_progress,
            close_status: detail.close_status,
            effective_at: detail.effective_at,
            version: detail.version,
            created_at: detail.created_at,
            updated_at: detail.created_at,
            stage: detail.stage,
            purchase_coverage: detail.purchase_coverage,
        },
        {
            customerName:
                commercial.customerName ||
                extras?.customerName ||
                detail.customer_id,
            contractNumber:
                commercial.contractNo || extras?.contractNumber || "",
            contractRevisionLabel:
                extras?.contractRevisionLabel ||
                commercial.contractNo ||
                extras?.contractNumber ||
                "",
            contractCompanyName:
                commercial.customerName || extras?.customerName || "",
            amountGross: commercial.amountGross,
            amountNet: commercial.amountNet,
            taxAmount: commercial.taxAmount,
            receivedAmount: detail.settled_total,
            invoicedAmount: detail.invoiced_total,
            lineItems: mapWorkingCopyLines(commercial.lines),
            ownerUserId: extras?.ownerUserId || detail.owner_user_id || "",
            ownerName: extras?.ownerName || "",
            customerContact: extras?.customerContact,
            paymentTerms: commercial.paymentTerms,
            taxRatePercent: commercial.taxRatePercent,
            welfareScene: commercial.welfareScene,
            fulfillmentDeadline: commercial.fulfillmentDeadline,
            receivableDueDate: commercial.receivableDueDate,
            remark: commercial.remark,
            revisions: mapRevisions(detail.revisions),
            approval:
                detail.business_type === "VOUCHER"
                    ? mapVoucherSalesOrderApproval(detail.approval)
                    : mapSalesOrderApproval(detail.approval),
            activeChangeOrder: extras?.activeChangeOrder,
            settlementEntity:
                commercial.settlementPartyName || detail.settlement_party_id,
            closeEligibility: detail.close_eligibility,
            startSalesChange: {
                allowed: detail.can_start_sales_change_order,
                blocker: detail.change_order_blocker,
            },
            currentRevisionNo,
            purchaseOrderCount: detail.purchase_order_count,
            purchaseCreationAccess: {
                allowed: detail.purchase_creation_access.allowed,
                taskCount: detail.purchase_creation_access.task_count,
                blocker: detail.purchase_creation_access.blocker ?? undefined,
            },
        },
    )
}

/**
 * 把销售变更单列表/详情行映射为页面摘要。
 *
 * 审批投影只透传服务端结构；状态中文由统一映射表产出，不按影响路径推导节点。
 *
 * @param row 后端变更单行或详情。
 * @param nature 原销售单业务性质，仅保留旧摘要字段兼容。
 */
export function mapChangeOrder(
    row: BackendSalesChangeOrder,
    nature: SalesOrderNature,
): SalesChangeOrderSummary {
    const impactPath = nature === "card_voucher" ? "operations" : "procurement"
    const statusLabel = salesChangeOrderStatusLabel(row.status)
    return {
        id: row.id,
        statusLabel,
        statusTone: toneForStatus(statusLabel),
        statusCode: row.status,
        version: row.version,
        baseRevisionNo: 0,
        createdAt: new Date(row.created_at * 1000).toISOString(),
        impactPath,
        approval: mapSalesChangeOrderApproval(row.approval),
    }
}
