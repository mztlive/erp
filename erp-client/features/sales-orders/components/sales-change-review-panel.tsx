"use client"

import {
    FormalActionConfirmDialog,
    SequentialProcessBar,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import {
    useSalesChangeReviewActions,
    type SalesChangeReviewResult,
} from "@/features/sales-orders/hooks/use-change-review-actions"
import type { SalesChangeOrderSummary } from "@/features/sales-orders/types"

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
    onResult: (result: SalesChangeReviewResult) => void
}) {
    const actions = useSalesChangeReviewActions({
        salesOrderId,
        changeOrder,
        workItemId,
        returnTo,
        onResult,
    })
    const {
        workItemQuery,
        responsibility,
        decision,
        workItem,
        handlerKey,
        handlerMatches,
        valid,
        canProcess,
        canStart,
        canRelease,
        responsibilityStatus,
        reason,
        setReason,
    } = actions

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
                onBack={() => actions.router.push(returnTo)}
                onProcess={() => actions.setApproveOpen(true)}
                onProcessNext={() => actions.setApproveOpen(true)}
                onStartProcessing={
                    canStart
                        ? async () => {
                              await actions.startProcessing()
                          }
                        : undefined
                }
            />
            <div className="flex flex-wrap gap-2">
                <Button
                    type="button"
                    variant="outline"
                    disabled={!canProcess || decision.isPending}
                    onClick={() => actions.setRejectOpen(true)}
                >
                    驳回复核
                </Button>
                {canRelease ? (
                    <Button
                        type="button"
                        variant="ghost"
                        disabled={responsibility.isPending}
                        onClick={() => void actions.releaseToTeam()}
                    >
                        退回团队
                    </Button>
                ) : null}
            </div>
            <FormalActionConfirmDialog
                open={actions.approveOpen}
                onOpenChange={actions.setApproveOpen}
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
                onConfirm={() => actions.submitDecision("APPROVE")}
            />
            <FormalActionConfirmDialog
                open={actions.rejectOpen}
                onOpenChange={actions.setRejectOpen}
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
                onConfirm={() => actions.submitDecision("REJECT")}
            />
        </div>
    )
}
