"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import { ArrowUpRightIcon, ChevronDownIcon, RefreshCwIcon } from "lucide-react"

import {
    BusinessFailureState,
    PaperDocumentViewport,
    UnifiedMaterialViewer,
    WorkspaceFactGrid,
    WorkspaceFactItem,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { Skeleton } from "@/components/ui/skeleton"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { COPY_TRIGGER_LABELS } from "@/features/approval-processes/copy-rules"
import type { ApprovalMaterials } from "@/features/approval-workflow/api/materials"
import { ApprovalSalesOrderPaper } from "@/features/approval-workflow/components/approval-sales-order-paper"
import { ApprovalSubmittedSummary } from "@/features/approval-workflow/components/approval-submitted-summary"
import { UnifiedAuditTrail } from "@/features/approval-workflow/components/unified-audit-trail"
import {
    displayActorName,
    displayUnixSeconds,
} from "@/features/approval-workflow/display"
import type { ApprovalHistoryItem } from "@/features/approval-workflow/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayBusinessText } from "@/lib/display-name"
import { cn } from "@/lib/utils"
import type { CopyDetail, CopyHistoryItemDto, CopyReceipt } from "./api"
import { CopyFiles, useCopyFileItems } from "./copy-files"
import { CopyPurchaseLines } from "./purchase-lines"
import { useCopyInfinite, useReadCopies } from "./queries"

/** 将抄送接口返回的追加分页执行历史去重并映射为 `UnifiedAuditTrail` 标准项。 */
export function mapCopyHistoryToAuditItems(
    pages: readonly CopyDetail[] | undefined,
): ApprovalHistoryItem[] {
    if (!pages?.length) return []
    const seen = new Set<string>()
    const items: ApprovalHistoryItem[] = []
    let fallbackOrder = 0
    for (const page of pages) {
        for (const row of page.history.items) {
            if (seen.has(row.execution_id)) continue
            seen.add(row.execution_id)
            fallbackOrder += 1
            items.push(mapSingleCopyHistoryItem(row, fallbackOrder))
        }
    }
    return items
}

function mapSingleCopyHistoryItem(
    row: CopyHistoryItemDto,
    fallbackExecutionNo: number,
): ApprovalHistoryItem {
    return {
        executionId: row.execution_id,
        roundNo: row.round_no,
        executionNo: row.execution_no ?? fallbackExecutionNo,
        nodeKey: row.node_key ?? row.execution_id,
        nodeName: row.node_name || "审批节点未标注",
        result: row.result,
        assigneeName: row.assignee_name ?? undefined,
        decidedBy: row.decided_by ?? undefined,
        decisionReason: row.decision_reason ?? undefined,
        decidedAt: row.decided_at ?? undefined,
    }
}

type CopyInspectorTab = "snapshot" | "materials" | "history"

const EMPTY_ATTACHMENTS: ApprovalMaterials["attachments"] = []

export type CopyInspectorProps = Readonly<{
    receipt: CopyReceipt
    onOpenFullPage: () => void
    presentation: "pane" | "sheet"
    /** 调用方已在点选行时立即触发已读请求时传 `false`，避免重复请求。 */
    autoMarkRead?: boolean
    readFailed?: boolean
    onRetryRead?: () => void
}>

export function CopyMarkReadError({
    id,
    onRetry,
}: {
    id: string
    onRetry: () => void
}) {
    return (
        <div
            role="alert"
            className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-r-md border-l-2 border-l-destructive bg-destructive-soft/65 px-3 py-1.5 text-xs text-destructive-soft-foreground"
        >
            <span>已读状态保存失败。</span>
            <Button
                id={id}
                type="button"
                variant="ghost"
                size="xs"
                onClick={onRetry}
            >
                重试标记已读
            </Button>
        </div>
    )
}

export function CopySnapshot({
    detail,
    idPrefix,
    showAttachments,
    onSelectFile,
    activeFileId,
}: {
    detail: CopyDetail
    idPrefix: string
    showAttachments: boolean
    onSelectFile: (id: string) => void
    activeFileId?: string
}) {
    const { receipt, materials } = detail
    const {
        display,
        sales_order: salesOrder,
        purchase_lines: purchaseLines,
    } = materials
    const source = display.source
    const submitter = displayActorName(
        source.submitter_name,
        salesOrder?.submitted_by,
    )
    const sourceOrders = materials.source_sales_orders ?? []
    const sourceBriefs = display.source_sales ?? []
    const fullSourceIds = new Set(
        sourceOrders.map((order) => order.revision.id),
    )
    const briefOnlySources = sourceBriefs.filter(
        (brief) => !fullSourceIds.has(brief.revision_id),
    )

    return (
        <div className="min-w-0 space-y-4 wrap-anywhere">
            <p className="num text-xs text-muted-foreground">
                提交版本 {materials.subject_version}
                {!salesOrder &&
                source.extra_sections.length === 0 &&
                source.list_summary
                    ? ` · ${source.list_summary}`
                    : null}
            </p>
            {salesOrder ? (
                <CopySalesPaper
                    submission={salesOrder}
                    documentNo={receipt.document_no}
                    submitterName={submitter ?? null}
                />
            ) : (
                <ApprovalSubmittedSummary
                    idPrefix={`${idPrefix}-summary`}
                    source={source}
                    submitterName={submitter}
                    impactSummary={display.impact_summary}
                    lineDetails={
                        purchaseLines?.length ? (
                            <CopyPurchaseLines
                                lines={purchaseLines}
                                fulfillment={materials.purchase_fulfillment}
                                idPrefix={`${idPrefix}-purchase`}
                            />
                        ) : undefined
                    }
                />
            )}

            {sourceOrders.length || briefOnlySources.length ? (
                <section className="min-w-0 border-t border-border/60 pt-3">
                    <h3 className="text-section-title font-semibold">
                        关联销售单
                    </h3>
                    <div className="divide-y divide-border/60">
                        {sourceOrders.map((order) => (
                            <CopySourceDisclosure
                                key={order.revision.id}
                                id={`${idPrefix}-source-sales-${toAutomationIdSegment(order.revision.id)}`}
                                documentNo={order.document_no}
                                revisionNo={order.revision.revision_no}
                            >
                                <CopySalesPaper
                                    submission={order.revision}
                                    documentNo={order.document_no}
                                    submitterName={submitter ?? null}
                                    hideDetailedDeliveryAddresses={
                                        materials.document_type ===
                                        "purchase_order"
                                    }
                                />
                            </CopySourceDisclosure>
                        ))}
                        {briefOnlySources.map((brief) => (
                            <CopySourceDisclosure
                                key={brief.revision_id}
                                id={`${idPrefix}-source-sales-${toAutomationIdSegment(brief.revision_id)}`}
                                documentNo={
                                    displayBusinessText(
                                        brief.document_no,
                                        brief.document_id,
                                    ) || "未记录销售单号"
                                }
                                revisionNo={brief.revision_no}
                            >
                                <ApprovalSubmittedSummary
                                    idPrefix={`${idPrefix}-source-summary-${toAutomationIdSegment(brief.revision_id)}`}
                                    source={brief.source}
                                    submitterName={displayActorName(
                                        brief.source.submitter_name,
                                    )}
                                    impactSummary={null}
                                    nested
                                />
                            </CopySourceDisclosure>
                        ))}
                    </div>
                </section>
            ) : null}

            {showAttachments && materials.attachments.length > 0 ? (
                <section className="min-w-0 space-y-2 border-t border-border/60 pt-3">
                    <h3 className="text-section-title font-semibold">
                        提交附件
                    </h3>
                    <CopyFiles
                        id={receipt.id}
                        files={materials.attachments}
                        activeFileId={activeFileId}
                        onSelectFile={onSelectFile}
                        idPrefix={`${idPrefix}-file`}
                    />
                </section>
            ) : null}
        </div>
    )
}

function CopySalesPaper(
    props: React.ComponentProps<typeof ApprovalSalesOrderPaper>,
) {
    return (
        <PaperDocumentViewport
            fitKey={props.documentNo}
            fitMode="width"
            className="min-w-0 overflow-hidden bg-card p-0! [&>div]:mb-0!"
        >
            <ApprovalSalesOrderPaper {...props} />
        </PaperDocumentViewport>
    )
}

function CopySourceDisclosure({
    id,
    documentNo,
    revisionNo,
    children,
}: {
    id: string
    documentNo: string
    revisionNo: number
    children: React.ReactNode
}) {
    return (
        <Collapsible>
            <CollapsibleTrigger
                id={id}
                render={<Button type="button" variant="ghost" size="sm" />}
                className="group h-auto min-h-control-sm w-full justify-start py-1.5 text-left whitespace-normal"
            >
                <ChevronDownIcon
                    aria-hidden="true"
                    className="-rotate-90 transition-transform group-data-panel-open:rotate-0"
                />
                <span className="num min-w-0 wrap-anywhere">{documentNo}</span>
                <span className="num text-xs text-muted-foreground">
                    V{revisionNo}
                </span>
            </CollapsibleTrigger>
            <CollapsibleContent
                id={`${id}-content`}
                className="min-w-0 space-y-3 pb-3"
            >
                {children}
            </CollapsibleContent>
        </Collapsible>
    )
}

/** 抄送队列和轻作业抽屉共用的冻结快照检查器。 */
export function CopyInspector({
    receipt,
    onOpenFullPage,
    presentation,
    autoMarkRead = true,
    readFailed = false,
    onRetryRead,
}: CopyInspectorProps) {
    const [activeTab, setActiveTab] = useState<CopyInspectorTab>("snapshot")
    const [activeFileId, setActiveFileId] = useState<string | undefined>()
    const query = useCopyInfinite(receipt.id)
    const mark = useReadCopies()

    const attemptedRef = useRef<string | null>(null)
    useEffect(() => {
        if (
            autoMarkRead &&
            !receipt.read_at &&
            attemptedRef.current !== receipt.id &&
            !mark.isPending
        ) {
            attemptedRef.current = receipt.id
            mark.mutate([receipt.id])
        }
    }, [autoMarkRead, mark, receipt.id, receipt.read_at])

    const primaryPage =
        query.isError && !query.isFetchNextPageError
            ? undefined
            : query.data?.pages[0]
    const historyItems = useMemo(
        () => mapCopyHistoryToAuditItems(query.data?.pages),
        [query.data?.pages],
    )
    const attachments = primaryPage?.materials.attachments ?? EMPTY_ATTACHMENTS
    const { items: materialItems } = useCopyFileItems(receipt.id, attachments)
    const selectFile = (fileId: string) => {
        setActiveFileId(fileId)
        setActiveTab("materials")
    }

    const idPrefix = `copy-inspector-${toAutomationIdSegment(receipt.id)}`
    const padClass = presentation === "sheet" ? "px-sheet-inline" : "px-5"
    const copiedAt = displayUnixSeconds(receipt.copied_at)
    const isRead = Boolean(
        receipt.read_at || primaryPage?.receipt.read_at || mark.isSuccess,
    )

    if (!primaryPage && query.isError) {
        return (
            <div
                className={cn(
                    "flex min-h-0 min-w-0 flex-1 flex-col justify-center bg-card py-6",
                    padClass,
                )}
            >
                <BusinessFailureState
                    idPrefix={`${idPrefix}-failure`}
                    title="抄送资料读取失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                    action={
                        <Button
                            id={`${idPrefix}-error-open`}
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={onOpenFullPage}
                        >
                            在完整页面打开
                            <ArrowUpRightIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        </Button>
                    }
                />
            </div>
        )
    }

    if (!primaryPage) {
        return (
            <div
                aria-busy="true"
                className={cn(
                    "flex min-h-0 min-w-0 flex-1 flex-col gap-4 bg-card py-5",
                    padClass,
                )}
            >
                <span role="status" className="sr-only">
                    正在读取抄送资料…
                </span>
                <Skeleton className="h-14 w-full" />
                <Skeleton className="h-32 w-full" />
                <Skeleton className="h-40 w-full" />
            </div>
        )
    }

    const copyNodeLabel = `${COPY_TRIGGER_LABELS[receipt.trigger]}${receipt.node_name ? ` · ${receipt.node_name}` : ""}`

    return (
        <div
            data-slot="copy-inspector"
            className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-card"
        >
            <div className={cn("shrink-0 space-y-2 pt-3 pb-2", padClass)}>
                <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0 flex-1">
                        <WorkspaceFactGrid columns={3} ariaLabel="抄送信息">
                            <WorkspaceFactItem
                                label="阅读状态"
                                value={isRead ? "已读" : "未读"}
                            />
                            <WorkspaceFactItem
                                label="抄送节点"
                                value={copyNodeLabel}
                                span={copyNodeLabel.length > 30 ? "full" : 1}
                            />
                            <WorkspaceFactItem
                                label="审批轮次"
                                value={`第 ${receipt.round_no} 轮`}
                                numeric
                            />
                            {copiedAt ? (
                                <WorkspaceFactItem label="抄送时间" numeric>
                                    <time dateTime={copiedAt.dateTime}>
                                        {copiedAt.label}
                                    </time>
                                </WorkspaceFactItem>
                            ) : null}
                            <WorkspaceFactItem
                                label="附件"
                                value={`${attachments.length} 份`}
                                numeric
                            />
                        </WorkspaceFactGrid>
                    </div>
                    <Button
                        id={`${idPrefix}-refresh`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        className="shrink-0 text-muted-foreground"
                        disabled={query.isFetching}
                        onClick={() => void query.refetch()}
                    >
                        <RefreshCwIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                            className={
                                query.isFetching ? "animate-spin" : undefined
                            }
                        />
                        刷新
                    </Button>
                </div>

                {mark.isError || readFailed ? (
                    <CopyMarkReadError
                        id={`${idPrefix}-read-retry`}
                        onRetry={
                            onRetryRead ?? (() => mark.mutate([receipt.id]))
                        }
                    />
                ) : null}
            </div>

            <Tabs
                value={activeTab}
                onValueChange={(next) => {
                    if (
                        next === "snapshot" ||
                        next === "materials" ||
                        next === "history"
                    ) {
                        setActiveTab(next)
                    }
                }}
                className="flex min-h-0 min-w-0 flex-1 flex-col gap-0 overflow-hidden"
            >
                <div
                    className={cn(
                        "shrink-0 border-b border-border/60 bg-card",
                        padClass,
                    )}
                >
                    <TabsList
                        variant="line"
                        aria-label="抄送检查器视图"
                        className="max-w-full gap-3"
                    >
                        <TabsTrigger
                            id={`${idPrefix}-tab-snapshot`}
                            aria-controls={`${idPrefix}-panel-snapshot`}
                            value="snapshot"
                        >
                            单据快照
                        </TabsTrigger>
                        <TabsTrigger
                            id={`${idPrefix}-tab-materials`}
                            aria-controls={`${idPrefix}-panel-materials`}
                            value="materials"
                            className="gap-1"
                        >
                            <span>附件对照</span>
                            <span className="num text-xs text-muted-foreground">
                                ({attachments.length})
                            </span>
                        </TabsTrigger>
                        <TabsTrigger
                            id={`${idPrefix}-tab-history`}
                            aria-controls={`${idPrefix}-panel-history`}
                            value="history"
                            className="gap-1"
                        >
                            <span>审批记录</span>
                            <span className="num text-xs text-muted-foreground">
                                ({historyItems.length}
                                {query.hasNextPage ? "+" : ""})
                            </span>
                        </TabsTrigger>
                    </TabsList>
                </div>

                <TabsContent
                    id={`${idPrefix}-panel-snapshot`}
                    value="snapshot"
                    className={cn(
                        "min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-contain py-4",
                        padClass,
                    )}
                >
                    <div className="@container/copy-detail min-w-0">
                        <CopySnapshot
                            detail={primaryPage}
                            idPrefix={idPrefix}
                            showAttachments
                            activeFileId={activeFileId}
                            onSelectFile={selectFile}
                        />
                    </div>
                </TabsContent>
                <TabsContent
                    id={`${idPrefix}-panel-materials`}
                    value="materials"
                    className={cn(
                        "flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden py-4",
                        padClass,
                    )}
                >
                    <UnifiedMaterialViewer
                        mode="docked"
                        files={materialItems}
                        activeFileId={activeFileId}
                        onActiveFileChange={setActiveFileId}
                        idPrefix={`${idPrefix}-viewer`}
                        className="min-h-0 flex-1"
                    />
                </TabsContent>
                <TabsContent
                    id={`${idPrefix}-panel-history`}
                    value="history"
                    className={cn(
                        "min-h-0 min-w-0 flex-1 space-y-2 overflow-x-hidden overflow-y-auto overscroll-contain py-4",
                        padClass,
                    )}
                >
                    {query.isFetchNextPageError ? (
                        <p role="alert" className="text-xs text-destructive">
                            审批记录加载失败，请重试加载更多。
                        </p>
                    ) : null}
                    <UnifiedAuditTrail
                        historyItems={historyItems}
                        historyHasMore={Boolean(query.hasNextPage)}
                        historyLoadingMore={query.isFetchingNextPage}
                        onLoadMoreHistory={() => void query.fetchNextPage()}
                        showCopyHistory={false}
                        idPrefix={`${idPrefix}-audit`}
                    />
                </TabsContent>
            </Tabs>
        </div>
    )
}
