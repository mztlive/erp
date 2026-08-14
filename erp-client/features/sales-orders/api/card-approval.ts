import { apiPost } from "@/lib/api"

import type {
    CancelCardSalesApprovalResult,
    SubmitCardSalesApprovalDecisionResult,
} from "./contracts"

type CardSalesApprovalDecisionBase = {
    salesOrderId: string
    salesOrderSubmissionId: string
    expectedSalesOrderLockVersion: number
    expectedSubmissionNo: number
    comment?: string
}

export type CardSalesApprovalDecision = CardSalesApprovalDecisionBase &
    (
        | {
              workItemType: "CARD_SALES_MANAGER_APPROVAL"
              expectedReviewStatus: "PENDING_SALES_LEAD"
              reviewDecision: "APPROVE"
          }
        | {
              workItemType: "CARD_SALES_MANAGER_APPROVAL"
              expectedReviewStatus: "PENDING_SALES_LEAD"
              reviewDecision: "REJECT"
              reasonCode: string
          }
        | {
              workItemType: "CARD_SALES_MANAGER_APPROVAL"
              expectedReviewStatus: "PENDING_SALES_LEAD"
              reviewDecision: "TERMINATE"
              reasonCode: string
          }
        | {
              workItemType: "CARD_SALES_OPERATION_APPROVAL"
              expectedReviewStatus: "PENDING_OPERATIONS"
              reviewDecision: "APPROVE"
          }
        | {
              workItemType: "CARD_SALES_OPERATION_APPROVAL"
              expectedReviewStatus: "PENDING_OPERATIONS"
              reviewDecision: "REJECT"
              reasonCode: string
          }
        | {
              workItemType: "CARD_SALES_OPERATION_APPROVAL"
              expectedReviewStatus: "PENDING_OPERATIONS"
              reviewDecision: "TERMINATE"
              reasonCode: string
          }
    )

export type SubmitCardSalesApprovalDecisionCommand = {
    approvalInstanceId: string
    expectedInstanceVersion: string
    approvalStepInstanceId: string
    expectedStepVersion: string
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    decision: CardSalesApprovalDecision
    idempotencyKey: string
}

export type CancelCardSalesApprovalCommand = {
    approvalInstanceId: string
    currentStepInstanceId: string
    workItemId?: string
    expectedInstanceVersion: string
    expectedStepVersion: string
    expectedTaskVersion?: string
    expectedSubjectVersion: string
    reason: string
    idempotencyKey: string
}

/**
 * 提交卡券销售当前活动步骤的唯一正式决定。
 *
 * 实例、步骤、任务和业务事实由同一命令携带并由服务端在一个事务中推进。
 */
export function submitCardSalesApprovalDecision(
    command: SubmitCardSalesApprovalDecisionCommand,
): Promise<SubmitCardSalesApprovalDecisionResult> {
    return apiPost<SubmitCardSalesApprovalDecisionResult>(
        "/admin/sales-order-reviews/decisions",
        {
            approval_instance_id: command.approvalInstanceId,
            expected_instance_version: command.expectedInstanceVersion,
            approval_step_instance_id: command.approvalStepInstanceId,
            expected_step_version: command.expectedStepVersion,
            work_item_id: command.workItemId,
            expected_task_version: command.expectedTaskVersion,
            expected_subject_version: command.expectedSubjectVersion,
            decision: {
                sales_order_id: command.decision.salesOrderId,
                sales_order_submission_id:
                    command.decision.salesOrderSubmissionId,
                expected_sales_order_lock_version:
                    command.decision.expectedSalesOrderLockVersion,
                expected_submission_no: command.decision.expectedSubmissionNo,
                work_item_type: command.decision.workItemType,
                expected_review_status: command.decision.expectedReviewStatus,
                review_decision: command.decision.reviewDecision,
                reason_code:
                    command.decision.reviewDecision === "APPROVE"
                        ? undefined
                        : command.decision.reasonCode,
                comment: command.decision.comment,
            },
            idempotency_key: command.idempotencyKey,
        },
    )
}

/**
 * 撤回尚未形成不可逆决定的卡券销售审批。
 *
 * 提交人身份由服务端注入；可见性只取销售单详情返回的 `CANCEL` 动作。
 */
export const cancelCardSalesApproval = (
    command: CancelCardSalesApprovalCommand,
): Promise<CancelCardSalesApprovalResult> =>
    apiPost<CancelCardSalesApprovalResult>(
        "/admin/sales-order-reviews/cancellations",
        {
            approval_instance_id: command.approvalInstanceId,
            current_step_instance_id: command.currentStepInstanceId,
            work_item_id: command.workItemId,
            expected_instance_version: command.expectedInstanceVersion,
            expected_step_version: command.expectedStepVersion,
            expected_task_version: command.expectedTaskVersion,
            expected_subject_version: command.expectedSubjectVersion,
            reason: command.reason,
            idempotency_key: command.idempotencyKey,
        },
    )
