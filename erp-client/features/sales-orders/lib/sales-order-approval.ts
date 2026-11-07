import {
    mapDocumentApprovalViewDto,
    type DocumentApprovalView,
    type DocumentApprovalViewDto,
} from "@/features/approval-workflow/types"

/** 实物及服务销售单作为合同 DocumentType 的固定值。 */
export const SALES_ORDER_DOCUMENT_TYPE = "SalesOrder" as const

export type SalesOrderApprovalPhase = "draft" | "confirm" | "runtime"

/**
 * 把销售单详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省返回 undefined，禁止前端补默认审批人或节点。
 *
 * @param dto 详情内嵌的审批 DTO。
 */
export const mapSalesOrderApproval = (
    dto?: DocumentApprovalViewDto | null,
): DocumentApprovalView | undefined =>
    dto ? mapDocumentApprovalViewDto(dto) : undefined
