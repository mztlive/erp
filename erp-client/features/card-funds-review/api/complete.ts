/**
 * W13 卡券票款复核 API · 提交复核结论
 * (POST /admin/receivable-funds-reviews)。
 */

import { apiPost } from "@/lib/api"
import type {
    CompleteCardFundsReviewCommand,
    FormalActionResponse,
} from "@/features/card-funds-review/types"

export async function completeCardFundsReview(
    input: CompleteCardFundsReviewCommand,
): Promise<FormalActionResponse> {
    try {
        const result = await apiPost<{
            work_item_id: string
            work_item_status: "COMPLETED"
            business_result: {
                receivable_funds_review_id: string
                receivable_account_id: string
                review_no: number
                account_review_status: string
                workflow_action_id: string
                operation_id: string
                completed_at: string
                review_result: "APPROVED" | "REJECTED"
                conclusion:
                    | "NO_HISTORY_FROM_ZERO"
                    | "RECORDED_FACTS_RECONCILED"
                    | "REJECTED"
                follow_up_configuration?: {
                    status: "BLOCKED"
                    blocker_code: "REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED"
                    collaboration_message: string
                    required_registration: readonly (
                        | "WORK_ITEM_TYPE"
                        | "OWNER_ASSIGNEE"
                        | "HANDLER_KEY"
                    )[]
                }
            }
        }>("/admin/receivable-funds-reviews", {
            work_item_id: input.workItemId,
            expected_task_version: input.expectedTaskVersion,
            expected_subject_version: input.expectedSubjectVersion,
            decision: {
                review_result: input.decision.reviewResult,
                conclusion: input.decision.conclusion,
                reason_code:
                    input.decision.reviewResult === "REJECTED"
                        ? input.decision.reasonCode
                        : undefined,
                receivable_account_id: input.decision.receivableAccountId,
                expected_account_seq: input.decision.expectedAccountSeq,
                expected_account_domain_version:
                    input.decision.expectedAccountDomainVersion,
                expected_review_chain_tail_id:
                    input.decision.expectedReviewChainTailId,
                expected_review_chain_version:
                    input.decision.expectedReviewChainVersion,
                expected_next_review_no: input.decision.expectedNextReviewNo,
                expected_sales_order_revision_id:
                    input.decision.expectedSalesOrderRevisionId,
                expected_funds_fact_version:
                    input.decision.expectedFundsFactVersion,
                review_type: input.decision.reviewType,
                evidence_document_ids: input.decision.evidenceDocumentIds,
                evidence_references: input.decision.evidenceReferences,
                comment: input.decision.comment,
            },
            idempotency_key: input.idempotencyKey,
        })
        const row = result.business_result
        if (
            result.work_item_id !== input.workItemId ||
            result.work_item_status !== "COMPLETED" ||
            !row?.receivable_funds_review_id ||
            !row.workflow_action_id ||
            !row.operation_id
        ) {
            return {
                status: "failed",
                code: "INCOMPLETE_FORMAL_RESULT",
                message: "任务、复核记录或操作号不完整；当前结果不能按成功展示",
            }
        }
        const businessBase = {
            receivableFundsReviewId: row.receivable_funds_review_id,
            receivableAccountId: row.receivable_account_id,
            reviewNo: row.review_no,
            accountReviewStatus: row.account_review_status,
            workflowActionId: row.workflow_action_id,
            operationId: row.operation_id,
            completedAt: row.completed_at,
        }
        if (row.review_result === "APPROVED") {
            if (
                row.conclusion !== "NO_HISTORY_FROM_ZERO" &&
                row.conclusion !== "RECORDED_FACTS_RECONCILED"
            ) {
                return {
                    status: "failed",
                    code: "INCOMPLETE_FORMAL_RESULT",
                    message: "通过复核的记录不完整，请刷新任务核对处理结果",
                }
            }
            return {
                status: "succeeded",
                outcome: {
                    kind: "APPROVED",
                    business: {
                        ...businessBase,
                        reviewResult: "APPROVED",
                        conclusion: row.conclusion,
                    },
                },
            }
        }
        if (
            row.review_result !== "REJECTED" ||
            row.conclusion !== "REJECTED" ||
            row.follow_up_configuration?.status !== "BLOCKED" ||
            row.follow_up_configuration.blocker_code !==
                "REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED"
        ) {
            return {
                status: "failed",
                code: "INCOMPLETE_FORMAL_RESULT",
                message: "驳回后的处理规则不完整；当前结果不能按成功展示",
            }
        }
        return {
            status: "succeeded",
            outcome: {
                kind: "REJECTED",
                business: {
                    ...businessBase,
                    reviewResult: "REJECTED",
                    conclusion: "REJECTED",
                    followUpConfiguration: {
                        status: row.follow_up_configuration.status,
                        blockerCode: row.follow_up_configuration.blocker_code,
                        collaborationMessage:
                            row.follow_up_configuration.collaboration_message,
                        requiredRegistration:
                            row.follow_up_configuration.required_registration,
                    },
                },
            },
        }
    } catch (err) {
        const message =
            err && typeof err === "object" && "message" in err
                ? String((err as { message: unknown }).message)
                : "完成复核失败"
        const status =
            err && typeof err === "object" && "status" in err
                ? (err as { status?: number }).status
                : undefined
        const kind =
            err && typeof err === "object" && "kind" in err
                ? (err as { kind?: unknown }).kind
                : undefined
        if (kind === "Network" || kind === "Parse") {
            return {
                status: "unknown",
                idempotencyKey: input.idempotencyKey,
                message:
                    "请求结果尚未确认；请按操作号查询处理结果，确认前不得再次推进任务",
            }
        }
        return {
            status: "failed",
            code:
                status === 409
                    ? "SUBJECT_HASH_MISMATCH"
                    : String(status ?? "HTTP_ERROR"),
            message,
        }
    }
}
