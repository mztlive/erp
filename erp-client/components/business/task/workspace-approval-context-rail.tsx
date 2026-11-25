"use client"

import { useState, type ReactNode } from "react"

import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

import {
    UnifiedMaterialViewer,
    type MaterialFileItem,
} from "../unified-material-viewer"

export type ApprovalContextRailTab =
    | "trail"
    | "materials"
    | "source-sales"
    | "timeline"

export type WorkspaceApprovalContextRailProps = Readonly<{
    /** 用于自动派生稳定 DOM id 前缀（当未显式传入 `idPrefix` 时）。 */
    instanceId?: string
    /** 「审批记录」页签正文内容（由业务层注入审计轨迹等视图）。 */
    auditTrailContent?: ReactNode
    trailLabel?: string
    /** 右栏「附件对照」可预览的附件列表。 */
    materialFiles?: readonly MaterialFileItem[]
    materialsLoading?: boolean
    materialsLoadingMessage?: string
    materialsEmptyMessage?: string
    materialsLabel?: string
    /** 可选：覆写「附件对照」页签正文内容。 */
    materialsContent?: ReactNode
    /** 「采销对照」/来源单据对照页签正文（传入后自动挂载该页签）。 */
    sourceComparisonContent?: ReactNode
    sourceComparisonLabel?: string
    /** 来源对照页签标题旁的异常计数角标。 */
    sourceComparisonAttentionCount?: number
    sourceComparisonHasSevereAttention?: boolean
    /** 「单据全动态」页签正文（传入后自动挂载该页签）。 */
    timelineContent?: ReactNode
    timelineLabel?: string
    activeTab?: ApprovalContextRailTab
    onActiveTabChange?: (tab: ApprovalContextRailTab) => void
    activeFileId?: string
    onActiveFileChange?: (fileId: string) => void
    ariaLabel?: string
    idPrefix?: string
    className?: string
}>

/**
 * 2 栏决策审批工作台右侧上下文轨（飞书细线单层页签）：
 * - 单层底线页签：「审批记录」|「附件对照 (N)」|「采销对照」|「单据全动态」；
 * - 「审批记录」与「单据全动态」通过插槽内容注入，不耦合具体业务域查询；
 * - 「附件对照」侧边对接 `UnifiedMaterialViewer (mode="docked")`，切换时自动扩宽并支持拖拽拉宽对照。
 */
