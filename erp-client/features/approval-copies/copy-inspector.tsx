"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import { ArrowUpRightIcon, RefreshCwIcon } from "lucide-react"

import { BusinessFailureState } from "@/components/business"
import {
    UnifiedMaterialViewer,
    type MaterialFileItem,
} from "@/components/business/unified-material-viewer"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { StatusBadge } from "@/components/ui/status-badge"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { COPY_TRIGGER_LABELS } from "@/features/approval-processes/copy-rules"
import { ApprovalSubmittedMaterialsContent } from "@/features/approval-workflow/components/approval-submitted-materials"
import { UnifiedAuditTrail } from "@/features/approval-workflow/components/unified-audit-trail"
import { displayUnixSeconds } from "@/features/approval-workflow/display"
import type { ApprovalHistoryItem } from "@/features/approval-workflow/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { downloadBlob } from "@/lib/download-blob"
import { getApprovalInstanceStatusMeta } from "@/lib/task-status-dictionary"
import { cn } from "@/lib/utils"
import {
    fetchCopyFile,
    type CopyDetail,
    type CopyHistoryItemDto,
    type CopyReceipt,
} from "./api"
import { documentTypeLabel } from "./columns"
import { CopyFiles } from "./copy-files"
import { CopyPurchaseLines } from "./purchase-lines"
import { useCopyFile, useCopyInfinite, useReadCopies } from "./queries"

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

export type CopyInspectorProps = Readonly<{
    receipt: CopyReceipt
    onOpenFullPage: () => void
    presentation: "pane" | "sheet"
    /** 调用方已在点选行时立即触发已读请求时传 `false`，避免重复请求。 */
    autoMarkRead?: boolean
    readFailed?: boolean
    onRetryRead?: () => void
}>

