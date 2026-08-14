import { z } from "zod"

import type { CardSalesApprovalDecision } from "@/features/sales-orders/api/card-approval"
import type {
    CardSalesApproval,
    SalesOrderListItem,
} from "@/features/sales-orders/types"

/**
 * 卡券销售审批的纯模型：命令键、决定构造、结果不确定判定与驳回表单校验。
 * 不含任何 React 状态，由 use-card-approval-actions 消费。
 */

export const rejectSchema = z.object({
    reasonCode: z.string().trim().min(2, "请填写驳回原因（简短分类即可）"),
    comment: z.string().trim().min(4, "请填写驳回说明"),
})

export type ApprovalResult = {
    status: "succeeded" | "rejected" | "blocked" | "unknown"
    title: string
    description: string
    reference: string
    nextResponsible?: string
}

export function actionKey(
    approval: Extract<CardSalesApproval, { processingState: "READY" }>,
    action: "START_PROCESSING" | "APPROVE" | "REJECT" | "TERMINATE",
): string {
    return `w05:${approval.workItemId}:${approval.taskVersion}:${action}`
}

/** 由服务端投影版本构造可安全重试的撤回操作键。 */
export const cancelActionKey = (approval: CardSalesApproval): string =>
    `w05:${approval.approvalInstanceId}:${approval.instanceVersion}:${approval.approvalStepInstanceId}:${approval.stepVersion}:CANCEL`

export function approvalDecision(
    order: SalesOrderListItem,
    approval: Extract<CardSalesApproval, { processingState: "READY" }>,
    reviewDecision: "APPROVE" | "REJECT" | "TERMINATE",
    decisionReason?: { reasonCode: string; comment: string },
): CardSalesApprovalDecision {
    const common = {
        salesOrderId: order.id,
        salesOrderSubmissionId: approval.salesOrderSubmissionId,
        expectedSalesOrderLockVersion: order.lockVersion,
        expectedSubmissionNo: approval.submissionNo,
        comment: decisionReason?.comment,
    }

    if (approval.workItemType === "CARD_SALES_MANAGER_APPROVAL") {
        return reviewDecision === "APPROVE"
            ? {
                  ...common,
                  workItemType: "CARD_SALES_MANAGER_APPROVAL",
                  expectedReviewStatus: "PENDING_SALES_LEAD",
                  reviewDecision: "APPROVE",
              }
            : {
                  ...common,
                  workItemType: "CARD_SALES_MANAGER_APPROVAL",
                  expectedReviewStatus: "PENDING_SALES_LEAD",
                  reviewDecision,
                  reasonCode: decisionReason?.reasonCode ?? "",
              }
    }

    return reviewDecision === "APPROVE"
        ? {
              ...common,
              workItemType: "CARD_SALES_OPERATION_APPROVAL",
              expectedReviewStatus: "PENDING_OPERATIONS",
              reviewDecision: "APPROVE",
          }
        : {
              ...common,
              workItemType: "CARD_SALES_OPERATION_APPROVAL",
              expectedReviewStatus: "PENDING_OPERATIONS",
              reviewDecision,
              reasonCode: decisionReason?.reasonCode ?? "",
          }
}

export function isUncertainResult(error: unknown): boolean {
    if (!error || typeof error !== "object" || !("kind" in error)) {
        return false
    }
    const kind = (error as { kind?: unknown }).kind
    return kind === "Network" || kind === "Parse"
}
