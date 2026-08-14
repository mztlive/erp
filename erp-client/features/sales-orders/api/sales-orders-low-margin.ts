/**
 * W05 低毛利上级确认决定（mutationFn 纯函数）。
 *
 * 后端域：sales_order_review。失败统一抛 ApiError（@/lib/api）。
 */

import { apiPost } from "@/lib/api"
import type {
    BackendLowMarginManagerDecisionResult,
    LowMarginManagerDecisionOutcome,
} from "@/features/sales-orders/api/contracts"

type CompleteLowMarginManagerConfirmationInput = {
    salesOrderId: string
    workItemId: string
    taskVersion: string
    subjectVersion: string
    lowMarginSubmissionId: string
    rejectedProcurementConfirmationId: string
    expectedSalesOrderLockVersion: number
    idempotencyKey: string
} & (
    | { decision: "APPROVE"; comment?: string }
    | {
          decision: "REJECT"
          reasonCode: string
          comment: string
      }
)

/** 提交低毛利上级确认的唯一强类型决定。 */
export async function completeLowMarginManagerConfirmation(
    input: CompleteLowMarginManagerConfirmationInput,
): Promise<LowMarginManagerDecisionOutcome> {
    const decision =
        input.decision === "APPROVE"
            ? {
                  decision: "APPROVE",
                  work_item_type: "LOW_MARGIN_MANAGER_CONFIRMATION",
                  sales_order_id: input.salesOrderId,
                  rejected_procurement_confirmation_id:
                      input.rejectedProcurementConfirmationId,
                  low_margin_submission_id: input.lowMarginSubmissionId,
                  expected_sales_order_lock_version:
                      input.expectedSalesOrderLockVersion,
                  comment: input.comment?.trim() || null,
              }
            : {
                  decision: "REJECT",
                  work_item_type: "LOW_MARGIN_MANAGER_CONFIRMATION",
                  sales_order_id: input.salesOrderId,
                  rejected_procurement_confirmation_id:
                      input.rejectedProcurementConfirmationId,
                  low_margin_submission_id: input.lowMarginSubmissionId,
                  expected_sales_order_lock_version:
                      input.expectedSalesOrderLockVersion,
                  reason_code: input.reasonCode.trim(),
                  comment: input.comment.trim(),
              }
    const result = await apiPost<BackendLowMarginManagerDecisionResult>(
        "/admin/sales-order-reviews/low-margin-decisions",
        {
            work_item_id: input.workItemId,
            expected_task_version: input.taskVersion,
            expected_subject_version: input.subjectVersion,
            decision,
            idempotency_key: input.idempotencyKey,
        },
    )
    const business = result.business_result
    if (
        business.outcome === "LOW_MARGIN_APPROVED_AND_PROCUREMENT_RESUBMITTED"
    ) {
        return {
            outcome: business.outcome,
            salesOrderId: business.sales_order_id,
            lowMarginSubmissionId: business.low_margin_submission_id,
            salesOrderReviewId: business.sales_order_review_id,
            workflowActionId: business.workflow_action_id,
            newProcurementConfirmationId:
                business.new_procurement_confirmation_id,
            newProcurementWorkItemId: business.new_procurement_work_item_id,
        }
    }
    return {
        outcome: business.outcome,
        salesOrderId: business.sales_order_id,
        lowMarginSubmissionId: business.low_margin_submission_id,
        salesOrderReviewId: business.sales_order_review_id,
        workflowActionId: business.workflow_action_id,
    }
}
