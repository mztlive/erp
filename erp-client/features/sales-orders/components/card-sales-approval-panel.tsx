"use client"

import * as React from "react"
import { z } from "zod"
import {
    DocumentSection,
    FormalActionConfirmDialog,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { CARD_APPROVAL_TYPE_LABEL } from "@/features/sales-orders/lib/labels"
import {
    useClaimCardSalesApprovalMutation,
    useCompleteCardSalesApprovalMutation,
} from "@/features/sales-orders/hooks/queries"
import type {
    CardSalesApproval,
    SalesOrderListItem,
} from "@/features/sales-orders/types"
import { leaseText } from "@/lib/ui-text"
import { getErrorPresentation } from "@/lib/api/errors"

const WORK_ITEM_STATUS_LABEL: Record<
    CardSalesApproval["workItemStatus"],
    string
> = {
    UNCLAIMED: "待领取",
    CLAIMED: "处理中",
    COMPLETED: "已完成",
}

const EXPECTED_REVIEW_LABEL: Record<string, string> = {
    PENDING_SALES_LEAD: "待销售领导审批",
    PENDING_OPERATIONS: "待运营审批",
    APPROVED: "已通过",
    REJECTED: "已驳回",
}

const rejectSchema = z.object({
    reasonCode: z.string().trim().min(2, "请填写驳回原因（简短分类即可）"),
    comment: z.string().trim().min(4, "请填写驳回说明"),
})

type ApprovalResult = {
    status: "succeeded" | "rejected" | "blocked"
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

/**
 * 卡券双审批：领导 / 运营共用处理区。
 * 处理权仅在会话内存，刷新后需重新领取。
 */
export function CardSalesApprovalPanel({
    order,
    approval,
    onResult,
}: CardSalesApprovalPanelProps) {
    const claimMutation = useClaimCardSalesApprovalMutation()
    const completeMutation = useCompleteCardSalesApprovalMutation()

    /** 处理权仅会话内存，不写入 query cache / URL */
    const claimRef = React.useRef<{
        workItemId: string
    } | null>(null)

    const publishResult = React.useCallback(
        (next: ApprovalResult) => {
            onResult?.(next)
        },
        [onResult],
    )
    const [confirmApprove, setConfirmApprove] = React.useState(false)
    const [confirmReject, setConfirmReject] = React.useState(false)
    const [rejectPayload, setRejectPayload] = React.useState<{
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

    const claimedHere =
        claimRef.current?.workItemId === approval.workItemId &&
        approval.workItemStatus === "CLAIMED"
    const canDecide =
        claimedHere &&
        approval.allowedActions.includes("APPROVE") &&
        Boolean(claimRef.current)

    const statusLabel =
        WORK_ITEM_STATUS_LABEL[approval.workItemStatus] ??
        approval.workItemStatus
    const expectedLabel =
        EXPECTED_REVIEW_LABEL[approval.expectedReviewStatus] ??
        approval.expectedReviewStatus
    const isManager = approval.workItemType === "CARD_SALES_MANAGER_APPROVAL"

    return (
        <DocumentSection
            title="卡券销售审批"
            description="请先领取再决定通过或驳回。审批未完成前，卡券单不会生效。"
            action={
                <div className="flex flex-wrap items-center gap-2">
                    <Badge variant="info">
                        {CARD_APPROVAL_TYPE_LABEL[approval.workItemType]}
                    </Badge>
                    <Badge variant="secondary">{statusLabel}</Badge>
                </div>
            }
        >
            <div className="space-y-4">
                <Alert variant="info">
                    <AlertTitle>待审批内容（只读）</AlertTitle>
                    <AlertDescription>
                        {approval.frozenSubmissionSummary}
                        <span className="mt-1 block text-xs text-muted-foreground">
                            当前环节：{expectedLabel}
                        </span>
                    </AlertDescription>
                </Alert>

                {approval.claimedByLabel ? (
                    <p className="text-xs text-muted-foreground">
                        处理人 {approval.claimedByLabel}
                    </p>
                ) : (
                    <p className="text-xs text-muted-foreground">
                        {leaseText.reclaimHint}；若已被他人领取，此处只能查看。
                    </p>
                )}

                <div className="flex flex-wrap gap-2">
                    {approval.allowedActions.includes("CLAIM") ? (
                        <Button
                            type="button"
                            size="sm"
                            disabled={claimMutation.isPending}
                            onClick={async () => {
                                await claimMutation.mutateAsync({
                                    workItemId: approval.workItemId,
                                })
                                claimRef.current = {
                                    workItemId: approval.workItemId,
                                }
                                publishResult({
                                    status: "succeeded",
                                    title: "已领取，可以审批了",
                                    description:
                                        "请在有效时间内完成通过或驳回；刷新页面后需重新领取。",
                                    reference: order.documentNumber,
                                })
                            }}
                        >
                            领取并开始审批
                        </Button>
                    ) : null}

                    {canDecide ? (
                        <Button
                            type="button"
                            size="sm"
                            disabled={completeMutation.isPending}
                            onClick={() => setConfirmApprove(true)}
                        >
                            {isManager ? "领导通过" : "运营通过并生效"}
                        </Button>
                    ) : null}
                </div>

                {canDecide ? (
                    <form
                        className="max-w-md space-y-3 rounded-lg border border-border p-3"
                        onSubmit={(e) => {
                            e.preventDefault()
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
                                    placeholder="写清要销售改什么；改完后需重新从领导审批走"
                                    description="说明将随驳回送达销售。"
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

                <FormalActionConfirmDialog
                    open={confirmApprove}
                    onOpenChange={setConfirmApprove}
                    title={isManager ? "确认领导通过" : "确认运营通过并生效"}
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
                            ? ["记录领导审批通过", "结束本环节，交给运营审批"]
                            : [
                                  "记录运营审批通过",
                                  "销售单正式生效，并生成应收",
                                  "向商城同步执行信息",
                              ]
                    }
                    nextDepartment={isManager ? "运营" : "票款与商城执行"}
                    onConfirm={async () => {
                        const claim = claimRef.current
                        if (!claim) return
                        try {
                            const outcome = await completeMutation.mutateAsync({
                                workItemId: approval.workItemId,
                                workItemType: approval.workItemType,
                                decision: "APPROVE",
                            })
                            claimRef.current = null
                            publishResult({
                                status: "succeeded",
                                title:
                                    outcome.outcome === "MANAGER_APPROVED"
                                        ? "领导已通过，请运营继续审批"
                                        : "运营已通过，销售单已生效",
                                description: outcome.detail,
                                reference: outcome.reference,
                                nextResponsible:
                                    outcome.outcome === "MANAGER_APPROVED"
                                        ? "运营"
                                        : "票款与商城执行",
                            })
                        } catch (error) {
                            const failure = getErrorPresentation(
                                error,
                                "审批未完成，请核对当前状态后再决定是否重试。",
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
                        "结束本环节审批，不进入下一环节",
                        "销售修改后需重新从领导审批开始",
                    ]}
                    nextDepartment="销售"
                    onConfirm={async () => {
                        const claim = claimRef.current
                        if (!claim || !rejectPayload) return
                        try {
                            const outcome = await completeMutation.mutateAsync({
                                workItemId: approval.workItemId,
                                workItemType: approval.workItemType,
                                decision: "REJECT",
                                reasonCode: rejectPayload.reasonCode,
                                comment: rejectPayload.comment,
                            })
                            claimRef.current = null
                            publishResult({
                                status: "rejected",
                                title: "已驳回，请销售修改后重提",
                                description: `${outcome.detail}${
                                    rejectPayload.comment
                                        ? ` 驳回说明：${rejectPayload.comment}`
                                        : ""
                                }`,
                                reference: outcome.reference,
                                nextResponsible: "销售",
                            })
                        } catch (error) {
                            const failure = getErrorPresentation(
                                error,
                                "驳回未完成，请核对当前状态后再决定是否重试。",
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
