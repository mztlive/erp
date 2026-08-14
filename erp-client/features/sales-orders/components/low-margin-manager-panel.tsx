"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { useLowMarginManagerActions } from "@/features/sales-orders/hooks/use-low-margin-actions"
import type {
    ActiveLowMarginManagerConfirmation,
    SalesOrderListItem,
} from "@/features/sales-orders/types"

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
    const actions = useLowMarginManagerActions({
        order,
        confirmation,
        onResult,
    })

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
                                onClick={() => void actions.startProcessing()}
                            >
                                开始处理
                            </Button>
                        ) : null}
                        {confirmation.allowedActions.includes("APPROVE") ? (
                            <Button
                                size="sm"
                                onClick={() => actions.setApproveOpen(true)}
                            >
                                同意承接
                            </Button>
                        ) : null}
                        {confirmation.allowedActions.includes("REJECT") ? (
                            <Button
                                size="sm"
                                variant="outline"
                                onClick={() => actions.setRejectOpen(true)}
                            >
                                不同意承接
                            </Button>
                        ) : null}
                    </div>
                </AlertDescription>
            </Alert>

            <FormalActionConfirmDialog
                open={actions.approveOpen}
                onOpenChange={actions.setApproveOpen}
                actionLabel="同意低毛利承接"
                fromStatus={{ label: "待销售上级确认", tone: "warning" }}
                toStatus={{ label: "待采购重新确认", tone: "info" }}
                effects={["形成正式上级决定", "创建新的采购确认与待办"]}
                pending={actions.isPending}
                onConfirm={actions.confirmApprove}
            />

            <FormalActionConfirmDialog
                open={actions.rejectOpen}
                onOpenChange={actions.setRejectOpen}
                actionLabel="驳回低毛利承接"
                fromStatus={{ label: "待销售上级确认", tone: "warning" }}
                toStatus={{ label: "退回销售处理", tone: "warning" }}
                description={
                    <div className="space-y-2 text-left">
                        <Input
                            value={actions.reasonCode}
                            onChange={(event) =>
                                actions.setReasonCode(event.target.value)
                            }
                            placeholder="原因代码"
                        />
                        <Textarea
                            value={actions.comment}
                            onChange={(event) =>
                                actions.setComment(event.target.value)
                            }
                            placeholder="驳回说明"
                        />
                    </div>
                }
                effects={["形成正式上级驳回决定", "销售回到固定三路处置"]}
                pending={actions.isPending}
                onConfirm={actions.confirmReject}
            />
        </>
    )
}
