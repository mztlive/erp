"use client"

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import {
    usePaymentRequestQuery,
    usePaymentSubmissionQuery,
} from "../hooks/queries"
import type { PaymentTaskContext } from "../types"
import { RequestDetailContent } from "./request-detail-content"
import { RequestDetail } from "./request-detail"

/** 使用当前任务读取申请及限定来源，发起人历史跟踪使用普通申请资格。 */
export function PurchasePaymentRequestTask({
    item,
    onDecisionApplied,
    onTaskCompleted,
    onSelectFile,
    activeFileId,
    hideEmbeddedMaterials = false,
}: {
    item: PaymentTaskContext
    onDecisionApplied?: (view: ApprovalCommandView) => void
    onTaskCompleted?: () => void
    onSelectFile?: (fileId: string) => void
    activeFileId?: string
    hideEmbeddedMaterials?: boolean
}) {
    if (item.workItemType === "APPROVAL_INSTANCE")
        return (
            <PaymentSubmissionTask
                item={item}
                onSelectFile={onSelectFile}
                activeFileId={activeFileId}
                hideEmbeddedMaterials={hideEmbeddedMaterials}
            />
        )
    return (
        <CurrentPaymentTask
            item={item}
            onDecisionApplied={onDecisionApplied}
            onTaskCompleted={onTaskCompleted}
            onSelectFile={onSelectFile}
            activeFileId={activeFileId}
            hideEmbeddedMaterials={hideEmbeddedMaterials}
        />
    )
}

function CurrentPaymentTask({
    item,
    onDecisionApplied,
    onTaskCompleted,
    onSelectFile,
    activeFileId,
    hideEmbeddedMaterials = false,
}: {
    item: PaymentTaskContext
    onDecisionApplied?: (view: ApprovalCommandView) => void
    onTaskCompleted?: () => void
    onSelectFile?: (fileId: string) => void
    activeFileId?: string
    hideEmbeddedMaterials?: boolean
}) {
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
            <p role="status" className="py-5 text-xs text-muted-foreground">
                当前任务已处理，正在更新待办…
            </p>
        )
    if (query.isPending)
        return (
            <p className="py-5 text-xs text-muted-foreground">
                正在读取付款申请…
            </p>
        )
    if (query.isError)
        return (
            <div className="space-y-3 py-5">
                <p role="alert" className="text-body-compact text-destructive">
                    {getErrorMessage(query.error, "付款申请读取失败")}
                </p>
                <Button
                    id="ppr-task-retry"
                    variant="outline"
                    onClick={() => void query.refetch()}
                >
                    重试
                </Button>
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
    hideEmbeddedMaterials = false,
}: {
    item: PaymentTaskContext
    onSelectFile?: (fileId: string) => void
    activeFileId?: string
    hideEmbeddedMaterials?: boolean
}) {
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
            <p role="status" className="py-5 text-xs text-muted-foreground">
                正在读取本次付款申请…
            </p>
        )
    if (query.isError)
        return (
            <div className="space-y-3 py-5">
                <p role="alert">
                    {getErrorMessage(query.error, "本次付款申请读取失败")}
                </p>
                <Button
                    id="ppr-submission-task-retry"
                    variant="outline"
                    onClick={() => void query.refetch()}
                >
                    重试
                </Button>
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
        <RequestDetailContent
            request={request}
            embedded
            showHistory={false}
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
