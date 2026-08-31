"use client"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    QuickPreviewSheet,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { AdjustmentApprovalArea } from "@/features/inventory/components/adjustment-approval-area"
import { adjustmentApprovalPhase } from "@/features/inventory/components/adjustment-approval-area"
import type { AdjustmentDetailView } from "@/features/inventory/types"
import { formatDateTime } from "@/lib/datetime"

/**
 * 库存调整详情。按草稿 / 运行中终态嵌入通用审批区。
 */
export function AdjustmentDetailSheet({
    open,
    detail,
    isPending,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onClose,
    onDecisionApplied,
}: {
    open: boolean
    detail: AdjustmentDetailView | null | undefined
    isPending: boolean
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onClose: () => void
    onDecisionApplied?: (view: ApprovalCommandView) => void
}) {
    const adjustment = detail?.adjustment
    return (
        <QuickPreviewSheet
            id="inventory-adjustment-detail-sheet"
            open={open}
            onOpenChange={(nextOpen) => {
                if (!nextOpen) onClose()
            }}
            size="preview"
            title={adjustment ? adjustment.adjustmentNo : "调整详情"}
            identity={
                adjustment ? (
                    <span className="num text-sm">
                        {adjustment.warehouseName} · {adjustment.skuCode}
                    </span>
                ) : null
            }
            summary={
                adjustment ? (
                    <BusinessStatusBadge
                        context="preview"
                        label={adjustment.statusLabel}
                        tone={adjustment.statusTone}
                    />
                ) : null
            }
            footer={
                <Button
                    id="inventory-adjustment-detail-close"
                    type="button"
                    variant="outline"
                    onClick={onClose}
                >
                    关闭
                </Button>
            }
        >
            {isPending ? (
                <div className="space-y-3 p-1">
                    <div className="h-24 animate-pulse rounded-xl bg-muted" />
                    <div className="h-40 animate-pulse rounded-xl bg-muted" />
                </div>
            ) : detail ? (
                <div className="flex flex-col gap-4">
                    <div className="rounded-xl border bg-card p-3 text-sm">
                        <div className="font-medium">
                            {detail.adjustment.reasonTypeLabel} ·{" "}
                            {detail.adjustment.direction === "increase"
                                ? "增加"
                                : "减少"}{" "}
                            {detail.adjustment.quantity}{" "}
                            {detail.adjustment.baseUnit}
                        </div>
                        <div className="mt-1 text-xs text-muted-foreground">
                            经办 {detail.adjustment.operatorLabel} · 创建{" "}
                            {formatDateTime(
                                detail.adjustment.createdAt,
                                "full",
                                "passthrough",
                            )}
                        </div>
                    </div>
                    <AdjustmentApprovalArea
                        phase={adjustmentApprovalPhase(
                            detail.adjustment.status,
                        )}
                        approval={detail.approval}
                        documentId={detail.adjustment.adjustmentId}
                        workItemId={workItemId}
                        expectedTaskVersion={expectedTaskVersion}
                        workItemAllowedActions={workItemAllowedActions}
                        onDecisionApplied={onDecisionApplied}
                    />
                </div>
            ) : (
                <BusinessFailureState
                    kind="business"
                    title="无法加载调整详情"
                    description="调整单可能已不存在，或权限已变化。"
                />
            )}
        </QuickPreviewSheet>
    )
}
