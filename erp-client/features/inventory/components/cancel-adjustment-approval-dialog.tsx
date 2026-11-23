"use client"

import * as React from "react"

import { Button } from "@/components/ui/button"
import { ApprovalWithdrawDialog } from "@/features/approval-workflow/components/approval-withdraw-dialog"
import { useCancelStockAdjustmentApprovalMutation } from "@/features/inventory/hooks/queries"
import type { StockAdjustmentCancelCommand } from "@/features/inventory/types"

/**
 * 库存调整普通撤回入口。命令只使用详情投影下发的完整 CAS 令牌。
 * 底层统一复用 `ApprovalWithdrawDialog`（强制 destructive + BPM 512B UTF-8 校验 + 幂等恢复）。
 */
export function CancelAdjustmentApprovalDialog({
    stockAdjustmentId,
    command,
    currentNodeName,
    id,
    reviseRejected = false,
    onCancelled,
}: {
    stockAdjustmentId: string
    command: StockAdjustmentCancelCommand
    currentNodeName?: string
    id: string
    reviseRejected?: boolean
    onCancelled?: (stockAdjustmentId: string) => void
}) {
    const [open, setOpen] = React.useState(false)
    const cancelApproval = useCancelStockAdjustmentApprovalMutation()

    return (
        <>
            <Button
                id={`${id}-trigger`}
                type="button"
                variant="outline"
                onClick={() => setOpen(true)}
            >
                {reviseRejected ? "修改原单" : "撤回审批"}
            </Button>
            <ApprovalWithdrawDialog
                open={open}
                onOpenChange={setOpen}
                idPrefix={id}
                reasonId={`${id}-reason`}
                title={reviseRejected ? "修改原单" : "撤回审批"}
                description={
                    currentNodeName
                        ? `当前节点：${currentNodeName}。`
                        : "请确认撤回影响并填写撤回原因。"
                }
                actionLabel="撤回审批"
                confirmLabel={reviseRejected ? "撤回并修改原单" : "确认撤回"}
                unknownConfirmLabel="使用本次操作重试"
                cancelLabel="取消"
                fromStatus={{
                    label: reviseRejected ? "已驳回" : "审批中",
                    tone: reviseRejected ? "destructive" : "info",
                }}
                toStatus={{ label: "草稿", tone: "neutral" }}
                effects={[
                    reviseRejected
                        ? "撤回后打开原库存调整单草稿，修改后可重新提交审批。"
                        : "撤回后库存调整单将回到草稿。",
                ]}
                reasonLabel="撤回原因"
                reasonPlaceholder="请输入撤回原因"
                reviseRejected={reviseRejected}
                pending={cancelApproval.isPending}
                idempotencyScope={`inventory:${stockAdjustmentId}:cancel`}
                onSubmitWithdraw={async ({ reason, idempotencyKey }) => {
                    await cancelApproval.mutateAsync({
                        stockAdjustmentId,
                        command,
                        reason,
                        idempotencyKey,
                    })
                    onCancelled?.(stockAdjustmentId)
                }}
            />
        </>
    )
}
