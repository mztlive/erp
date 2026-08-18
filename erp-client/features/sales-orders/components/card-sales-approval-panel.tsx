"use client"

import { DocumentSection } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    CardApprovalApproveConfirmDialog,
    CardApprovalCancelConfirmDialog,
    CardApprovalRejectConfirmDialog,
    CardApprovalTerminateConfirmDialog,
} from "@/features/sales-orders/components/card-sales-approval-dialogs"
import {
    CardApprovalRejectForm,
    CardApprovalTerminateForm,
} from "@/features/sales-orders/components/card-sales-approval-forms"
import {
    useCardSalesApprovalActions,
    type ApprovalResult,
} from "@/features/sales-orders/hooks/use-card-approval-actions"
import { CARD_APPROVAL_TYPE_LABEL } from "@/features/sales-orders/lib/labels"
import type {
    CardSalesApproval,
    SalesOrderListItem,
} from "@/features/sales-orders/types"
import type { WorkItemStatus } from "@/features/work-items"

const WORK_ITEM_STATUS_LABEL: Record<WorkItemStatus, string> = {
    OPEN: "待处理",
    COMPLETED: "已完成",
    CLOSED: "已关闭",
}

const EXPECTED_REVIEW_LABEL: Record<string, string> = {
    PENDING_SALES_LEAD: "待销售领导审批",
    PENDING_OPERATIONS: "待运营审批",
}

type CardSalesApprovalPanelProps = {
    order: SalesOrderListItem
    approval: CardSalesApproval
    onResult?: (result: ApprovalResult) => void
}

/** 卡券审批对象中心处理器；动作资格完全取自服务端活动步骤投影。 */
export function CardSalesApprovalPanel({
    order,
    approval,
    onResult,
}: CardSalesApprovalPanelProps) {
    const actions = useCardSalesApprovalActions({ order, approval, onResult })

    return (
        <DocumentSection
            title="卡券销售审批"
            description="审批进度与业务结果将在一次提交中一并保存。"
            action={
                <div className="flex flex-wrap items-center gap-2">
                    <Badge variant="info">
                        {approval.workItemType
                            ? CARD_APPROVAL_TYPE_LABEL[approval.workItemType]
                            : actions.isManager
                              ? "卡券销售领导审批"
                              : "卡券销售运营审批"}
                    </Badge>
                    {approval.workItemStatus ? (
                        <Badge variant="secondary">
                            {WORK_ITEM_STATUS_LABEL[approval.workItemStatus]}
                        </Badge>
                    ) : (
                        <Badge variant="warning">尚未创建任务</Badge>
                    )}
                </div>
            }
        >
            <div className="space-y-4">
                <Alert
                    variant={
                        approval.processingState === "APPROVAL_BLOCKED"
                            ? "destructive"
                            : "info"
                    }
                >
                    <AlertTitle>
                        {approval.processingState === "APPROVAL_BLOCKED"
                            ? "审批暂时受阻"
                            : "待审批内容（只读）"}
                    </AlertTitle>
                    <AlertDescription>
                        {approval.processingBlocker?.message ??
                            approval.frozenSubmissionSummary}
                        <span className="mt-1 block text-xs text-muted-foreground">
                            当前环节：
                            {EXPECTED_REVIEW_LABEL[
                                approval.expectedReviewStatus
                            ] ?? approval.expectedReviewStatus}
                        </span>
                    </AlertDescription>
                </Alert>

                <p className="text-xs text-muted-foreground">
                    {approval.ownerUser
                        ? `当前处理人：${approval.ownerUser.displayName}`
                        : approval.processingState === "APPROVAL_BLOCKED"
                          ? "当前步骤已受阻；尚未解析出处理人时不会创建或猜测任务。"
                          : approval.assignmentMode === "POOL"
                            ? "当前由责任团队处理；开始处理后才可作出决定。"
                            : "当前为直接分配任务，无需开始处理。"}
                </p>

                {approval.actionBlockers.length > 0 ? (
                    <ul className="space-y-1 text-xs text-destructive">
                        {approval.actionBlockers.map((blocker) => (
                            <li key={`${blocker.action}-${blocker.reason}`}>
                                {blocker.reason}
                            </li>
                        ))}
                    </ul>
                ) : null}

                <div className="flex flex-wrap gap-2">
                    {actions.canStart ? (
                        <Button
                            type="button"
                            size="sm"
                            disabled={actions.isStartPending}
                            onClick={() => void actions.startProcessing()}
                        >
                            开始处理
                        </Button>
                    ) : null}

                    {actions.canApprove ? (
                        <Button
                            type="button"
                            size="sm"
                            disabled={actions.isDecisionPending}
                            onClick={() => actions.setConfirmApprove(true)}
                        >
                            {actions.isManager ? "领导通过" : "运营通过并生效"}
                        </Button>
                    ) : null}

                    {actions.canCancel ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={actions.isCancelling}
                            onClick={() => actions.setConfirmCancel(true)}
                        >
                            撤回审批
                        </Button>
                    ) : null}
                </div>

                {actions.canReject ? (
                    <CardApprovalRejectForm form={actions.rejectForm} />
                ) : null}

                {actions.canTerminate ? (
                    <CardApprovalTerminateForm form={actions.terminateForm} />
                ) : null}

                <CardApprovalApproveConfirmDialog
                    open={actions.confirmApprove}
                    onOpenChange={actions.setConfirmApprove}
                    isManager={actions.isManager}
                    onConfirm={actions.confirmApproveDecision}
                />

                <CardApprovalRejectConfirmDialog
                    open={actions.confirmReject}
                    onOpenChange={actions.setConfirmReject}
                    onConfirm={actions.confirmRejectDecision}
                />

                <CardApprovalTerminateConfirmDialog
                    open={actions.confirmTerminate}
                    onOpenChange={actions.setConfirmTerminate}
                    onConfirm={actions.confirmTerminateDecision}
                />

                <CardApprovalCancelConfirmDialog
                    open={actions.confirmCancel}
                    onOpenChange={actions.setConfirmCancel}
                    onConfirm={actions.confirmCancelDecision}
                />
            </div>
        </DocumentSection>
    )
}
