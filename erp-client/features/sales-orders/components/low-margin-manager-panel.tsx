"use client"

import * as React from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"

import { FormalActionConfirmDialog } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { completeLowMarginManagerConfirmation } from "@/features/sales-orders/api/sales-orders"
import { salesOrderKeys } from "@/features/sales-orders/hooks/queries"
import type {
    ActiveLowMarginManagerConfirmation,
    SalesOrderListItem,
} from "@/features/sales-orders/types"
import { useWorkItemResponsibilityMutation } from "@/features/work-items"

type Result = {
    status: "succeeded" | "blocked" | "rejected" | "unknown"
    title: string
    description: string
    reference: string
    nextResponsible?: string
}

/** 低毛利上级确认的最小正式工作面，动作完全由详情 allowedActions 驱动。 */
export function LowMarginManagerPanel({
    order,
    confirmation,
    onResult,
}: {
    order: SalesOrderListItem
    confirmation: ActiveLowMarginManagerConfirmation
    onResult: (result: Result) => void
}) {
    const queryClient = useQueryClient()
    const responsibility = useWorkItemResponsibilityMutation()
    const decision = useMutation({
        mutationFn: completeLowMarginManagerConfirmation,
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.detail(order.id),
            })
        },
    })
    const [approveOpen, setApproveOpen] = React.useState(false)
    const [rejectOpen, setRejectOpen] = React.useState(false)
    const [reasonCode, setReasonCode] = React.useState("")
    const [comment, setComment] = React.useState("")
    const common = {
        salesOrderId: order.id,
        workItemId: confirmation.workItemId,
        taskVersion: confirmation.taskVersion,
        subjectVersion: confirmation.subjectVersion,
        lowMarginSubmissionId: confirmation.lowMarginSubmissionId,
        rejectedProcurementConfirmationId:
            confirmation.rejectedProcurementConfirmationId,
        expectedSalesOrderLockVersion: order.lockVersion,
    }

    return (
        <>
            <Alert variant="warning">
                <AlertTitle>低毛利承接确认</AlertTitle>
                <AlertDescription className="space-y-2">
                    <p>{confirmation.acceptanceReason}</p>
                    <p>
                        当前处理人：
                        {confirmation.ownerUser?.displayName ??
                            "销售领导责任池"}
                    </p>
                    <div className="flex flex-wrap gap-2">
                        {confirmation.allowedActions.includes(
                            "START_PROCESSING",
                        ) ? (
                            <Button
                                size="sm"
                                onClick={async () => {
                                    await responsibility.mutateAsync({
                                        kind: "START_PROCESSING",
                                        workItemId: confirmation.workItemId,
                                        expectedTaskVersion:
                                            confirmation.taskVersion,
                                        idempotencyKey: `w05:${confirmation.workItemId}:${confirmation.taskVersion}:START`,
                                    })
                                    await queryClient.invalidateQueries({
                                        queryKey: salesOrderKeys.detail(
                                            order.id,
                                        ),
                                    })
                                }}
                            >
                                开始处理
                            </Button>
                        ) : null}
                        {confirmation.allowedActions.includes("APPROVE") ? (
                            <Button
                                size="sm"
                                onClick={() => setApproveOpen(true)}
                            >
                                同意承接
                            </Button>
                        ) : null}
                        {confirmation.allowedActions.includes("REJECT") ? (
                            <Button
                                size="sm"
                                variant="outline"
                                onClick={() => setRejectOpen(true)}
                            >
                                不同意承接
                            </Button>
                        ) : null}
                    </div>
                </AlertDescription>
            </Alert>

            <FormalActionConfirmDialog
                open={approveOpen}
                onOpenChange={setApproveOpen}
                actionLabel="同意低毛利承接"
                fromStatus={{ label: "待销售上级确认", tone: "warning" }}
                toStatus={{ label: "待采购重新确认", tone: "info" }}
                effects={["形成正式上级决定", "创建新的采购确认与待办"]}
                pending={decision.isPending}
                onConfirm={async () => {
                    const result = await decision.mutateAsync({
                        ...common,
                        decision: "APPROVE",
                        idempotencyKey: `w05:${confirmation.workItemId}:${confirmation.taskVersion}:APPROVE`,
                    })
                    onResult({
                        status: "succeeded",
                        title: "已同意低毛利承接",
                        description: "已创建新的采购确认待办。",
                        reference:
                            result.outcome ===
                            "LOW_MARGIN_APPROVED_AND_PROCUREMENT_RESUBMITTED"
                                ? result.newProcurementConfirmationId
                                : result.workflowActionId,
                        nextResponsible: "采购",
                    })
                }}
            />

            <FormalActionConfirmDialog
                open={rejectOpen}
                onOpenChange={setRejectOpen}
                actionLabel="驳回低毛利承接"
                fromStatus={{ label: "待销售上级确认", tone: "warning" }}
                toStatus={{ label: "退回销售处理", tone: "warning" }}
                description={
                    <div className="space-y-2 text-left">
                        <Input
                            value={reasonCode}
                            onChange={(event) =>
                                setReasonCode(event.target.value)
                            }
                            placeholder="原因代码"
                        />
                        <Textarea
                            value={comment}
                            onChange={(event) => setComment(event.target.value)}
                            placeholder="驳回说明"
                        />
                    </div>
                }
                effects={["形成正式上级驳回决定", "销售回到固定三路处置"]}
                pending={decision.isPending}
                onConfirm={async () => {
                    if (!reasonCode.trim() || !comment.trim())
                        throw new Error("原因代码和驳回说明不能为空")
                    const result = await decision.mutateAsync({
                        ...common,
                        decision: "REJECT",
                        reasonCode,
                        comment,
                        idempotencyKey: `w05:${confirmation.workItemId}:${confirmation.taskVersion}:REJECT`,
                    })
                    onResult({
                        status: "rejected",
                        title: "已驳回低毛利承接",
                        description: "销售已回到采购驳回固定处置。",
                        reference: result.workflowActionId,
                        nextResponsible: "销售",
                    })
                }}
            />
        </>
    )
}
