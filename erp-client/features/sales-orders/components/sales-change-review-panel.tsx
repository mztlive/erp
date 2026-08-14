"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import {
    FormalActionConfirmDialog,
    SequentialProcessBar,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { useSalesChangeReviewDecisionMutation } from "@/features/sales-orders/hooks/queries"
import type { SalesChangeOrderSummary } from "@/features/sales-orders/types"
import {
    mapWorkItemDto,
    useWorkItemDetailQuery,
    useWorkItemResponsibilityMutation,
} from "@/features/work-items"
import {
    classifyFormalCommandError,
    FormalCommandKeyLedger,
} from "@/lib/formal-command"

type Result = {
    status: "succeeded" | "blocked" | "rejected" | "unknown"
    title: string
    description: string
    reference: string
    nextResponsible?: string
}

/** W05 销售变更影响/财务复核的唯一强类型任务处理面。 */
export function SalesChangeReviewPanel({
    salesOrderId,
    changeOrder,
    workItemId,
    returnTo,
    onResult,
}: {
    salesOrderId: string
    changeOrder: SalesChangeOrderSummary | null
    workItemId: string
    returnTo: string
    onResult: (result: Result) => void
}) {
    const router = useRouter()
    const workItemQuery = useWorkItemDetailQuery(workItemId)
    const responsibility = useWorkItemResponsibilityMutation()
    const decision = useSalesChangeReviewDecisionMutation()
    const ledger = React.useRef(new FormalCommandKeyLedger()).current
    const [approveOpen, setApproveOpen] = React.useState(false)
    const [rejectOpen, setRejectOpen] = React.useState(false)
    const [reason, setReason] = React.useState("")
    const workItem = workItemQuery.data
        ? mapWorkItemDto(workItemQuery.data)
        : null
    const handlerKey = workItem?.handlerKey
    const handlerMatches =
        handlerKey === "sales_change_impact_review" ||
        handlerKey === "sales_change_finance_review"
    const valid = Boolean(
        workItem &&
        changeOrder &&
        handlerMatches &&
        workItem.status === "OPEN" &&
        workItem.businessObjectType === "sales_change_review" &&
        workItem.rootBusinessObjectId === salesOrderId,
    )
    const canProcess = Boolean(
        valid && workItem?.allowedActions.includes("PROCESS"),
    )
    const canStart = Boolean(
        valid && workItem?.allowedActions.includes("START_PROCESSING"),
    )
    const canRelease = Boolean(
        valid && workItem?.allowedActions.includes("RELEASE_TO_TEAM"),
    )
    const responsibilityStatus = !workItem
        ? "blocked"
        : workItem.status === "COMPLETED"
          ? "completed"
          : workItem.status === "CLOSED"
            ? "closed"
            : workItem.processingState === "APPROVAL_BLOCKED"
              ? "blocked"
              : canStart
                ? "pool_available"
                : canProcess
                  ? "assigned_to_me"
                  : "assigned_to_other"

    const submitDecision = async (nextDecision: "APPROVE" | "REJECT") => {
        if (!workItem || !changeOrder || !handlerMatches) return
        const normalizedReason = reason.trim()
        if (nextDecision === "REJECT" && !normalizedReason) {
            throw new Error("驳回原因不能为空")
        }
        const slot = `${workItem.workItemId}:${nextDecision}`
        const payload = {
            salesChangeOrderId: changeOrder.id,
            handlerKey,
            decision: nextDecision,
            workItemId: workItem.workItemId,
            expectedTaskVersion: workItem.taskVersion,
            expectedSubjectVersion: workItem.subjectVersion,
            decisionReason: normalizedReason || undefined,
        } as const
        const command = ledger.acquire(
            slot,
            `w05-change-review:${workItem.workItemId}:${nextDecision}`,
            payload,
        )
        try {
            const changed = await decision.mutateAsync({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
            ledger.settle(slot, "succeeded")
            onResult({
                status: nextDecision === "APPROVE" ? "succeeded" : "rejected",
                title:
                    nextDecision === "APPROVE"
                        ? "销售变更复核已通过"
                        : "销售变更复核已驳回",
                description:
                    nextDecision === "APPROVE"
                        ? handlerKey === "sales_change_impact_review"
                            ? "已形成财务复核任务。"
                            : "变更已形成新的正式销售版本。"
                        : "变更单已退回修改。",
                reference: changed.id,
                nextResponsible:
                    nextDecision === "APPROVE" &&
                    handlerKey === "sales_change_impact_review"
                        ? "财务"
                        : undefined,
            })
        } catch (error) {
            ledger.settle(slot, classifyFormalCommandError(error))
            throw error
        }
    }

    if (workItemQuery.isLoading) {
        return (
            <p className="text-sm text-muted-foreground">
                正在加载销售变更复核任务…
            </p>
        )
    }
    if (!valid || !workItem || !changeOrder || !handlerMatches) {
        return (
            <Alert variant="warning">
                <AlertTitle>销售变更复核任务不可执行</AlertTitle>
                <AlertDescription>
                    任务、销售单或当前改单关系已变化，请返回任务队列刷新后重试。
                </AlertDescription>
            </Alert>
        )
    }

    return (
        <div className="space-y-4">
            <Alert>
                <AlertTitle>
                    {handlerKey === "sales_change_impact_review"
                        ? "销售变更履约影响复核"
                        : "销售变更财务复核"}
                </AlertTitle>
                <AlertDescription>
                    当前改单 {changeOrder.id}；任务版本 {workItem.taskVersion}
                    ；提交版本 {workItem.subjectVersion}。
                </AlertDescription>
            </Alert>
            <SequentialProcessBar
                current={1}
                total={1}
                responsibilityStatus={responsibilityStatus}
                processLabel="通过复核"
                showProcessNext={false}
                pending={responsibility.isPending || decision.isPending}
                processDisabled={!canProcess}
                onBack={() => router.push(returnTo)}
                onProcess={() => setApproveOpen(true)}
                onProcessNext={() => setApproveOpen(true)}
                onStartProcessing={
                    canStart
                        ? async () => {
                              await responsibility.mutateAsync({
                                  kind: "START_PROCESSING",
                                  workItemId: workItem.workItemId,
                                  expectedTaskVersion: workItem.taskVersion,
                                  idempotencyKey: `w05:${workItem.workItemId}:${workItem.taskVersion}:START`,
                              })
                              await workItemQuery.refetch()
                          }
                        : undefined
                }
            />
            <div className="flex flex-wrap gap-2">
                <Button
                    type="button"
                    variant="outline"
                    disabled={!canProcess || decision.isPending}
                    onClick={() => setRejectOpen(true)}
                >
                    驳回复核
                </Button>
                {canRelease ? (
                    <Button
                        type="button"
                        variant="ghost"
                        disabled={responsibility.isPending}
                        onClick={async () => {
                            await responsibility.mutateAsync({
                                kind: "RELEASE_TO_TEAM",
                                workItemId: workItem.workItemId,
                                expectedTaskVersion: workItem.taskVersion,
                                reason: "当前处理人退回责任池",
                                idempotencyKey: `w05:${workItem.workItemId}:${workItem.taskVersion}:RELEASE`,
                            })
                            await workItemQuery.refetch()
                        }}
                    >
                        退回团队
                    </Button>
                ) : null}
            </div>
            <FormalActionConfirmDialog
                open={approveOpen}
                onOpenChange={setApproveOpen}
                actionLabel="通过销售变更复核"
                fromStatus={{ label: "待复核", tone: "warning" }}
                toStatus={{
                    label:
                        handlerKey === "sales_change_impact_review"
                            ? "待财务复核"
                            : "变更已生效",
                    tone: "info",
                }}
                description={
                    <Textarea
                        value={reason}
                        onChange={(event) => setReason(event.target.value)}
                        placeholder="复核意见（可选）"
                    />
                }
                effects={["写入正式复核结论", "完成当前任务"]}
                pending={decision.isPending}
                onConfirm={() => submitDecision("APPROVE")}
            />
            <FormalActionConfirmDialog
                open={rejectOpen}
                onOpenChange={setRejectOpen}
                actionLabel="驳回销售变更复核"
                fromStatus={{ label: "待复核", tone: "warning" }}
                toStatus={{ label: "退回改单", tone: "warning" }}
                description={
                    <Textarea
                        value={reason}
                        onChange={(event) => setReason(event.target.value)}
                        placeholder="驳回原因（必填）"
                    />
                }
                effects={["写入正式驳回结论", "完成当前任务"]}
                pending={decision.isPending}
                onConfirm={() => submitDecision("REJECT")}
            />
        </div>
    )
}
