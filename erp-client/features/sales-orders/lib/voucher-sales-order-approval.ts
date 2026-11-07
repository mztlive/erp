import type { SalesOrderApprovalPhase } from "@/features/sales-orders/lib/sales-order-approval"
import {
    mapDocumentApprovalViewDto,
    type DocumentApprovalView,
    type DocumentApprovalViewDto,
} from "@/features/approval-workflow/types"

/** 卡券销售单作为合同 DocumentType 的固定值。 */
export const VOUCHER_SALES_ORDER_DOCUMENT_TYPE = "VoucherSalesOrder" as const

export type VoucherSalesOrderApprovalPhase = SalesOrderApprovalPhase

/**
 * 把卡券销售单详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省返回 undefined，禁止前端补默认审批人或节点。
 *
 * @param dto 详情内嵌的审批 DTO。
 */
export const mapVoucherSalesOrderApproval = (
    dto?: DocumentApprovalViewDto | null,
): DocumentApprovalView | undefined =>
    dto ? mapDocumentApprovalViewDto(dto) : undefined
