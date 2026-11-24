"use client"

import { useState } from "react"

import {
    UnifiedMaterialViewer,
    type MaterialFileItem,
} from "@/components/business/unified-material-viewer"
import { Button } from "@/components/ui/button"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import type { ApprovalMaterials } from "@/features/approval-workflow/api/materials"
import { ApprovalSalesOrderPaper } from "@/features/approval-workflow/components/approval-sales-order-paper"
import { UnifiedAuditTrail } from "@/features/approval-workflow/components/unified-audit-trail"
import type { ApprovalRuntimeInstance } from "@/features/approval-workflow/types"
import type { TimelineTarget } from "@/features/document-timeline/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayBusinessText, displayName } from "@/lib/display-name"
import { cn } from "@/lib/utils"

export type ApprovalContextRailTab =
    | "trail"
    | "materials"
    | "source-sales"
    | "timeline"

export type WorkspaceApprovalContextRailProps = Readonly<{
    instanceId: string
    instance?: ApprovalRuntimeInstance
    /** 单据动态查询目标（传入后启用平级「单据全动态」页签）。 */
    timelineTarget?: TimelineTarget
    currentDocument?: { type: string; id: string }
    /** 右栏「附件对照」可预览的认证附件列表。 */
    materialFiles?: readonly MaterialFileItem[]
    materialsLoading?: boolean
    /** 采购审批锁定的来源销售单完整快照（传入后启用「来源销售单」侧边对照页签）。 */
    sourceSalesOrders?: ApprovalMaterials["source_sales_orders"]
    sourceSalesSnapshots?: ApprovalMaterials["display"]["source_sales"]
    activeSourceRevisionId?: string
    onActiveSourceRevisionIdChange?: (revisionId: string) => void
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
 * 2 栏决策审批工作台右侧上下文轨（飞书细线单层页签）：
 * - 单层底线页签：「审批记录」|「附件对照 (N)」|「来源销售单」|「单据全动态」；
 * - 「审批记录」展示倒序 `UnifiedAuditTrail`；
 * - 「附件对照」侧边对接 `UnifiedMaterialViewer (mode="docked")`，切换时自动扩宽并支持拖拽拉宽至 50% 左右对照；
 * - 「来源销售单」在采购审批中支持左侧采销对照表 × 右侧冻结销售单无遮挡并排核对。
 */
export function WorkspaceApprovalContextRail({
    instanceId,
    timelineTarget,
    currentDocument,
    materialFiles = [],
    materialsLoading = false,
    sourceSalesOrders,
    sourceSalesSnapshots,
    activeSourceRevisionId,
    onActiveSourceRevisionIdChange,
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
    const [internalTab, setInternalTab] =
        useState<ApprovalContextRailTab>("trail")
    const [internalSourceRevisionId, setInternalSourceRevisionId] = useState<
        string | undefined
    >(undefined)

    const hasSourceSalesTab = Boolean(sourceSalesOrders?.length)
    const hasTimelineTab = Boolean(timelineTarget)
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

    const selectedSourceRevisionId =
        activeSourceRevisionId ??
        internalSourceRevisionId ??
        sourceSalesOrders?.[0]?.revision.id
    const activeSourceOrder =
        sourceSalesOrders?.find(
            (order) => order.revision.id === selectedSourceRevisionId,
        ) ?? sourceSalesOrders?.[0]
    const activeSourceSnapshot = sourceSalesSnapshots?.find(
        (snapshot) => snapshot.revision_id === activeSourceOrder?.revision.id,
    )

    const handleSelectSourceRevision = (revisionId: string) => {
        if (activeSourceRevisionId === undefined) {
            setInternalSourceRevisionId(revisionId)
        }
        onActiveSourceRevisionIdChange?.(revisionId)
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
                className="flex min-h-0 flex-1 flex-col gap-0"
            >
                <div className="-mx-4 shrink-0 border-b border-border/60 px-4 @[1120px]/workspace:-mx-5 @[1120px]/workspace:px-5">
                    <TabsList
                        variant="line"
                        aria-label="审批右侧对照面板"
                        className="w-full gap-3.5 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
                    >
                        <TabsTrigger
                            id={`${resolvedPrefix}-tab-trail`}
                            value="trail"
                            className="shrink-0 text-xs"
                        >
                            审批记录
                        </TabsTrigger>
                        <TabsTrigger
                            id={`${resolvedPrefix}-tab-materials`}
                            value="materials"
                            className="shrink-0 gap-1 text-xs"
                        >
                            <span>附件对照</span>
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
                                <span>来源销售单</span>
                                {(sourceSalesOrders?.length ?? 0) > 1 ? (
                                    <span className="num text-tiny text-muted-foreground">
                                        ({sourceSalesOrders?.length})
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
                                单据全动态
                            </TabsTrigger>
                        ) : null}
                    </TabsList>
                </div>

                <TabsContent
                    value="trail"
                    className="mt-0 min-h-0 flex-1 overflow-y-auto overscroll-contain py-3.5"
                >
                    <UnifiedAuditTrail
                        instanceId={instanceId}
                        currentDocument={currentDocument}
                        isApprovalContext
                        hideTabs
                        activeTab="approval"
                        highlightedExecutionId={highlightedExecutionId}
                        idPrefix={`${resolvedPrefix}-audit`}
                    />
                </TabsContent>

                <TabsContent
                    value="materials"
                    className="mt-0 flex min-h-0 flex-1 flex-col overflow-hidden py-3"
                >
                    {materialsLoading && materialFiles.length === 0 ? (
                        <p
                            role="status"
                            className="py-6 text-center text-xs text-muted-foreground"
                        >
                            正在读取本次提交的附件…
                        </p>
                    ) : (
                        <UnifiedMaterialViewer
                            mode="docked"
                            files={materialFiles}
                            activeFileId={activeFileId}
                            onActiveFileChange={onActiveFileChange}
                            idPrefix={`${resolvedPrefix}-viewer`}
                            emptyMessage="本次提交未附带合同、凭证或其他附件。"
                            className="min-h-96 flex-1 @[960px]/workspace:min-h-0"
                        />
                    )}
                </TabsContent>

                {hasSourceSalesTab ? (
                    <TabsContent
                        value="source-sales"
                        className="mt-0 min-h-0 flex-1 overflow-y-auto overscroll-contain py-3.5"
                    >
                        <div className="space-y-3">
                            {sourceSalesOrders &&
                            sourceSalesOrders.length > 1 ? (
                                <div
                                    role="group"
                                    aria-label="切换来源销售单"
                                    className="flex flex-wrap items-center gap-1.5"
                                >
                                    {sourceSalesOrders.map((order) => {
                                        const selected =
                                            order.revision.id ===
                                            activeSourceOrder?.revision.id
                                        const docNo =
                                            displayBusinessText(
                                                order.document_no,
                                                order.revision.id,
                                            ) || "销售单"
                                        return (
                                            <Button
                                                key={order.revision.id}
                                                id={`${resolvedPrefix}-source-sales-${toAutomationIdSegment(order.revision.id)}`}
                                                type="button"
                                                size="xs"
                                                variant={
                                                    selected
                                                        ? "secondary"
                                                        : "outline"
                                                }
                                                aria-pressed={selected}
                                                className="num"
                                                onClick={() =>
                                                    handleSelectSourceRevision(
                                                        order.revision.id,
                                                    )
                                                }
                                            >
                                                {docNo} · V
                                                {order.revision.revision_no}
                                            </Button>
                                        )
                                    })}
                                </div>
                            ) : null}

                            {activeSourceOrder ? (
                                <ApprovalSalesOrderPaper
                                    submission={activeSourceOrder.revision}
                                    documentNo={
                                        displayBusinessText(
                                            activeSourceOrder.document_no,
                                            activeSourceSnapshot?.document_id,
                                        ) || "未记录单据编号"
                                    }
                                    submitterName={
                                        displayName(
                                            activeSourceSnapshot?.source
                                                .submitter_name,
                                        ) ?? null
                                    }
                                />
                            ) : (
                                <p className="py-6 text-center text-xs text-muted-foreground">
                                    暂无可对照的来源销售单快照。
                                </p>
                            )}
                        </div>
                    </TabsContent>
                ) : null}

                {hasTimelineTab ? (
                    <TabsContent
                        value="timeline"
                        className="mt-0 min-h-0 flex-1 overflow-y-auto overscroll-contain py-3.5"
                    >
                        <UnifiedAuditTrail
                            timelineTarget={timelineTarget}
                            currentDocument={currentDocument}
                            isApprovalContext
                            hideTabs
                            activeTab="timeline"
                            idPrefix={`${resolvedPrefix}-timeline`}
                        />
                    </TabsContent>
                ) : null}
            </Tabs>
        </div>
    )
}
