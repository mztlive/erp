"use client"

import type * as React from "react"

import {
    BusinessFailureState,
    BusinessStatusBadge,
} from "@/components/business"
import { TaskActionSheet } from "@/components/business/task-action-sheet"
import { Button } from "@/components/ui/button"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import {
    AdjustmentApprovalArea,
    adjustmentApprovalPhase,
} from "@/features/inventory/components/adjustment-approval-area"
import type { AdjustmentDetailView } from "@/features/inventory/types"
import { formatDateTime } from "@/lib/datetime"

/**
 * 库存调整详情与侧边审批抽屉。
 * 固定采用 `TaskActionSheet`（768px `size="detail"`），提供充裕宽度承接审批路线、历史与动作。
 */
export function AdjustmentDetailSheet({
    open,
    finalFocus,
    detail,
    isPending,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onClose,
    onDecisionApplied,
    onEditOriginal,
    onCancelled,
}: {
    open: boolean
    finalFocus?: React.ComponentProps<typeof TaskActionSheet>["finalFocus"]
    detail: AdjustmentDetailView | null | undefined
    isPending: boolean
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onClose: () => void
    onDecisionApplied?: (view: ApprovalCommandView) => void
    onEditOriginal?: (stockAdjustmentId: string) => void
    onCancelled?: () => void
}) {
    const adjustment = detail?.adjustment
    const canEditOriginal = Boolean(
        detail?.approval.submitCommand &&
        detail.approval.allowedActions.includes("SUBMIT") &&
        onEditOriginal,
    )
    const nodeSummary =
        adjustment?.currentNodeLabel || adjustment?.currentAssigneeLabel
            ? [
                  adjustment.currentNodeLabel
                      ? `当前节点：${adjustment.currentNodeLabel}`
                      : null,
                  adjustment.currentAssigneeLabel
                      ? `审批人：${adjustment.currentAssigneeLabel}`
                      : null,
              ]
                  .filter(Boolean)
                  .join(" · ")
            : null

    return (
        <TaskActionSheet
            idPrefix="inventory-adjustment-detail-sheet"
            open={open}
            finalFocus={finalFocus}
            onOpenChange={(nextOpen) => {
                if (!nextOpen) onClose()
            }}
            title={adjustment ? adjustment.adjustmentNo : "调整详情"}
            subtitle={
                adjustment ? (
                    <span className="num">
                        {adjustment.warehouseName} · {adjustment.skuCode}
                        {adjustment.skuName ? ` · ${adjustment.skuName}` : ""}
                    </span>
                ) : undefined
            }
            badges={
                adjustment ? (
                    <BusinessStatusBadge
                        context="preview"
                        label={adjustment.statusLabel}
                        tone={adjustment.statusTone}
                    />
                ) : undefined
            }
            footerLeading={
                nodeSummary ? (
                    <span className="text-xs text-muted-foreground">
                        {nodeSummary}
                    </span>
                ) : undefined
            }
            footerActions={
                <>
                    <Button
                        id="inventory-adjustment-detail-close"
                        type="button"
                        variant="outline"
                        onClick={onClose}
                    >
                        关闭
                    </Button>
                    {canEditOriginal && detail && onEditOriginal ? (
                        <Button
                            id="inventory-adjustment-detail-edit"
                            type="button"
                            onClick={() =>
                                onEditOriginal(detail.adjustment.adjustmentId)
                            }
                        >
                            修改原单
                        </Button>
                    ) : null}
                </>
            }
        >
            {isPending ? (
                <div className="space-y-4">
                    <div className="h-24 animate-pulse rounded-lg bg-muted" />
                    <div className="h-44 animate-pulse rounded-lg bg-muted" />
                </div>
            ) : detail ? (
                <div className="flex flex-col gap-6">
                    <section
                        aria-label="调整概览"
                        className="space-y-4 border-b border-border pb-5"
                    >
                        <div className="flex flex-wrap items-baseline justify-between gap-2">
                            <div className="text-section-title font-semibold text-foreground">
                                {detail.adjustment.reasonTypeLabel} ·{" "}
                                <span className="num">
                                    {detail.adjustment.direction === "increase"
                                        ? "增加 +"
                                        : "减少 -"}
                                    {detail.adjustment.quantity}{" "}
                                    {detail.adjustment.baseUnit}
                                </span>
                            </div>
                            <span className="num text-xs text-muted-foreground">
                                创建{" "}
                                {formatDateTime(
                                    detail.adjustment.createdAt,
                                    "full",
                                    "passthrough",
                                )}
                            </span>
                        </div>

                        <dl className="grid grid-cols-1 gap-x-6 gap-y-2.5 text-body-compact sm:grid-cols-2 lg:grid-cols-3">
                            <div className="min-w-0">
                                <dt className="text-xs text-muted-foreground">
                                    仓库
                                </dt>
                                <dd className="mt-0.5 break-words font-medium text-foreground">
                                    {detail.adjustment.warehouseName}
                                </dd>
                            </div>
                            <div className="min-w-0">
                                <dt className="text-xs text-muted-foreground">
                                    商品
                                </dt>
                                <dd className="mt-0.5 break-words text-foreground">
                                    {detail.adjustment.skuName || "—"}
                                    <span className="num ml-1.5 text-xs text-muted-foreground">
                                        {detail.adjustment.skuCode}
                                    </span>
                                </dd>
                            </div>
                            <div className="min-w-0">
                                <dt className="text-xs text-muted-foreground">
                                    经办人
                                </dt>
                                <dd className="mt-0.5 break-words text-foreground">
                                    {detail.adjustment.operatorLabel}
                                </dd>
                            </div>
                            {detail.adjustment.applicantLabel &&
                            detail.adjustment.applicantLabel !==
                                detail.adjustment.operatorLabel ? (
                                <div className="min-w-0">
                                    <dt className="text-xs text-muted-foreground">
                                        申请人
                                    </dt>
                                    <dd className="mt-0.5 break-words text-foreground">
                                        {detail.adjustment.applicantLabel}
                                    </dd>
                                </div>
                            ) : null}
                            {detail.adjustment.postedAt ? (
                                <div className="min-w-0">
                                    <dt className="text-xs text-muted-foreground">
                                        过账时间
                                    </dt>
                                    <dd className="num mt-0.5 text-foreground">
                                        {formatDateTime(
                                            detail.adjustment.postedAt,
                                            "full",
                                            "passthrough",
                                        )}
                                    </dd>
                                </div>
                            ) : null}
                        </dl>

                        {detail.adjustment.note ? (
                            <p className="rounded-lg bg-muted/40 px-3 py-2 text-body-compact text-foreground">
                                <span className="mr-1.5 text-xs text-muted-foreground">
                                    备注
                                </span>
                                {detail.adjustment.note}
                            </p>
                        ) : null}
                    </section>

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
                        onEditOriginal={onEditOriginal}
                        onCancelled={onCancelled}
                    />
                </div>
            ) : (
                <BusinessFailureState
                    kind="business"
                    title="无法加载调整详情"
                    description="调整单可能已不存在，或权限已变化。"
                />
            )}
        </TaskActionSheet>
    )
}