export function WorkspaceApprovalContextRail({
    instanceId,
    auditTrailContent,
    trailLabel = "审批记录",
    materialFiles = [],
    materialsLoading = false,
    materialsLoadingMessage = "正在读取本次提交的附件…",
    materialsEmptyMessage = "本次提交未附带合同、凭证或其他附件。",
    materialsLabel = "附件对照",
    materialsContent,
    sourceComparisonContent,
    sourceComparisonLabel = "采销对照",
    sourceComparisonAttentionCount = 0,
    sourceComparisonHasSevereAttention = false,
    timelineContent,
    timelineLabel = "单据全动态",
    activeTab,
    onActiveTabChange,
    activeFileId,
    onActiveFileChange,
    ariaLabel = "审批右侧对照面板",
    idPrefix,
    className,
}: WorkspaceApprovalContextRailProps) {
    const resolvedPrefix =
        idPrefix ??
        (instanceId
            ? `workspace-approval-rail-${toAutomationIdSegment(instanceId)}`
            : "workspace-approval-rail")
    const [internalTab, setInternalTab] =
        useState<ApprovalContextRailTab>("trail")
    const hasSourceSalesTab = Boolean(sourceComparisonContent)
    const hasTimelineTab = Boolean(timelineContent)
    const rawTab = activeTab ?? internalTab
    const currentTab: ApprovalContextRailTab =
        rawTab === "timeline" && !hasTimelineTab
            ? "trail"
            : rawTab === "source-sales" && !hasSourceSalesTab
              ? "trail"
              : rawTab

    const setTab = (next: ApprovalContextRailTab) => {
        if (activeTab === undefined) setInternalTab(next)
        onActiveTabChange?.(next)
    }

    return (
        <div
            data-slot="workspace-approval-context-rail"
            className={cn(
                "flex h-full min-h-0 min-w-0 flex-col bg-card",
                className,
            )}
        >
            <Tabs
                value={currentTab}
                onValueChange={(next) => {
                    if (
                        next === "trail" ||
                        next === "materials" ||
                        next === "source-sales" ||
                        next === "timeline"
                    ) {
                        setTab(next)
                    }
                }}
                className="flex min-h-0 min-w-0 flex-1 flex-col gap-0"
            >
                <div className="-mx-4 shrink-0 border-b border-border/60 px-4 @[1120px]/workspace:-mx-5 @[1120px]/workspace:px-5">
                    <TabsList
                        variant="line"
                        aria-label={ariaLabel}
                        className="w-full gap-3.5 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
                    >
                        <TabsTrigger
                            id={`${resolvedPrefix}-tab-trail`}
                            value="trail"
                            className="shrink-0 text-xs"
                        >
                            {trailLabel}
                        </TabsTrigger>
                        <TabsTrigger
                            id={`${resolvedPrefix}-tab-materials`}
                            value="materials"
                            className="shrink-0 gap-1 text-xs"
                        >
                            <span>{materialsLabel}</span>
                            {materialFiles.length > 0 ? (
                                <span className="num text-tiny text-muted-foreground">
                                    ({materialFiles.length})
                                </span>
                            ) : null}
                        </TabsTrigger>
                        {hasSourceSalesTab ? (
                            <TabsTrigger
                                id={`${resolvedPrefix}-tab-source-sales`}
                                value="source-sales"
                                className="shrink-0 gap-1 text-xs"
                            >
                                <span>{sourceComparisonLabel}</span>
                                {sourceComparisonAttentionCount > 0 ? (
                                    <span
                                        className={cn(
                                            "num text-tiny font-medium",
                                            sourceComparisonHasSevereAttention
                                                ? "text-destructive-soft-foreground"
                                                : "text-warning-soft-foreground",
                                        )}
                                    >
                                        ({sourceComparisonAttentionCount})
                                    </span>
                                ) : null}
                            </TabsTrigger>
                        ) : null}
                        {hasTimelineTab ? (
                            <TabsTrigger
                                id={`${resolvedPrefix}-tab-timeline`}
                                value="timeline"
                                className="shrink-0 text-xs"
                            >
                                {timelineLabel}
                            </TabsTrigger>
                        ) : null}
                    </TabsList>
                </div>

                <TabsContent
                    value="trail"
                    className="mt-0 min-h-0 flex-1 overflow-y-auto overscroll-contain py-3.5"
                >
                    {auditTrailContent}
                </TabsContent>

                <TabsContent
                    value="materials"
                    className="mt-0 flex min-h-0 flex-1 flex-col overflow-hidden py-3"
                >
                    {materialsContent !== undefined ? (
                        materialsContent
                    ) : materialsLoading && materialFiles.length === 0 ? (
                        <p
                            role="status"
                            className="py-6 text-center text-xs text-muted-foreground"
                        >
                            {materialsLoadingMessage}
                        </p>
                    ) : (
                        <UnifiedMaterialViewer
                            mode="docked"
                            files={materialFiles}
                            activeFileId={activeFileId}
                            onActiveFileChange={onActiveFileChange}
                            idPrefix={`${resolvedPrefix}-viewer`}
                            emptyMessage={materialsEmptyMessage}
                            className="min-h-96 flex-1 @[960px]/workspace:min-h-0"
                        />
                    )}
                </TabsContent>

                {hasSourceSalesTab ? (
                    <TabsContent
                        value="source-sales"
                        className="mt-0 min-h-0 min-w-0 flex-1 scrollbar-gutter-stable overflow-auto overscroll-contain py-3.5 pe-3"
                    >
                        {sourceComparisonContent}
                    </TabsContent>
                ) : null}

                {hasTimelineTab ? (
                    <TabsContent
                        value="timeline"
                        className="mt-0 min-h-0 flex-1 overflow-y-auto overscroll-contain py-3.5"
                    >
                        {timelineContent}
                    </TabsContent>
                ) : null}
            </Tabs>
        </div>
    )
}
