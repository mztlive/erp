"use client"

import { useState, type ReactNode } from "react"
import { BusinessFailureState } from "@/components/business"
import { Spinner } from "@/components/ui/spinner"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import {
    usePaymentRequestQuery,
    usePaymentSubmissionQuery,
} from "../hooks/queries"
import type { PaymentRequest, PaymentTaskContext } from "../types"
import { PaymentRequestTaskReview, RequestDetail } from "./request-detail"
import { toAutomationIdSegment } from "@/lib/automation-id"

type PaymentRequestTaskProps = {
    item: PaymentTaskContext
    onDecisionApplied?: (view: ApprovalCommandView) => void
    onTaskCompleted?: () => void
    onSelectFile?: (fileId: string) => void
    activeFileId?: string
    hideEmbeddedMaterials?: boolean
    hasExternalPrimaryAction?: boolean
    approvalRoundNo?: number
    renderResubmission?: (request: PaymentRequest) => ReactNode
}

/** 使用当前任务读取申请及限定来源，发起人历史跟踪使用普通申请资格。 */
export function PurchasePaymentRequestTask(props: PaymentRequestTaskProps) {
    const { item } = props
    return (
        <div className="@container/payment-request min-w-0 bg-card">
            {item.workItemType === "APPROVAL_INSTANCE" ? (
                <PaymentSubmissionTask key={item.workItemId} {...props} />
            ) : (
                <CurrentPaymentTask key={item.workItemId} {...props} />
            )}
        </div>
    )
}

function CurrentPaymentTask({
    item,
    onDecisionApplied,
    onTaskCompleted,
    onSelectFile,
    activeFileId,
    hideEmbeddedMaterials,
    hasExternalPrimaryAction,
    approvalRoundNo,
    renderResubmission,
}: PaymentRequestTaskProps) {
    const [completed, setCompleted] = useState(false)
    const scoped =
        item.workItemType !== "APPROVAL_INSTANCE" &&
        item.allowedActions.some(
            (action) =>
                action === "PROCESS" ||
                action === "APPROVE" ||
                action === "REJECT",
        )
    const query = usePaymentRequestQuery(
        item.businessObjectId,
        scoped ? item.workItemId : undefined,
        !completed,
    )
    // 完成后旧任务不再具备材料读取资格，等待队列切换时不展示其刷新错误。
    if (completed)
        return (
            <p
                role="status"
                className="flex items-center gap-2 py-5 text-xs text-muted-foreground"
            >
                <Spinner />
                当前任务已处理，正在更新待办…
            </p>
        )
    if (query.isPending)
        return (
            <p
                role="status"
                className="flex items-center gap-2 py-5 text-xs text-muted-foreground"
            >
                <Spinner />
                正在读取付款申请…
            </p>
        )
    if (query.isError)
        return (
            <div className="py-5">
                <BusinessFailureState
                    id={`ppr-task-${toAutomationIdSegment(item.workItemId)}-retry`}
                    title="付款申请读取失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            </div>
        )
    return (
        <RequestDetail
            key={query.data.id}
            request={query.data}
            task={item}
            onSelectFile={onSelectFile}
            activeFileId={activeFileId}
            hideEmbeddedMaterials={hideEmbeddedMaterials}
            hasExternalPrimaryAction={hasExternalPrimaryAction}
            approvalRoundNo={approvalRoundNo}
            renderResubmission={renderResubmission}
            onDecisionApplied={(view) => {
                setCompleted(true)
                onDecisionApplied?.(view)
            }}
            onTaskCompleted={() => {
                setCompleted(true)
                onTaskCompleted?.()
            }}
        />
    )
}

/** 历史审批复用付款申请明细，申请及审核材料均锁定本次提交版本。 */
function PaymentSubmissionTask({
    item,
    onSelectFile,
    activeFileId,
    hideEmbeddedMaterials,
    approvalRoundNo,
    renderResubmission,
}: PaymentRequestTaskProps) {
    const version = Number(item.subjectVersion)
    const validVersion = Number.isSafeInteger(version) && version > 0
    const query = usePaymentSubmissionQuery(
        item.businessObjectId,
        validVersion ? version : null,
    )
    if (!validVersion)
        return (
            <p role="alert" className="py-5 text-body-compact text-destructive">
                本次审批缺少提交版本，请刷新后重试。
            </p>
        )
    if (query.isPending)
        return (
            <p
                role="status"
                className="flex items-center gap-2 py-5 text-xs text-muted-foreground"
            >
                <Spinner />
                正在读取本次付款申请…
            </p>
        )
    if (query.isError)
        return (
            <div className="py-5">
                <BusinessFailureState
                    id={`ppr-submission-task-${toAutomationIdSegment(item.workItemId)}-retry`}
                    title="本次付款申请读取失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            </div>
        )
    const request = query.data
    if (
        request.id !== item.businessObjectId ||
        request.submission_version !== version ||
        request.approval_instance_id !== item.approvalProcessInstanceId
    )
        return (
            <p role="alert" className="py-5 text-body-compact text-destructive">
                付款申请与本次审批不一致，请刷新后重试。
            </p>
        )
    return (
        <PaymentRequestTaskReview
            request={request}
            idPrefix={`ppr-review-${toAutomationIdSegment(item.workItemId)}`}
            approvalRoundNo={approvalRoundNo}
            resubmission={renderResubmission?.(request)}
            submissionVersion={version}
            onSelectFile={onSelectFile}
            activeFileId={activeFileId}
            hideEmbeddedMaterials={hideEmbeddedMaterials}
            account={
                <span className="num">
                    {request.payment_recipient?.account_number_masked ?? "—"}
                </span>
            }
            approval={null}
        />
    )
}
