"use client"

import { useState } from "react"
import { HistoryIcon, PaperclipIcon } from "lucide-react"

import {
    UnifiedMaterialViewer,
    type MaterialFileItem,
} from "@/components/business/unified-material-viewer"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { ApprovalRouteStepper } from "@/features/approval-workflow/components/approval-route-stepper"
import {
    UnifiedAuditTrail,
    type AuditTrailTab,
} from "@/features/approval-workflow/components/unified-audit-trail"
import {
    displayActorName,
    displayReadableName,
    displayRound,
    displayUnixSeconds,
} from "@/features/approval-workflow/display"
import { useApprovalHistoryInfiniteQuery } from "@/features/approval-workflow/queries"
import type { ApprovalRuntimeInstance } from "@/features/approval-workflow/types"
import type { TimelineTarget } from "@/features/document-timeline/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export type ApprovalContextRailTab = "trail" | "materials"

export type WorkspaceApprovalContextRailProps = Readonly<{
    instanceId: string
    instance?: ApprovalRuntimeInstance
    /** 单据动态查询目标（传入后可在「单据全动态」中直接查看当前任务与销售全链路）。 */
    timelineTarget?: TimelineTarget
    currentDocument?: { type: string; id: string }
    /** 右栏「附件对照」可预览的认证附件列表。 */
    materialFiles?: readonly MaterialFileItem[]
    materialsLoading?: boolean
    /** 是否处于退回重办上下文（置顶高亮上轮驳回意见）。 */
    isRework?: boolean
    activeTab?: ApprovalContextRailTab
    onActiveTabChange?: (tab: ApprovalContextRailTab) => void
    activeFileId?: string
    onActiveFileChange?: (fileId: string) => void
    idPrefix?: string
    className?: string
}>

/**
 * 2 栏决策审批工作台右侧上下文轨：
 * - 顶部：44px 紧凑 `ApprovalRouteStepper`（释放正文首屏纵向高度，消灭模态历史弹窗）；
 * - 退回重办态：置顶呈现上轮驳回摘要并支持定位至对应历史记录；
 * - 主体分段：
 *   1.「审批与轨迹」：倒序 `UnifiedAuditTrail`（合并审批与抄送 + 单据全动态）；
 *   2.「附件对照 (N)」：侧边对接 `UnifiedMaterialViewer (mode="docked")`，支持拖拽中轴拉宽至 50% 左右对照合同/回单。
 */
