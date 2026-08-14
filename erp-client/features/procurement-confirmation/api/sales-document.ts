/** W02 使用的采购确认业务展示摘要。 */

import { fetchConfirmationDetail, fetchSalesOrderDetail } from "./details"

/** 避免把内部对象 ID 直接展示给用户。 */
export type ProcurementWorkItemPresentation = {
    salesOrderNo: string
    customerName: string
    contractNo?: string
    paymentTermName: string
    grossAmount: string
}

/**
 * 解析采购确认待办对应的销售提交快照。
 *
 * @param confirmationId 采购确认批次 ID。
 * @returns 可直接用于待办列表的业务摘要；对象不存在时返回 null。
 */
export async function fetchProcurementWorkItemPresentation(
    confirmationId: string,
): Promise<ProcurementWorkItemPresentation | null> {
    const detail = await fetchConfirmationDetail(confirmationId)
    if (!detail) return null
    const sales = await fetchSalesOrderDetail(detail.sales_order_id)
    const submission = sales.submissions?.find(
        (row) => row.id === detail.submission_id,
    )
    if (!submission) return null
    return {
        salesOrderNo: sales.order_no,
        customerName: submission.customer_name,
        contractNo: submission.contract_no ?? undefined,
        paymentTermName: submission.payment_term_name,
        grossAmount: String(submission.gross_amount ?? "0"),
    }
}
