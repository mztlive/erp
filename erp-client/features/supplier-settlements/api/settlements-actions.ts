/**
 * W27 API 供应商结算 · 命令动作（创建/刷新/证据/结论/复核）
 * 从 api/settlements.ts 拆出；请求体与结果映射保持不变。
 */

import { apiGet, apiPost } from "@/lib/api"
import type {
    AppendEvidenceInput,
    CreateDraftInput,
    FormalOutcome,
    RefreshDraftInput,
    ResolveDifferenceInput,
    ReviewDecisionInput,
    SubmitReviewInput,
} from "@/features/supplier-settlements/types"
import { RESOLUTION_TO_STATUS } from "@/features/supplier-settlements/types"
import type {
    BackendDifferenceDecisionResult,
    BackendDraftCommandResult,
    BackendEvidenceResult,
    BackendReviewDecisionResult,
    BackendReviewSubmissionResult,
    BackendSourceEvidence,
} from "@/features/supplier-settlements/api/settlements-wire"

export async function createSettlementDraft(
    input: CreateDraftInput,
): Promise<FormalOutcome> {
    try {
        const source = await apiGet<BackendSourceEvidence>(
            "/admin/supplier-settlement-source-evidence",
            {
                supplier_id: input.supplierId,
                period_start: input.periodStart,
                period_end: input.periodEnd,
            },
        )
        const result = await apiPost<BackendDraftCommandResult>(
            "/admin/supplier-settlement-statements",
            {
                action: "CREATE",
                supplier_id: input.supplierId,
                period_start: input.periodStart,
                period_end: input.periodEnd,
                period_policy_id: source.period_policy_id,
                expected_period_policy_version: source.period_policy_version,
                request_id: input.requestId,
                idempotency_key: input.idempotencyKey,
            },
        )
        const created = result.statement
        return {
            status: "succeeded",
            title: "结算草稿已创建",
            message: result.message,
            reference: created.statement_no,
            statementId: created.id,
            lockVersion: created.version,
            facts: [
                { label: "结算单号", value: created.statement_no },
                { label: "供应商", value: created.supplier_id },
                {
                    label: "期间",
                    value: `${input.periodStart} ~ ${input.periodEnd}`,
                },
                { label: "来源行数", value: String(result.item_count) },
            ],
        }
    } catch (err) {
        const message =
            err && typeof err === "object" && "message" in err
                ? String((err as { message: string }).message)
                : "创建草稿失败"
        if (
            message.includes("SOURCE_EVIDENCE_MISSING") ||
            message.includes("来源证据")
        ) {
            return {
                status: "blocked",
                code: "SOURCE_EVIDENCE_MISSING",
                title: "来源证据尚未完备",
                message:
                    "当前供应商与期间尚无完整来源证据批次，请先通过来源证据录入命令补齐履约、退款、费用与账单行证据。",
            }
        }
        throw err
    }
}

export async function refreshSettlementTrial(
    input: RefreshDraftInput,
): Promise<FormalOutcome> {
    const result = await apiPost<BackendDraftCommandResult>(
        `/admin/supplier-settlement-statements/${encodeURIComponent(input.statementId)}/refreshes`,
        {
            action: "REFRESH",
            statement_id: input.statementId,
            expected_lock_version: input.expectedLockVersion,
            expected_source_snapshot_hash: input.expectedSourceSnapshotHash,
            request_id: input.requestId,
            idempotency_key: input.idempotencyKey,
        },
    )
    return {
        status: "succeeded",
        title:
            result.result_status === "UNCHANGED"
                ? "试算已是最新"
                : "试算已刷新",
        message: result.message,
        reference: result.statement.statement_no,
        statementId: result.statement.id,
        lockVersion: result.statement.version,
        sourceSnapshotHash: result.statement.source_snapshot_hash ?? undefined,
        subjectHash: result.statement.subject_hash ?? undefined,
    }
}

export async function appendDifferenceEvidence(
    input: AppendEvidenceInput,
): Promise<FormalOutcome> {
    const result = await apiPost<BackendEvidenceResult>(
        `/admin/supplier-settlement-differences/${encodeURIComponent(input.differenceId)}/evidence`,
        {
            statement_id: input.statementId,
            difference_id: input.differenceId,
            expected_difference_version: input.expectedDifferenceVersion,
            evidence_reference_ids: input.evidenceReferenceIds,
            opinion_code: input.opinionCode,
            comment: input.comment,
            request_id: input.requestId,
            idempotency_key: input.idempotencyKey,
        },
    )
    return {
        status: "succeeded",
        title: "差异证据已登记",
        message: result.message,
        reference: result.evidence.evidence_id,
        statementId: result.statement_id,
    }
}