export function WorkspaceApprovalContextRail({
    instanceId,
    instance,
    timelineTarget,
    currentDocument,
    materialFiles = [],
    materialsLoading = false,
    isRework = false,
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
    const [auditTab, setAuditTab] = useState<AuditTrailTab>("approval")
    const [highlightedExecutionId, setHighlightedExecutionId] = useState<
        string | null
    >(null)

    const currentTab = activeTab ?? internalTab
    const setTab = (next: ApprovalContextRailTab) => {
        if (activeTab === undefined) setInternalTab(next)
        onActiveTabChange?.(next)
    }

    const historyQuery = useApprovalHistoryInfiniteQuery(
        { instanceId, order: "desc" },
        Boolean(instanceId),
    )
    const route = historyQuery.data?.pages[0]?.route
    const historyItems =
        historyQuery.data?.pages.flatMap((page) => page.items) ?? []
    const latestRejectedRecord = historyItems.find(
        (record) => record.result === "REJECTED",
    )
    const rejectedAt = displayUnixSeconds(latestRejectedRecord?.decidedAt)
    const rejector = displayActorName(
        latestRejectedRecord?.assigneeName ?? instance?.latestRejectionByName,
        latestRejectedRecord?.decidedBy,
    )
    const rejectedNode = displayReadableName(
        latestRejectedRecord?.nodeName,
        latestRejectedRecord?.nodeKey,
    )
    const rejectionReason =
        latestRejectedRecord?.decisionReason?.trim() ||
        instance?.latestRejection?.trim()

    const processLabel = displayReadableName(instance?.processName)
    const roundLabel = displayRound(
        route?.roundNo ?? instance?.currentRoundNo ?? 1,
    )

    const handleJumpToHistory = (executionId?: string) => {
        setTab("trail")
        setAuditTab("approval")
        setHighlightedExecutionId(executionId ?? null)
    }

    return (
        <div
            data-slot="workspace-approval-context-rail"
            className={cn(
                "flex h-full min-h-0 min-w-0 flex-col bg-card",
                className,
            )}
        >
            <div className="shrink-0 space-y-2 border-b border-border/60 pt-2.5 pb-2">
                <div className="flex flex-wrap items-center justify-between gap-2">
                    <span className="text-xs font-semibold text-foreground">
                        {processLabel && processLabel !== "审批流程"
                            ? processLabel
                            : "审批流程"}
                    </span>
                    <span className="num text-tiny text-muted-foreground">
                        {roundLabel}
                    </span>
                </div>
                {route?.nodes.length ? (
                    <ApprovalRouteStepper
                        route={route}
                        instanceStatus={instance?.status}
                        latestRejection={
                            isRework ? null : instance?.latestRejection
                        }
                        latestRejectionByName={
                            isRework ? null : instance?.latestRejectionByName
                        }
                        onJumpToHistory={handleJumpToHistory}
                        idPrefix={`${resolvedPrefix}-stepper`}
                    />
                ) : historyQuery.isPending ? (
                    <p
                        role="status"
                        className="py-2 text-xs text-muted-foreground"
                    >
                        正在加载审批路线…
                    </p>
                ) : null}
                {isRework && rejectionReason ? (
                    <Alert variant="destructive" className="py-2.5">
                        <AlertTitle className="text-xs">
                            上轮驳回意见
                            {rejector ? ` · ${rejector}` : ""}
                        </AlertTitle>
                        <AlertDescription className="space-y-1 text-xs">
                            <p className="whitespace-pre-wrap break-words">
                                {rejectionReason}
                            </p>
                            {rejectedNode || rejectedAt ? (
                                <p className="num text-tiny opacity-85">
                                    {[rejectedNode, rejectedAt?.label]
                                        .filter(Boolean)
                                        .join(" · ")}
                                </p>
                            ) : null}
                        </AlertDescription>
                    </Alert>
                ) : null}
            </div>

            <Tabs
                value={currentTab}
                onValueChange={(next) => {
                    if (next === "trail" || next === "materials") {
                        setTab(next)
                    }
                }}
                className="flex min-h-0 flex-1 flex-col gap-0"
            >
                <div className="shrink-0 border-b border-border/60 pt-2 pb-1.5">
                    <TabsList
                        aria-label="审批右侧对照面板"
                        className="grid w-full grid-cols-2"
                    >
                        <TabsTrigger
                            id={`${resolvedPrefix}-tab-trail`}
                            value="trail"
                            className="gap-1.5 text-xs"
                        >
                            <HistoryIcon
                                aria-hidden="true"
                                className="size-3.5 shrink-0"
                            />
                            <span>审批与轨迹</span>
                        </TabsTrigger>
                        <TabsTrigger
                            id={`${resolvedPrefix}-tab-materials`}
                            value="materials"
                            className="gap-1.5 text-xs"
                        >
                            <PaperclipIcon
                                aria-hidden="true"
                                className="size-3.5 shrink-0"
                            />
                            <span>附件对照</span>
                            {materialFiles.length > 0 ? (
                                <Badge
                                    variant="secondary"
                                    className="num h-4 rounded-full px-1.5 text-tiny"
                                >
                                    {materialFiles.length}
                                </Badge>
                            ) : null}
                        </TabsTrigger>
                    </TabsList>
                </div>

                <TabsContent
                    value="trail"
                    className="mt-0 min-h-0 flex-1 overflow-y-auto overscroll-contain py-3"
                >
                    <UnifiedAuditTrail
                        instanceId={instanceId}
                        timelineTarget={timelineTarget}
                        currentDocument={currentDocument}
                        isApprovalContext
                        activeTab={auditTab}
                        onActiveTabChange={setAuditTab}
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
            </Tabs>
        </div>
    )
}
