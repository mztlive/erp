"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { z } from "zod"

import {
    DocumentSection,
    FormalActionConfirmDialog,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type { CardSalesApprovalDecision } from "@/features/sales-orders/api/card-approval"
import {
    salesOrderKeys,
    useCancelCardSalesApprovalMutation,
    useSubmitCardSalesApprovalDecisionMutation,
} from "@/features/sales-orders/hooks/queries"
import { CARD_APPROVAL_TYPE_LABEL } from "@/features/sales-orders/lib/labels"
import type {
    CardSalesApproval,
    SalesOrderListItem,
} from "@/features/sales-orders/types"
import { useWorkItemResponsibilityMutation } from "@/features/work-items"
import type { WorkItemStatus } from "@/features/work-items"
import { getErrorPresentation } from "@/lib/api/errors"

const WORK_ITEM_STATUS_LABEL: Record<WorkItemStatus, string> = {
    OPEN: "待处理",
    COMPLETED: "已完成",
    CLOSED: "已关闭",
}

const EXPECTED_REVIEW_LABEL: Record<string, string> = {
    PENDING_SALES_LEAD: "待销售领导审批",
    PENDING_OPERATIONS: "待运营审批",
}

const rejectSchema = z.object({
    reasonCode: z.string().trim().min(2, "请填写驳回原因（简短分类即可）"),
    comment: z.string().trim().min(4, "请填写驳回说明"),
})

type ApprovalResult = {
    status: "succeeded" | "rejected" | "blocked" | "unknown"
    title: string
    description: string
    reference: string
    nextResponsible?: string
}

type CardSalesApprovalPanelProps = {
    order: SalesOrderListItem
    approval: CardSalesApproval
    onResult?: (result: ApprovalResult) => void
}

function actionKey(
    approval: Extract<CardSalesApproval, { processingState: "READY" }>,
    action: "START_PROCESSING" | "APPROVE" | "REJECT" | "TERMINATE",
): string {
    return `w05:${approval.workItemId}:${approval.taskVersion}:${action}`
}

/** 由服务端投影版本构造可安全重试的撤回操作键。 */
const cancelActionKey = (approval: CardSalesApproval): string =>
    `w05:${approval.approvalInstanceId}:${approval.instanceVersion}:${approval.approvalStepInstanceId}:${approval.stepVersion}:CANCEL`

function approvalDecision(
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

function isUncertainResult(error: unknown): boolean {
    if (!error || typeof error !== "object" || !("kind" in error)) {
        return false
    }
    const kind = (error as { kind?: unknown }).kind
    return kind === "Network" || kind === "Parse"
}

/** 卡券审批对象中心处理器；动作资格完全取自服务端活动步骤投影。 */
export function CardSalesApprovalPanel({
    order,
    approval,
    onResult,
}: CardSalesApprovalPanelProps) {
    const queryClient = useQueryClient()
    const responsibilityMutation = useWorkItemResponsibilityMutation()
    const decisionMutation = useSubmitCardSalesApprovalDecisionMutation()
    const { mutateAsync: cancelApproval, isPending: isCancelling } =
        useCancelCardSalesApprovalMutation()
    const [confirmApprove, setConfirmApprove] = React.useState(false)
    const [confirmReject, setConfirmReject] = React.useState(false)
    const [confirmTerminate, setConfirmTerminate] = React.useState(false)
    const [confirmCancel, setConfirmCancel] = React.useState(false)
    const [rejectPayload, setRejectPayload] = React.useState<{
        reasonCode: string
        comment: string
    } | null>(null)
    const [terminatePayload, setTerminatePayload] = React.useState<{
        reasonCode: string
        comment: string
    } | null>(null)

    const rejectForm = useAppForm({
        defaultValues: { reasonCode: "", comment: "" },
        validators: { onChange: rejectSchema },
        onSubmit: async ({ value }) => {
            setRejectPayload({
                reasonCode: value.reasonCode.trim(),
                comment: value.comment.trim(),
            })
            setConfirmReject(true)
        },
    })
    const terminateForm = useAppForm({
        defaultValues: { reasonCode: "", comment: "" },
        validators: { onChange: rejectSchema },
        onSubmit: async ({ value }) => {
            setTerminatePayload({
                reasonCode: value.reasonCode.trim(),
                comment: value.comment.trim(),
            })
            setConfirmTerminate(true)
        },
    })

    const publishResult = React.useCallback(
        (next: ApprovalResult) => onResult?.(next),
        [onResult],
    )
    const actionableApproval =
        approval.processingState === "READY" ? approval : null
    const isManager = approval.expectedReviewStatus === "PENDING_SALES_LEAD"
    const isReady = actionableApproval?.workItemStatus === "OPEN"
    const canStart =
        isReady &&
        actionableApproval?.assignmentMode === "POOL" &&
        approval.allowedActions.includes("START_PROCESSING")
    const canApprove = isReady && approval.allowedActions.includes("APPROVE")
    const canReject = isReady && approval.allowedActions.includes("REJECT")
    const canTerminate =
        isReady && approval.allowedActions.includes("TERMINATE")
    const canCancel = approval.allowedActions.includes("CANCEL")

    const submitDecision = React.useCallback(
        async (
            reviewDecision: "APPROVE" | "REJECT" | "TERMINATE",
            decisionReason?: { reasonCode: string; comment: string },
        ) => {
            if (!actionableApproval) {
                throw new Error("审批当前受阻，不能提交普通决定")
            }
            const result = await decisionMutation.mutateAsync({
                approvalInstanceId: actionableApproval.approvalInstanceId,
                expectedInstanceVersion: actionableApproval.instanceVersion,
                approvalStepInstanceId:
                    actionableApproval.approvalStepInstanceId,
                expectedStepVersion: actionableApproval.stepVersion,
                workItemId: actionableApproval.workItemId,
                expectedTaskVersion: actionableApproval.taskVersion,
                expectedSubjectVersion: actionableApproval.subjectVersion,
                decision: approvalDecision(
                    order,
                    actionableApproval,
                    reviewDecision,
                    decisionReason,
                ),
                idempotencyKey: actionKey(actionableApproval, reviewDecision),
            })
            return result
        },
        [actionableApproval, decisionMutation, order],
    )

    return (
        <DocumentSection
            title="卡券销售审批"
            description="审批进度与业务结果将在一次提交中一并保存。"
            action={
                <div className="flex flex-wrap items-center gap-2">
                    <Badge variant="info">
                        {approval.workItemType
                            ? CARD_APPROVAL_TYPE_LABEL[approval.workItemType]
                            : isManager
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
                    {canStart ? (
                        <Button
                            type="button"
                            size="sm"
                            disabled={responsibilityMutation.isPending}
                            onClick={async () => {
                                if (!actionableApproval) return
                                const idempotencyKey = actionKey(
                                    actionableApproval,
                                    "START_PROCESSING",
                                )
                                try {
                                    await responsibilityMutation.mutateAsync({
                                        kind: "START_PROCESSING",
                                        workItemId:
                                            actionableApproval.workItemId,
                                        expectedTaskVersion:
                                            actionableApproval.taskVersion,
                                        idempotencyKey,
                                    })
                                    await queryClient.invalidateQueries({
                                        queryKey: salesOrderKeys.detail(
                                            order.id,
                                        ),
                                    })
                                    publishResult({
                                        status: "succeeded",
                                        title: "已开始处理",
                                        description:
                                            "当前审批已分配给你；页面刷新后可提交审批决定。",
                                        reference: order.documentNumber,
                                    })
                                } catch (error) {
                                    if (isUncertainResult(error)) {
                                        publishResult({
                                            status: "unknown",
                                            title: "开始处理结果待确认",
                                            description:
                                                "请求结果尚未确认。请刷新销售单核对当前责任，不要重复操作。",
                                            reference: idempotencyKey,
                                        })
                                        return
                                    }
                                    const failure = getErrorPresentation(
                                        error,
                                        "开始处理失败，请刷新任务责任后重试。",
                                    )
                                    publishResult({
                                        status: "blocked",
                                        title: failure.title,
                                        description: failure.description,
                                        reference: order.documentNumber,
                                    })
                                }
                            }}
                        >
                            开始处理
                        </Button>
                    ) : null}

                    {canApprove ? (
                        <Button
                            type="button"
                            size="sm"
                            disabled={decisionMutation.isPending}
                            onClick={() => setConfirmApprove(true)}
                        >
                            {isManager ? "领导通过" : "运营通过并生效"}
                        </Button>
                    ) : null}

                    {canCancel ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={isCancelling}
                            onClick={() => setConfirmCancel(true)}
                        >
                            撤回审批
                        </Button>
                    ) : null}
                </div>

                {canReject ? (
                    <form
                        className="max-w-md space-y-3 rounded-lg border border-border p-3"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void rejectForm.handleSubmit()
                        }}
                    >
                        <h3 className="text-sm font-semibold">驳回给销售</h3>
                        <rejectForm.AppField name="reasonCode">
                            {(field) => (
                                <field.TextField
                                    label="驳回原因分类"
                                    placeholder="例如：资料不齐"
                                />
                            )}
                        </rejectForm.AppField>
                        <rejectForm.AppField name="comment">
                            {(field) => (
                                <field.TextareaField
                                    label="驳回说明"
                                    rows={2}
                                    placeholder="写清需要修改的内容"
                                />
                            )}
                        </rejectForm.AppField>
                        <rejectForm.AppForm>
                            <rejectForm.SubmitButton
                                label="驳回"
                                pendingLabel="校验中"
                            />
                        </rejectForm.AppForm>
                    </form>
                ) : null}

                {canTerminate ? (
                    <form
                        className="max-w-md space-y-3 rounded-lg border border-destructive/40 p-3"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void terminateForm.handleSubmit()
                        }}
                    >
                        <h3 className="text-sm font-semibold">终止本次审批</h3>
                        <p className="text-xs text-muted-foreground">
                            终止会结束审批并将冻结提交置为已失效，不会形成驳回记录。
                        </p>
                        <terminateForm.AppField name="reasonCode">
                            {(field) => (
                                <field.TextField
                                    label="终止原因分类"
                                    placeholder="例如：业务取消"
                                />
                            )}
                        </terminateForm.AppField>
                        <terminateForm.AppField name="comment">
                            {(field) => (
                                <field.TextareaField
                                    label="终止说明"
                                    rows={2}
                                    placeholder="写清终止审批的原因"
                                />
                            )}
                        </terminateForm.AppField>
                        <terminateForm.AppForm>
                            <terminateForm.SubmitButton
                                label="终止审批"
                                pendingLabel="校验中"
                            />
                        </terminateForm.AppForm>
                    </form>
                ) : null}

                <FormalActionConfirmDialog
                    open={confirmApprove}
                    onOpenChange={setConfirmApprove}
                    title={
                        isManager ? "确认销售主管通过" : "确认运营通过并生效"
                    }
                    actionLabel="通过"
                    confirmLabel="确认通过"
                    fromStatus={{
                        label: isManager ? "待销售领导审批" : "待运营审批",
                        tone: "warning",
                    }}
                    toStatus={{
                        label: isManager ? "待运营审批" : "已生效",
                        tone: "success",
                    }}
                    lockedFields={["待审批内容", "销售单号"]}
                    effects={
                        isManager
                            ? ["记录领导审批通过", "激活唯一运营审批步骤"]
                            : [
                                  "记录运营审批通过",
                                  "原子形成销售版本、应收和执行投影",
                              ]
                    }
                    nextDepartment={isManager ? "运营" : "票款与商城执行"}
                    onConfirm={async () => {
                        if (!canApprove) return
                        try {
                            const result = await submitDecision("APPROVE")
                            const outcome = result.business_result
                            const blocked =
                                result.approval.instance.status === "BLOCKED"
                            publishResult({
                                status: blocked ? "blocked" : "succeeded",
                                title: blocked
                                    ? "领导决定已保存，运营步骤等待恢复"
                                    : outcome.outcome === "MANAGER_APPROVED"
                                      ? "领导已通过，请运营继续审批"
                                      : "运营已通过，销售单已生效",
                                description: blocked
                                    ? "下一步骤责任解析失败，未形成开放运营任务；请由审批管理员恢复当前步骤。"
                                    : outcome.outcome === "MANAGER_APPROVED"
                                      ? "已激活唯一运营步骤并形成新的开放任务。"
                                      : "销售版本、应收与执行投影已由同一事务形成。",
                                reference: outcome.workflow_action_id,
                                nextResponsible: blocked
                                    ? "审批管理员"
                                    : outcome.outcome === "MANAGER_APPROVED"
                                      ? "运营"
                                      : "票款与商城执行",
                            })
                        } catch (error) {
                            if (isUncertainResult(error)) {
                                publishResult({
                                    status: "unknown",
                                    title: "审批结果待确认",
                                    description:
                                        "请求结果尚未确认。请刷新审批状态，确认结果后再继续。",
                                    reference: actionKey(
                                        actionableApproval!,
                                        "APPROVE",
                                    ),
                                })
                                return
                            }
                            const failure = getErrorPresentation(
                                error,
                                "审批结果未确认；请刷新实例、步骤和任务版本后再处理。",
                            )
                            publishResult({
                                status: "blocked",
                                title: failure.title,
                                description: failure.description,
                                reference: order.documentNumber,
                            })
                        }
                    }}
                />

                <FormalActionConfirmDialog
                    open={confirmReject}
                    onOpenChange={setConfirmReject}
                    title="确认驳回卡券审批"
                    actionLabel="驳回"
                    confirmLabel="确认驳回"
                    fromStatus={{ label: "审批中", tone: "warning" }}
                    toStatus={{ label: "退回销售", tone: "destructive" }}
                    lockedFields={["待审批内容", "销售单号"]}
                    effects={[
                        "记录驳回原因与说明",
                        "结束当前审批实例",
                        "不激活下一审批步骤",
                    ]}
                    nextDepartment="销售"
                    onConfirm={async () => {
                        if (!canReject || !rejectPayload) return
                        try {
                            const result = await submitDecision(
                                "REJECT",
                                rejectPayload,
                            )
                            const outcome = result.business_result
                            publishResult({
                                status: "rejected",
                                title: "已驳回，请销售修改后重提",
                                description:
                                    "当前审批实例已结束；修改后将从领导审批重新开始。",
                                reference: outcome.workflow_action_id,
                                nextResponsible: "销售",
                            })
                        } catch (error) {
                            if (isUncertainResult(error)) {
                                publishResult({
                                    status: "unknown",
                                    title: "驳回结果待确认",
                                    description:
                                        "请求结果尚未确认。请刷新审批状态，确认结果后再继续。",
                                    reference: actionKey(
                                        actionableApproval!,
                                        "REJECT",
                                    ),
                                })
                                return
                            }
                            const failure = getErrorPresentation(
                                error,
                                "驳回结果未确认；请刷新实例、步骤和任务版本后再处理。",
                            )
                            publishResult({
                                status: "blocked",
                                title: failure.title,
                                description: failure.description,
                                reference: order.documentNumber,
                            })
                        }
                    }}
                />

                <FormalActionConfirmDialog
                    open={confirmTerminate}
                    onOpenChange={setConfirmTerminate}
                    title="确认终止卡券审批"
                    actionLabel="终止审批"
                    confirmLabel="确认终止"
                    fromStatus={{ label: "审批中", tone: "warning" }}
                    toStatus={{ label: "审批已终止", tone: "destructive" }}
                    lockedFields={["待审批内容", "销售单号"]}
                    effects={[
                        "记录终止原因与说明",
                        "结束当前审批实例且不形成驳回记录",
                        "冻结提交置为已失效，销售单恢复为草稿",
                    ]}
                    nextDepartment="销售"
                    onConfirm={async () => {
                        if (!canTerminate || !terminatePayload) return
                        try {
                            const result = await submitDecision(
                                "TERMINATE",
                                terminatePayload,
                            )
                            const outcome = result.business_result
                            publishResult({
                                status: "succeeded",
                                title: "审批已终止",
                                description:
                                    "本次审批已结束，冻结提交已失效；销售可重新编辑。",
                                reference: outcome.workflow_action_id,
                                nextResponsible: "销售",
                            })
                        } catch (error) {
                            if (isUncertainResult(error)) {
                                publishResult({
                                    status: "unknown",
                                    title: "终止结果待确认",
                                    description:
                                        "请求结果尚未确认。请刷新审批状态，确认结果后再继续。",
                                    reference: actionKey(
                                        actionableApproval!,
                                        "TERMINATE",
                                    ),
                                })
                                return
                            }
                            const failure = getErrorPresentation(
                                error,
                                "终止结果未确认；请刷新实例、步骤和任务版本后再处理。",
                            )
                            publishResult({
                                status: "blocked",
                                title: failure.title,
                                description: failure.description,
                                reference: order.documentNumber,
                            })
                        }
                    }}
                />

                <FormalActionConfirmDialog
                    open={confirmCancel}
                    onOpenChange={setConfirmCancel}
                    title="确认撤回卡券审批"
                    actionLabel="撤回审批"
                    confirmLabel="确认撤回"
                    fromStatus={{ label: "审批中", tone: "warning" }}
                    toStatus={{ label: "可继续修改", tone: "neutral" }}
                    lockedFields={["待审批内容", "销售单号"]}
                    effects={[
                        "取消当前审批和未执行环节",
                        "关闭当前待处理事项",
                        "销售单恢复为可修改草稿",
                    ]}
                    nextDepartment="销售"
                    onConfirm={async () => {
                        if (!canCancel) return
                        const idempotencyKey = cancelActionKey(approval)
                        try {
                            const result = await cancelApproval({
                                approvalInstanceId: approval.approvalInstanceId,
                                currentStepInstanceId:
                                    approval.approvalStepInstanceId,
                                workItemId: approval.workItemId,
                                expectedInstanceVersion:
                                    approval.instanceVersion,
                                expectedStepVersion: approval.stepVersion,
                                expectedTaskVersion: approval.taskVersion,
                                expectedSubjectVersion: approval.subjectVersion,
                                reason: "申请人撤回并继续修改",
                                idempotencyKey,
                            })
                            publishResult({
                                status: "succeeded",
                                title: "审批已撤回",
                                description:
                                    result.business_result
                                        .sales_order_commercial_status ===
                                    "DRAFT"
                                        ? "销售单已恢复为草稿，可以修改后重新提交。"
                                        : "审批已取消，请刷新销售单确认当前状态。",
                                reference: order.documentNumber,
                                nextResponsible: "销售",
                            })
                        } catch (error) {
                            if (isUncertainResult(error)) {
                                publishResult({
                                    status: "unknown",
                                    title: "撤回结果待确认",
                                    description:
                                        "请求结果尚未确认。请刷新审批状态，不要重复撤回。",
                                    reference: order.documentNumber,
                                })
                                return
                            }
                            const failure = getErrorPresentation(
                                error,
                                "撤回失败；请刷新审批、步骤和待处理事项版本后重试。",
                            )
                            publishResult({
                                status: "blocked",
                                title: failure.title,
                                description: failure.description,
                                reference: order.documentNumber,
                            })
                        }
                    }}
                />
            </div>
        </DocumentSection>
    )
}
