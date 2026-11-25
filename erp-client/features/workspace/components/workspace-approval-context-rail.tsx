"use client"

import { useMemo, type ComponentProps } from "react"

import {
    WorkspaceApprovalContextRail as SharedWorkspaceApprovalContextRail,
    type ApprovalContextRailTab,
} from "@/components/business/task/workspace-approval-context-rail"
import type { MaterialFileItem } from "@/components/business/unified-material-viewer"
import { UnifiedAuditTrail } from "@/features/approval-workflow/components/unified-audit-trail"
import type { ApprovalRuntimeInstance } from "@/features/approval-workflow/types"
import type { TimelineTarget } from "@/features/document-timeline/types"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"
import { toAutomationIdSegment } from "@/lib/automation-id"

import { WorkspacePurchaseSourceComparison } from "./workspace-purchase-source-comparison"
import { buildPurchaseSourceComparisonModel } from "./workspace-purchase-source-comparison-model"

export type { ApprovalContextRailTab }

export type WorkspaceApprovalContextRailProps = Readonly<{
    instanceId: string
    instance?: ApprovalRuntimeInstance
    /** 单据动态查询目标（传入后启用平级「单据全动态」页签）。 */
    timelineTarget?: TimelineTarget
    currentDocument?: { type: string; id: string }
    /** 右栏「附件对照」可预览的认证附件列表。 */
    materialFiles?: readonly MaterialFileItem[]
    materialsLoading?: boolean
    /** 本次采购提交及锁定的销售依据。 */
    sourceComparison?: Omit<
        ComponentProps<typeof WorkspacePurchaseSourceComparison>,
        "idPrefix"
    >
    /** 需要滚动定位并高亮的审批执行 ID。 */
    highlightedExecutionId?: string | null
    /** 兼容保留字段；退回重办提示现由左侧单据顶部横幅承载。 */
    isRework?: boolean
    activeTab?: ApprovalContextRailTab
    onActiveTabChange?: (tab: ApprovalContextRailTab) => void
    activeFileId?: string
    onActiveFileChange?: (fileId: string) => void
    idPrefix?: string
    className?: string
}>

/**
 * 工作台审批右轨业务适配壳：基于纯展示原语 `components/business/task/workspace-approval-context-rail`，
 * 自动接入 `UnifiedAuditTrail` 与采购来源销售单对照视图。
 */
export function WorkspaceApprovalContextRail({
    instanceId,
    timelineTarget,
    currentDocument,
    materialFiles = [],
    materialsLoading = false,
    sourceComparison,
    highlightedExecutionId,
    activeTab,
    onActiveTabChange,
    activeFileId,
    onActiveFileChange,
    idPrefix,
    className,
}: WorkspaceApprovalContextRailProps) {
    const resolvedPrefix =
        idPrefix ??
        `workspace-approval-rail-${toAutomationIdSegment(instanceId)}`
    const hasSalesLines = Boolean(
        !sourceComparison?.precomputedModel &&
        sourceComparison?.orders.some(
            (order) => order.revision.commercial_lines.length > 0,
        ),
    )
    const { names: unitNames } = useProductUnitNames([], hasSalesLines)
    const sourceComparisonModel = useMemo(
        () =>
            sourceComparison
                ? (sourceComparison.precomputedModel ??
                  buildPurchaseSourceComparisonModel({
                      orders: sourceComparison.orders,
                      snapshots: sourceComparison.snapshots,
                      purchaseLines: sourceComparison.purchaseLines,
                      purchaseFulfillment: sourceComparison.purchaseFulfillment,
                      purchaseExtraSections:
                          sourceComparison.purchaseExtraSections,
                      extraCostSummary: sourceComparison.extraCostSummary,
                      unitNames,
                  }))
                : undefined,
        [sourceComparison, unitNames],
    )

    return (
        <SharedWorkspaceApprovalContextRail
            instanceId={instanceId}
            idPrefix={resolvedPrefix}
            materialFiles={materialFiles}
            materialsLoading={materialsLoading}
            activeTab={activeTab}
            onActiveTabChange={onActiveTabChange}
            activeFileId={activeFileId}
            onActiveFileChange={onActiveFileChange}
            className={className}
            auditTrailContent={
                <UnifiedAuditTrail
                    instanceId={instanceId}
                    currentDocument={currentDocument}
                    isApprovalContext
                    hideTabs
                    activeTab="approval"
                    highlightedExecutionId={highlightedExecutionId}
                    idPrefix={`${resolvedPrefix}-audit`}
                />
            }
            sourceComparisonContent={
                sourceComparison ? (
                    <WorkspacePurchaseSourceComparison
                        key={instanceId}
                        idPrefix={resolvedPrefix}
                        precomputedModel={sourceComparisonModel}
                        {...sourceComparison}
                    />
                ) : undefined
            }
            sourceComparisonAttentionCount={
                sourceComparisonModel?.totalAttentionCount ?? 0
            }
            sourceComparisonHasSevereAttention={Boolean(
                sourceComparisonModel?.hasSevereAttention,
            )}
            timelineContent={
                timelineTarget ? (
                    <UnifiedAuditTrail
                        timelineTarget={timelineTarget}
                        currentDocument={currentDocument}
                        isApprovalContext
                        hideTabs
                        activeTab="timeline"
                        idPrefix={`${resolvedPrefix}-timeline`}
                    />
                ) : undefined
            }
        />
    )
}