/**
 * 抄送原位速览检查器：用于 `WorkspaceSplitQueue` 右分栏或窄屏 `TaskActionSheet`。
 *
 * - 打开时按需自动标记未读抄送为已读；
 * - 使用 `bare` 冻结快照，无外框卡片嵌套与脆弱 `[&>article]` 选择器覆盖；
 * - 内置 `UnifiedMaterialViewer`（认证 Blob 原位对照）与追加式 `UnifiedAuditTrail`（不丢弃前页）。
 */
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
    const fileDownload = useCopyFile()

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

    const primaryPage = query.data?.pages[0]
    const historyItems = useMemo(
        () => mapCopyHistoryToAuditItems(query.data?.pages),
        [query.data?.pages],
    )

    const attachments = useMemo(
        () => primaryPage?.materials.attachments ?? [],
        [primaryPage?.materials.attachments],
    )
    const materialItems = useMemo<readonly MaterialFileItem[]>(
        () =>
            attachments.map((file) => ({
                id: file.file_asset_id,
                name: file.file_name,
                mimeType: file.content_type,
                sizeBytes: file.byte_size,
                loadPreview: () =>
                    fetchCopyFile(receipt.id, file.file_asset_id, true),
                onDownload: async () => {
                    const blob = await fileDownload.mutateAsync({
                        id: receipt.id,
                        file: file.file_asset_id,
                        preview: false,
                    })
                    downloadBlob(blob, file.file_name)
                },
            })),
        [attachments, fileDownload, receipt.id],
    )

    const segment = toAutomationIdSegment(receipt.id)
    const padClass = presentation === "sheet" ? "px-sheet-inline" : "px-5"
    const copiedAt = displayUnixSeconds(receipt.copied_at)
    const statusMeta = getApprovalInstanceStatusMeta(
        primaryPage?.instance_status ?? receipt.instance_status,
    )

    if (query.isError) {
        return (
            <div
                className={cn(
                    "flex min-h-0 flex-1 flex-col justify-center py-6",
                    padClass,
                )}
            >
                <BusinessFailureState
                    idPrefix={`copy-inspector-failure-${segment}`}
                    title="抄送资料读取失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                    action={
                        <Button
                            id={`copy-inspector-error-open-${segment}`}
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

    if (query.isPending || !primaryPage) {
        return (
            <div
                aria-busy="true"
                className={cn(
                    "flex min-h-0 flex-1 flex-col gap-4 py-5",
                    padClass,
                )}
            >
                <span role="status" className="sr-only">
                    正在读取抄送资料…
                </span>
                <div className="space-y-2 border-b border-border/60 pb-4">
                    <Skeleton className="h-4 w-32" />
                    <Skeleton className="h-5 w-56" />
                </div>
                <Skeleton className="h-32 w-full" />
                <Skeleton className="h-40 w-full" />
            </div>
        )
    }

    return (
        <div
            data-slot="copy-inspector"
            className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-card"
        >
            <div
                className={cn(
                    "flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken/50 py-2 text-xs text-muted-foreground",
                    padClass,
                )}
            >
                <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                    <StatusBadge
                        label={statusMeta.label}
                        tone={statusMeta.tone}
                    />
                    <span>{documentTypeLabel(receipt.document_type)}审批</span>
                    <span>·</span>
                    <span>{COPY_TRIGGER_LABELS[receipt.trigger]}</span>
                    {receipt.node_name ? (
                        <>
                            <span>·</span>
                            <span>{receipt.node_name}</span>
                        </>
                    ) : null}
                    <span>·</span>
                    <span className="num">第 {receipt.round_no} 轮</span>
                    {copiedAt ? (
                        <>
                            <span>·</span>
                            <time dateTime={copiedAt.dateTime} className="num">
                                {copiedAt.label}
                            </time>
                        </>
                    ) : null}
                </div>
                <Button
                    id={`copy-inspector-refresh-${segment}`}
                    type="button"
                    variant="ghost"
                    size="xs"
                    className="h-control-xs px-2 text-xs text-muted-foreground hover:text-foreground"
                    disabled={query.isFetching}
                    onClick={() => void query.refetch()}
                >
                    <RefreshCwIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                    />
                    刷新
                </Button>
            </div>

            {mark.isError || readFailed ? (
                <div
                    role="alert"
                    className={cn(
                        "flex shrink-0 flex-wrap items-center gap-2 border-b border-destructive-border bg-destructive-soft py-2 text-xs text-destructive-soft-foreground",
                        padClass,
                    )}
                >
                    <span>已读状态保存失败。</span>
                    <Button
                        id={`copy-inspector-read-retry-${segment}`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        onClick={() =>
                            onRetryRead
                                ? onRetryRead()
                                : mark.mutate([receipt.id])
                        }
                    >
                        重试标记已读
                    </Button>
                </div>
            ) : null}

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
                className="flex min-h-0 flex-1 flex-col gap-0 overflow-hidden"
            >
                <div
                    className={cn(
                        "shrink-0 border-b border-border/60 bg-card",
                        padClass,
                    )}
                >
                    <TabsList variant="line" aria-label="抄送检查器视图">
                        <TabsTrigger
                            id={`copy-inspector-tab-snapshot-${segment}`}
                            value="snapshot"
                        >
                            单据快照
                        </TabsTrigger>
                        <TabsTrigger
                            id={`copy-inspector-tab-materials-${segment}`}
                            value="materials"
                        >
                            附件对照
                            {attachments.length > 0 ? (
                                <span className="num ml-1 text-xs text-muted-foreground">
                                    ({attachments.length})
                                </span>
                            ) : null}
                        </TabsTrigger>
                        <TabsTrigger
                            id={`copy-inspector-tab-history-${segment}`}
                            value="history"
                        >
                            审批记录
                            {historyItems.length > 0 ? (
                                <span className="num ml-1 text-xs text-muted-foreground">
                                    ({historyItems.length}
                                    {query.hasNextPage ? "+" : ""})
                                </span>
                            ) : null}
                        </TabsTrigger>
                    </TabsList>
                </div>

                <TabsContent
                    value="snapshot"
                    className={cn(
                        "mt-0 min-h-0 flex-1 space-y-6 overflow-y-auto overscroll-contain py-5",
                        padClass,
                    )}
                >
                    <ApprovalSubmittedMaterialsContent
                        bare
                        instanceId={primaryPage.receipt.instance_id}
                        data={primaryPage.materials}
                        attachmentContent={
                            <CopyFiles
                                id={receipt.id}
                                files={attachments}
                                activeFileId={activeFileId}
                                onSelectFile={(fileId) => {
                                    setActiveFileId(fileId)
                                    setActiveTab("materials")
                                }}
                                idPrefix={`copy-inspector-file-${segment}`}
                            />
                        }
                    />

                    <CopyPurchaseLines
                        lines={primaryPage.materials.purchase_lines ?? []}
                        fulfillment={primaryPage.materials.purchase_fulfillment}
                        idPrefix={`copy-inspector-purchase-${segment}`}
                    />

                    {(primaryPage.materials.source_sales_orders?.length ?? 0) >
                    0 ? (
                        <section className="divide-y divide-border/60 border-t border-border/60 pt-4">
                            {primaryPage.materials.source_sales_orders?.map(
                                (order) => (
                                    <details
                                        key={order.revision.id}
                                        className="py-3 first:pt-0 last:pb-0"
                                    >
                                        <summary
                                            id={`copy-inspector-source-sales-${toAutomationIdSegment(order.revision.id)}`}
                                            className="cursor-pointer text-body-compact font-medium"
                                        >
                                            查看关联销售单完整资料 ·{" "}
                                            <span className="num">
                                                {order.document_no}
                                            </span>
                                        </summary>
                                        <div className="pt-4">
                                            <ApprovalSubmittedMaterialsContent
                                                bare
                                                instanceId={
                                                    primaryPage.receipt
                                                        .instance_id
                                                }
                                                data={primaryPage.materials}
                                                view="source-sales"
                                                sourceRevisionId={
                                                    order.revision.id
                                                }
                                                attachmentContent={
                                                    <p className="text-xs text-muted-foreground">
                                                        附件请在上方「附件对照」页签中查看。
                                                    </p>
                                                }
                                            />
                                        </div>
                                    </details>
                                ),
                            )}
                        </section>
                    ) : null}
                </TabsContent>

                <TabsContent
                    value="materials"
                    className={cn(
                        "mt-0 flex min-h-0 flex-1 flex-col overflow-hidden py-4",
                        padClass,
                    )}
                >
                    <UnifiedMaterialViewer
                        mode="docked"
                        files={materialItems}
                        activeFileId={activeFileId}
                        onActiveFileChange={setActiveFileId}
                        idPrefix={`copy-inspector-viewer-${segment}`}
                        className="min-h-0 flex-1"
                    />
                </TabsContent>

                <TabsContent
                    value="history"
                    className={cn(
                        "mt-0 min-h-0 flex-1 overflow-y-auto overscroll-contain py-5",
                        padClass,
                    )}
                >
                    <UnifiedAuditTrail
                        historyItems={historyItems}
                        historyHasMore={Boolean(query.hasNextPage)}
                        historyLoadingMore={query.isFetchingNextPage}
                        onLoadMoreHistory={() => void query.fetchNextPage()}
                        showCopyHistory={false}
                        idPrefix={`copy-inspector-audit-${segment}`}
                    />
                </TabsContent>
            </Tabs>
        </div>
    )
}