export async function resolveDifference(
    input: ResolveDifferenceInput,
): Promise<FormalOutcome> {
    const result = await apiPost<BackendDifferenceDecisionResult>(
        `/admin/supplier-settlement-differences/${encodeURIComponent(input.differenceId)}/decisions`,
        {
            statement_id: input.statementId,
            difference_id: input.differenceId,
            expected_lock_version: input.expectedLockVersion,
            expected_difference_version: input.expectedDifferenceVersion,
            resolution: input.resolution,
            reason_code: input.reasonCode,
            evidence_reference_ids: input.evidenceReferenceIds,
            operation_id: input.operationId,
            idempotency_key: input.idempotencyKey,
        },
    )

    return {
        status: result.result_status === "RESOLVED" ? "succeeded" : "unknown",
        title:
            result.result_status === "RESOLVED"
                ? "差异结论已登记"
                : "差异处理结果待确认",
        message: result.message,
        reference: result.operation_id,
        operationId: result.operation_id,
        statementId: result.statement_id,
        lockVersion: result.statement_lock_version,
        facts: [
            {
                label: "结论",
                value: RESOLUTION_TO_STATUS[input.resolution],
            },
            { label: "原因", value: input.reasonCode },
        ],
    }
}

export async function submitSettlementReview(
    input: SubmitReviewInput,
): Promise<FormalOutcome> {
    const result = await apiPost<BackendReviewSubmissionResult>(
        `/admin/supplier-settlement-statements/${encodeURIComponent(input.statementId)}/review-submissions`,
        {
            action: "SUBMIT_REVIEW",
            statement_id: input.statementId,
            expected_lock_version: input.expectedLockVersion,
            subject_hash: input.subjectHash,
            refresh_cutoff_policy_id: input.refreshCutoffPolicyId,
            expected_refresh_cutoff_policy_version:
                input.expectedRefreshCutoffPolicyVersion,
            reviewer_user_id: input.reviewerUserId,
            operation_id: input.operationId,
            idempotency_key: input.idempotencyKey,
            comment: input.comment,
        },
    )
    return {
        status: result.result_status === "SUBMITTED" ? "succeeded" : "unknown",
        title:
            result.result_status === "SUBMITTED"
                ? "已提交复核"
                : "提交复核结果待确认",
        message: result.message,
        reference: result.work_item_id ?? result.operation_id,
        operationId: result.operation_id,
        statementId: result.statement.id,
        lockVersion: result.statement.version,
    }
}

export async function decideSettlementReview(
    input: ReviewDecisionInput,
): Promise<FormalOutcome> {
    const result = await apiPost<BackendReviewDecisionResult>(
        `/admin/supplier-settlement-statements/${encodeURIComponent(input.statementId)}/review-decisions`,
        {
            work_item_id: input.workItemId,
            expected_task_version: input.expectedTaskVersion,
            expected_subject_version: input.expectedSubjectVersion,
            decision: {
                statement_id: input.statementId,
                expected_lock_version: input.expectedLockVersion,
                action: input.action,
                operation_id: input.operationId,
                reason_code: input.reasonCode,
                comment: input.comment,
            },
            idempotency_key: input.idempotencyKey,
        },
    )
    return {
        status:
            result.result_status === "UNKNOWN"
                ? "unknown"
                : result.result_status === "REJECTED"
                  ? "rejected"
                  : "succeeded",
        title:
            result.result_status === "UNKNOWN"
                ? "复核结果待确认"
                : result.result_status === "REJECTED"
                  ? "结算已驳回"
                  : "结算已确认",
        message: result.message,
        reference: result.payable_no ?? result.operation_id,
        operationId: result.operation_id,
        statementId: result.statement.id,
        payableNo: result.payable_no ?? undefined,
        payableAccountId: result.payable_account_id ?? undefined,
        costDeltaGross: result.cost_delta_gross ?? undefined,
        lockVersion: result.statement.version,
    }
}
