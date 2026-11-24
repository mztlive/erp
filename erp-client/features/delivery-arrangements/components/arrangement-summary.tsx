"use client"

import { useProductUnitNames } from "@/hooks/use-product-unit-names"

import * as React from "react"
import {
    CheckIcon,
    ChevronDownIcon,
    ChevronRightIcon,
    CopyIcon,
    LockIcon,
    MapPinIcon,
    TriangleAlertIcon,
} from "lucide-react"

import {
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toast } from "@/components/ui/toast"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { ArrangementView } from "../api"
import { useArrangement } from "../hooks/queries"
import {
    deriveArrangementSummary,
    formatArrangementQuantity,
    formatCopyableAddress,
    type ArrangementLineMeta,
    type DerivedAllocationRow,
    type DerivedArrangementLine,
    type DerivedArrangementSummary,
} from "../lib/arrangement-model"
import { ArrangementDialog } from "./arrangement-button"
import {
    ArrangementDetailSheet,
    type ArrangementDetailViewMode,
} from "./arrangement-detail-sheet"

const INLINE_PREVIEW_LIMIT = 5
const UNIFIED_ITEM_PREVIEW_LIMIT = 4

/** 展示当前履约收货安排，与销售审核版本中的原始要求分开。 */
export function ArrangementSummary({
    salesOrderId,
    canEdit = false,
    lineMeta,
}: {
    salesOrderId: string
    canEdit?: boolean
    lineMeta?: Readonly<Record<string, ArrangementLineMeta>>
}) {
    const query = useArrangement("sales-orders", salesOrderId, true)
    const { names } = useProductUnitNames([], Boolean(query.data?.lines.length))
    const orderSeg = toAutomationIdSegment(salesOrderId)
    const idPrefix = `sales-arrangement-${orderSeg}`

    const [sheetState, setSheetState] = React.useState<{
        open: boolean
        view: ArrangementDetailViewMode
        lineId: string | null
    }>({ open: false, view: "product", lineId: null })
    const [editOpen, setEditOpen] = React.useState(false)
    const [expandedLineIds, setExpandedLineIds] = React.useState<
        ReadonlySet<string>
    >(() => new Set())
    const [copiedKey, setCopiedKey] = React.useState<string | null>(null)
    const copyTimerRef = React.useRef<number | null>(null)

    React.useEffect(() => {
        return () => {
            if (copyTimerRef.current != null) {
                window.clearTimeout(copyTimerRef.current)
            }
        }
    }, [])

    const handleCopy = React.useCallback(
        (
            copyId: string,
            target: { recipient: string; phone: string; fullAddress: string },
        ) => {
            const text = formatCopyableAddress(target)
            if (!text || typeof navigator === "undefined") return
            void navigator.clipboard
                ?.writeText(text)
                .then(() => {
                    setCopiedKey(copyId)
                    if (copyTimerRef.current != null) {
                        window.clearTimeout(copyTimerRef.current)
                    }
                    copyTimerRef.current = window.setTimeout(() => {
                        setCopiedKey(null)
                    }, 1800)
                    toast.add({
                        title: "收货地址已复制",
                        description: `${target.recipient} · ${target.phone}`,
                        type: "success",
                        timeout: 2200,
                    })
                })
                .catch(() => {
                    toast.add({
                        title: "复制失败",
                        description:
                            "当前浏览器未授予剪贴板权限，请手动选中复制。",
                        type: "warning",
                        timeout: 3000,
                    })
                })
        },
        [],
    )

    const summary = React.useMemo(
        () =>
            query.data
                ? deriveArrangementSummary(query.data, lineMeta, names)
                : undefined,
        [query.data, lineMeta, names],
    )

    const unitLabels = React.useMemo(() => {
        const labels: Record<string, string> = {}
        for (const line of summary?.lines ?? []) {
            if (line.unit) labels[line.lineId] = line.unit
        }
        return labels
    }, [summary])

    if (query.isPending) {
        return (
            <DetailWorkspaceSection
                width="form"
                title="当前收货安排"
                headingId="sales-order-arrangement-heading"
            >
                <p role="status" className="text-xs text-muted-foreground">
                    正在读取当前收货安排…
                </p>
            </DetailWorkspaceSection>
        )
    }

    if (query.isError) {
        return (
            <DetailWorkspaceSection
                width="form"
                title="当前收货安排"
                headingId="sales-order-arrangement-heading"
            >
                <div className="space-y-2.5">
                    <p className="text-body-compact text-muted-foreground">
                        当前收货安排暂不可用：{query.error.message}
                    </p>
                    <Button
                        id={`${idPrefix}-retry`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                        onClick={() => void query.refetch()}
                    >
                        重新读取
                    </Button>
                </div>
            </DetailWorkspaceSection>
        )
    }

    if (!summary || summary.totalLines === 0) return null

    const data = query.data
    const editable = canEdit && !data.blocked_reason
    const openDetailSheet = (
        view: ArrangementDetailViewMode,
        lineId: string | null = null,
    ) => {
        setSheetState({ open: true, view, lineId })
    }

    const isTrivialSingle = Boolean(
        summary.unifiedDestination &&
        summary.totalLines === 1 &&
        summary.totalShippedCount === 0,
    )
    const showDetailAction =
        !isTrivialSingle &&
        (summary.totalAllocationCount > 0 || summary.totalShippedCount > 0)

    const statusBadge =
        summary.overallStatus === "complete" ? (
            <StatusBadge tone="success" appearance="quiet" label="已配齐" />
        ) : summary.overallStatus === "unset" ? (
            <StatusBadge tone="warning" label="待补地址" />
        ) : (
            <StatusBadge
                tone="warning"
                label={`待补 ${summary.pendingLineCount} 项`}
            />
        )

    const subtitle = buildSectionSubtitle(summary)
    const firstLine = summary.lines[0]
    const isSingleLineCompact =
        !summary.unifiedDestination &&
        summary.totalLines === 1 &&
        firstLine != null &&
        firstLine.allocations.length > 0 &&
        firstLine.allocations.length <= INLINE_PREVIEW_LIMIT

    const toggleExpandLine = (lineId: string) => {
        setExpandedLineIds((current) => {
            const next = new Set(current)
            if (next.has(lineId)) {
                next.delete(lineId)
            } else {
                next.add(lineId)
            }
            return next
        })
    }

    return (
        <>
            <DetailWorkspaceSection
                width="form"
                title="当前收货安排"
                headingId="sales-order-arrangement-heading"
                badge={statusBadge}
                subtitle={subtitle}
                actions={
                    showDetailAction || editable ? (
                        <>
                            {editable ? (
                                <Button
                                    id={`${idPrefix}-adjust`}
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className={
                                        detailWorkspaceStyles.subtleButton
                                    }
                                    onClick={() => setEditOpen(true)}
                                >
                                    <MapPinIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    {summary.overallStatus === "complete"
                                        ? "调整安排"
                                        : "补充地址"}
                                </Button>
                            ) : null}
                            {showDetailAction ? (
                                <Button
                                    id={`${idPrefix}-open-all`}
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className={
                                        detailWorkspaceStyles.subtleButton
                                    }
                                    onClick={() =>
                                        openDetailSheet(
                                            summary.uniqueAddressCount > 1
                                                ? "address"
                                                : "product",
                                        )
                                    }
                                >
                                    {summary.uniqueAddressCount > 0 ? (
                                        <>
                                            查看全部地址
                                            <span className="num text-muted-foreground">
                                                ({summary.uniqueAddressCount})
                                            </span>
                                        </>
                                    ) : (
                                        "查看地址明细"
                                    )}
                                    <ChevronRightIcon
                                        data-icon="inline-end"
                                        aria-hidden="true"
                                    />
                                </Button>
                            ) : null}
                        </>
                    ) : undefined
                }
            >
                <ArrangementAlerts data={data} />

                {summary.totalAllocationCount === 0 &&
                summary.totalShippedCount === 0 ? (
                    <UnsetArrangementView
                        idPrefix={idPrefix}
                        summary={summary}
                        editable={editable}
                        onRequestEdit={() => setEditOpen(true)}
                    />
                ) : summary.unifiedDestination ? (
                    <UnifiedDestinationView
                        idPrefix={idPrefix}
                        summary={summary}
                        copiedKey={copiedKey}
                        onCopy={handleCopy}
                        onOpenDetail={() => openDetailSheet("product")}
                    />
                ) : isSingleLineCompact && firstLine ? (
                    <SingleLineCompactView
                        idPrefix={idPrefix}
                        line={firstLine}
                        copiedKey={copiedKey}
                        onCopy={handleCopy}
                    />
                ) : (
                    <MultiLineSummaryTable
                        idPrefix={idPrefix}
                        summary={summary}
                        expandedLineIds={expandedLineIds}
                        copiedKey={copiedKey}
                        editable={editable}
                        onToggleExpand={toggleExpandLine}
                        onCopy={handleCopy}
                        onInspectLine={(lineId) =>
                            openDetailSheet("product", lineId)
                        }
                        onRequestEdit={() => setEditOpen(true)}
                    />
                )}

                <ArrangementFootnote
                    data={data}
                    summary={summary}
                    editable={editable}
                />
            </DetailWorkspaceSection>

            <ArrangementDetailSheet
                open={sheetState.open}
                onOpenChange={(open) =>
                    setSheetState((current) => ({ ...current, open }))
                }
                salesOrderId={salesOrderId}
                data={data}
                summary={summary}
                initialView={sheetState.view}
                initialLineId={sheetState.lineId}
                canEdit={canEdit}
                onRequestEdit={() => setEditOpen(true)}
            />

            {canEdit ? (
                <ArrangementDialog
                    scope="sales-orders"
                    objectId={salesOrderId}
                    open={editOpen}
                    onOpenChange={setEditOpen}
                    idPrefix={`sales-arrangement-inline-${orderSeg}`}
                    unitLabels={unitLabels}
                />
            ) : null}
        </>
    )
}

function buildSectionSubtitle(summary: DerivedArrangementSummary): string {
    const parts: string[] = []
    if (summary.unifiedDestination) {
        parts.push("统一发往 1 个收货地址")
    } else if (summary.uniqueAddressCount > 0) {
        parts.push(`分送 ${summary.uniqueAddressCount} 个收货地址`)
    } else {
        parts.push("暂未填写收货地址")
    }
    parts.push(`共 ${summary.totalLines} 项商品`)
    if (summary.singleUnitTotals) {
        parts.push(
            `合计 ${formatArrangementQuantity(summary.singleUnitTotals.total, summary.singleUnitTotals.unit)}`,
        )
    }
    if (summary.shippedLineCount > 0) {
        parts.push(`${summary.shippedLineCount} 项含已发`)
    }
    return parts.join(" · ")
}

function ArrangementAlerts({ data }: { data: ArrangementView }) {
    if (!data.blocked_reason && !data.source_changed) return null
    return (
        <div className="space-y-2">
            {data.blocked_reason ? (
                <div
                    role="alert"
                    className="flex items-start gap-2 rounded-lg border border-warning-border bg-warning-soft/70 px-3.5 py-2.5 text-body-compact text-warning-soft-foreground"
                >
                    <TriangleAlertIcon
                        aria-hidden="true"
                        className="mt-0.5 size-4 shrink-0"
                    />
                    <span>{data.blocked_reason}</span>
                </div>
            ) : null}
            {data.source_changed ? (
                <div
                    role="status"
                    className="flex items-start gap-2 rounded-lg border border-warning-border bg-warning-soft/70 px-3.5 py-2.5 text-body-compact text-warning-soft-foreground"
                >
                    <TriangleAlertIcon
                        aria-hidden="true"
                        className="mt-0.5 size-4 shrink-0"
                    />
                    <span>销售内容已有更新，请重新核对当前收货安排。</span>
                </div>
            ) : null}
        </div>
    )
}

function ArrangementFootnote({
    data,
    summary,
    editable,
}: {
    data: ArrangementView
    summary: DerivedArrangementSummary
    editable: boolean
}) {
    if (data.registered_packages > 0) {
        return (
            <p className="text-xs text-muted-foreground">
                进行中发货任务已登记{" "}
                <span className="num font-medium text-foreground">
                    {data.registered_packages}
                </span>{" "}
                个包裹；调整未发部分地址后需重新核对受影响包裹，已发包裹保持原地址。
            </p>
        )
    }
    if (summary.totalShippedCount > 0) {
        return (
            <p className="text-xs text-muted-foreground">
                已发包裹保持原地址；未发部分可在履约前继续补充或调整。
            </p>
        )
    }
    if (editable) {
        return (
            <p className="text-xs text-muted-foreground">
                仅调整履约收货安排，不改变销售审批版本；已发包裹保持原地址。
            </p>
        )
    }
    return null
}

/** 模式 1：全单暂未安排地址，以精简待补表清晰呈现缺口。 */
function UnsetArrangementView({
    idPrefix,
    summary,
    editable,
    onRequestEdit,
}: {
    idPrefix: string
    summary: DerivedArrangementSummary
    editable: boolean
    onRequestEdit: () => void
}) {
    return (
        <div className="space-y-3">
            <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-dashed border-border/80 bg-surface-sunken/45 px-4 py-3">
                <div className="flex min-w-0 items-center gap-2 text-body-compact text-muted-foreground">
                    <MapPinIcon
                        aria-hidden="true"
                        className="size-4 shrink-0"
                    />
                    <span>
                        暂未设置收货地址，可暂留待补；有地址的商品可优先安排发货。
                    </span>
                </div>
                {editable ? (
                    <Button
                        id={`${idPrefix}-empty-add`}
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={onRequestEdit}
                    >
                        补充收货地址
                    </Button>
                ) : null}
            </div>

            <div
                data-slot="detail-inline-table"
                className={detailWorkspaceStyles.inlineTableWrapper}
            >
                <Table>
                    <TableHeader>
                        <TableRow>
                            <TableHead>商品</TableHead>
                            <TableHead data-align="end" className="w-36">
                                待补数量
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {summary.lines.map((line) => (
                            <TableRow key={line.lineId}>
                                <TableCell className="whitespace-normal">
                                    <div className="flex flex-wrap items-center gap-1.5 font-medium text-foreground">
                                        <span>{line.itemName}</span>
                                        {line.isGift ? (
                                            <Badge variant="secondary">
                                                赠品
                                            </Badge>
                                        ) : null}
                                    </div>
                                </TableCell>
                                <TableCell
                                    data-align="end"
                                    className="num whitespace-nowrap font-medium text-warning-soft-foreground"
                                >
                                    {formatArrangementQuantity(
                                        line.pendingQuantity,
                                        line.unit,
                                    )}
                                </TableCell>
                            </TableRow>
                        ))}
                    </TableBody>
                </Table>
            </div>
        </div>
    )
}

/** 模式 2：全单统一发往同一收货人，避免逐行重复同一地址。 */
function UnifiedDestinationView({
    idPrefix,
    summary,
    copiedKey,
    onCopy,
    onOpenDetail,
}: {
    idPrefix: string
    summary: DerivedArrangementSummary
    copiedKey: string | null
    onCopy: (
        copyId: string,
        target: { recipient: string; phone: string; fullAddress: string },
    ) => void
    onOpenDetail: () => void
}) {
    const dest = summary.unifiedDestination
    if (!dest) return null
    const isCopied = copiedKey === dest.key
    const visibleItems = dest.items.slice(0, UNIFIED_ITEM_PREVIEW_LIMIT)
    const remainingItems = dest.items.length - visibleItems.length

    return (
        <div className="space-y-3 rounded-lg border border-border/75 bg-surface-sunken/35 px-4 py-3.5">
            <div className="flex items-start justify-between gap-3">
                <div className="min-w-0 flex-1 space-y-1">
                    <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                        <MapPinIcon
                            aria-hidden="true"
                            className="size-3.5 shrink-0 text-primary"
                        />
                        <span className="font-medium text-foreground">
                            {dest.recipient}
                        </span>
                        <span className="num text-muted-foreground">
                            {dest.phone}
                        </span>
                        <span className="rounded-sm bg-info-soft px-1.5 py-0.5 text-tiny font-medium text-info-soft-foreground">
                            全单统一收货
                        </span>
                        {dest.hasShipped ? (
                            <StatusBadge
                                tone="info"
                                appearance="quiet"
                                icon={LockIcon}
                                label="含已发"
                            />
                        ) : null}
                    </div>
                    <p className="pl-5.5 break-words text-body-compact text-foreground/90">
                        {dest.fullAddress}
                    </p>
                </div>

                <Button
                    id={`${idPrefix}-unified-copy`}
                    type="button"
                    variant="ghost"
                    size="xs"
                    className={detailWorkspaceStyles.quietButton}
                    onClick={() => onCopy(dest.key, dest)}
                >
                    {isCopied ? (
                        <>
                            <CheckIcon
                                aria-hidden="true"
                                className="size-3 text-success"
                            />
                            已复制
                        </>
                    ) : (
                        <>
                            <CopyIcon aria-hidden="true" className="size-3" />
                            复制地址
                        </>
                    )}
                </Button>
            </div>

            {dest.items.length > 1 || dest.hasShipped ? (
                <div className="flex flex-wrap items-center gap-1.5 border-t border-border/60 pt-2.5 pl-5.5">
                    {visibleItems.map((item) => (
                        <span
                            key={item.lineId}
                            className="inline-flex max-w-full items-center gap-1.5 rounded-md border border-border/65 bg-card px-2 py-0.5 text-xs text-foreground"
                        >
                            <span className="truncate">{item.itemName}</span>
                            <span className="num shrink-0 font-medium">
                                ×{" "}
                                {formatArrangementQuantity(
                                    item.quantity,
                                    item.unit,
                                )}
                            </span>
                            {item.hasShipped ? (
                                <span className="num shrink-0 text-info-soft-foreground">
                                    （已发{" "}
                                    {formatArrangementQuantity(
                                        item.shippedQuantity,
                                        item.unit,
                                    )}
                                    ）
                                </span>
                            ) : null}
                        </span>
                    ))}
                    {remainingItems > 0 ? (
                        <Button
                            id={`${idPrefix}-unified-more-items`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className={detailWorkspaceStyles.inlineLinkButton}
                            onClick={onOpenDetail}
                        >
                            另有 {remainingItems} 项商品
                            <ChevronRightIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        </Button>
                    ) : null}
                </div>
            ) : null}
        </div>
    )
}

/** 模式 3：单商品少量地址（≤ 5 条），直接展示整齐的分列表格，无需额外展开。 */
function SingleLineCompactView({
    idPrefix,
    line,
    copiedKey,
    onCopy,
}: {
    idPrefix: string
    line: DerivedArrangementLine
    copiedKey: string | null
    onCopy: (
        copyId: string,
        target: { recipient: string; phone: string; fullAddress: string },
    ) => void
}) {
    const linePrefix = `${idPrefix}-${toAutomationIdSegment(line.lineId)}`
    return (
        <div className="space-y-2.5">
            <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex min-w-0 flex-wrap items-center gap-2">
                    <span className="font-medium text-foreground">
                        {line.itemName}
                    </span>
                    {line.isGift ? (
                        <Badge variant="secondary">赠品</Badge>
                    ) : null}
                    <span className="num text-xs text-muted-foreground">
                        已安排{" "}
                        {formatArrangementQuantity(
                            line.allocatedQuantity,
                            line.unit,
                        )}{" "}
                        / {formatArrangementQuantity(line.quantity, line.unit)}
                    </span>
                    {line.hasPending ? (
                        <span className="num text-xs font-medium text-warning-soft-foreground">
                            · 待补{" "}
                            {formatArrangementQuantity(
                                line.pendingQuantity,
                                line.unit,
                            )}
                        </span>
                    ) : null}
                </div>
                <LineProgressBar
                    percent={line.allocatedPercent}
                    complete={line.isComplete}
                />
            </div>

            <InlineAllocationsTable
                idPrefix={linePrefix}
                lineId={line.lineId}
                unit={line.unit}
                rows={line.allocations}
                copiedKey={copiedKey}
                onCopy={onCopy}
            />

            {line.unmatchedShipped.length > 0 ? (
                <UnmatchedShippedStrip
                    unit={line.unit}
                    rows={line.unmatchedShipped}
                />
            ) : null}
        </div>
    )
}

/** 模式 4：多商品或大批量多地址，按商品汇总 + 行内前 5 条快览 + 抽屉全量搜索。 */
function MultiLineSummaryTable({
    idPrefix,
    summary,
    expandedLineIds,
    copiedKey,
    editable,
    onToggleExpand,
    onCopy,
    onInspectLine,
    onRequestEdit,
}: {
    idPrefix: string
    summary: DerivedArrangementSummary
    expandedLineIds: ReadonlySet<string>
    copiedKey: string | null
    editable: boolean
    onToggleExpand: (lineId: string) => void
    onCopy: (
        copyId: string,
        target: { recipient: string; phone: string; fullAddress: string },
    ) => void
    onInspectLine: (lineId: string) => void
    onRequestEdit: () => void
}) {
    // 只有 1 项商品且地址超过 5 条时默认展开首屏 5 条快览；多商品时默认收起保持概览紧凑。
    const singleLargeLine =
        summary.totalLines === 1 ? summary.lines[0]?.lineId : null

    return (
        <div
            data-slot="detail-inline-table"
            className={detailWorkspaceStyles.dataTableWrapper}
        >
            <Table data-density="comfortable" className="min-w-[38rem]">
                <TableHeader>
                    <TableRow>
                        <TableHead>商品</TableHead>
                        <TableHead className="w-40">安排进度</TableHead>
                        <TableHead>收货分布</TableHead>
                        <TableHead data-align="end" className="w-28">
                            地址明细
                        </TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {summary.lines.map((line) => {
                        const lineSeg = toAutomationIdSegment(line.lineId)
                        const linePrefix = `${idPrefix}-${lineSeg}`
                        const hasRows =
                            line.allocations.length > 0 ||
                            line.unmatchedShipped.length > 0
                        const isExpanded =
                            singleLargeLine === line.lineId
                                ? !expandedLineIds.has(line.lineId)
                                : expandedLineIds.has(line.lineId)
                        const previewRows = line.allocations.slice(
                            0,
                            INLINE_PREVIEW_LIMIT,
                        )
                        const hiddenCount =
                            line.allocations.length - previewRows.length
                        const singleRow =
                            line.allocations.length === 1
                                ? line.allocations[0]
                                : undefined

                        return (
                            <React.Fragment key={line.lineId}>
                                <TableRow>
                                    <TableCell className="min-w-44 whitespace-normal align-top">
                                        <div className="flex items-start gap-1.5">
                                            {hasRows ? (
                                                <Button
                                                    id={`${linePrefix}-toggle`}
                                                    type="button"
                                                    variant="ghost"
                                                    size="icon-xs"
                                                    aria-expanded={isExpanded}
                                                    aria-label={`${isExpanded ? "收起" : "展开"}${line.itemName}收货地址预览`}
                                                    className="-ml-1.5 mt-0.5 shrink-0 text-muted-foreground hover:text-foreground aria-expanded:bg-muted/70 aria-expanded:text-foreground"
                                                    onClick={() =>
                                                        onToggleExpand(
                                                            line.lineId,
                                                        )
                                                    }
                                                >
                                                    {isExpanded ? (
                                                        <ChevronDownIcon
                                                            aria-hidden="true"
                                                            className="size-3.5"
                                                        />
                                                    ) : (
                                                        <ChevronRightIcon
                                                            aria-hidden="true"
                                                            className="size-3.5"
                                                        />
                                                    )}
                                                </Button>
                                            ) : (
                                                <span
                                                    aria-hidden="true"
                                                    className="inline-block w-5 shrink-0"
                                                />
                                            )}
                                            <div className="min-w-0 space-y-0.5">
                                                <div className="flex flex-wrap items-center gap-1.5 font-medium text-foreground">
                                                    <span>{line.itemName}</span>
                                                    {line.isGift ? (
                                                        <Badge variant="secondary">
                                                            赠品
                                                        </Badge>
                                                    ) : null}
                                                </div>
                                                {line.hasShipped ? (
                                                    <p className="num inline-flex items-center gap-1 text-xs text-info-soft-foreground">
                                                        <LockIcon
                                                            aria-hidden="true"
                                                            className="size-3"
                                                        />
                                                        已发{" "}
                                                        {formatArrangementQuantity(
                                                            line.shippedQuantity,
                                                            line.unit,
                                                        )}
                                                    </p>
                                                ) : null}
                                            </div>
                                        </div>
                                    </TableCell>

                                    <TableCell className="whitespace-nowrap align-top">
                                        <div className="space-y-1.5">
                                            <LineProgressBar
                                                percent={line.allocatedPercent}
                                                complete={line.isComplete}
                                            />
                                            <div className="num text-xs text-muted-foreground">
                                                <span className="font-medium text-foreground">
                                                    {formatArrangementQuantity(
                                                        line.allocatedQuantity,
                                                        line.unit,
                                                    )}
                                                </span>{" "}
                                                /{" "}
                                                {formatArrangementQuantity(
                                                    line.quantity,
                                                    line.unit,
                                                )}
                                            </div>
                                            {line.hasPending ? (
                                                <div className="num text-xs font-medium text-warning-soft-foreground">
                                                    待补{" "}
                                                    {formatArrangementQuantity(
                                                        line.pendingQuantity,
                                                        line.unit,
                                                    )}
                                                </div>
                                            ) : null}
                                        </div>
                                    </TableCell>

                                    <TableCell className="min-w-48 whitespace-normal align-top">
                                        {line.allocations.length === 0 ? (
                                            <span className="text-xs text-muted-foreground">
                                                暂未设置收货地址
                                            </span>
                                        ) : singleRow ? (
                                            <div className="space-y-0.5">
                                                <div className="flex flex-wrap items-baseline gap-x-2">
                                                    <span className="font-medium text-foreground">
                                                        {singleRow.recipient}
                                                    </span>
                                                    <span className="num text-xs text-muted-foreground">
                                                        {singleRow.phone}
                                                    </span>
                                                </div>
                                                <p
                                                    className="line-clamp-1 text-xs text-muted-foreground"
                                                    title={
                                                        singleRow.fullAddress
                                                    }
                                                >
                                                    {singleRow.fullAddress}
                                                </p>
                                            </div>
                                        ) : (
                                            <div className="space-y-0.5">
                                                <div className="flex flex-wrap items-baseline gap-x-1.5">
                                                    <span className="num font-medium text-foreground">
                                                        {
                                                            line.allocations
                                                                .length
                                                        }{" "}
                                                        个地址
                                                    </span>
                                                    {line.recipientPreview ? (
                                                        <span className="text-xs text-muted-foreground">
                                                            ·{" "}
                                                            {
                                                                line.recipientPreview
                                                            }
                                                        </span>
                                                    ) : null}
                                                </div>
                                                {line.regionPreview ? (
                                                    <p
                                                        className="line-clamp-1 text-xs text-muted-foreground"
                                                        title={line.uniqueRegions.join(
                                                            "、",
                                                        )}
                                                    >
                                                        {line.regionPreview}
                                                    </p>
                                                ) : null}
                                            </div>
                                        )}
                                    </TableCell>

                                    <TableCell
                                        data-align="end"
                                        className="whitespace-nowrap align-top"
                                    >
                                        {hasRows ? (
                                            <Button
                                                id={`${linePrefix}-inspect`}
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                className={
                                                    detailWorkspaceStyles.inlineLinkButton
                                                }
                                                onClick={() =>
                                                    onInspectLine(line.lineId)
                                                }
                                            >
                                                <span className="num">
                                                    {line.allocations.length > 0
                                                        ? `${line.allocations.length} 地址`
                                                        : "查看已发"}
                                                </span>
                                                <ChevronRightIcon
                                                    data-icon="inline-end"
                                                    aria-hidden="true"
                                                />
                                            </Button>
                                        ) : editable ? (
                                            <Button
                                                id={`${linePrefix}-supplement`}
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                className={
                                                    detailWorkspaceStyles.inlineLinkButton
                                                }
                                                onClick={onRequestEdit}
                                            >
                                                去补充
                                            </Button>
                                        ) : (
                                            <span className="text-xs text-muted-foreground">
                                                —
                                            </span>
                                        )}
                                    </TableCell>
                                </TableRow>

                                {isExpanded && hasRows ? (
                                    <TableRow className="bg-surface-sunken/40 hover:bg-surface-sunken/40">
                                        <TableCell
                                            colSpan={4}
                                            className="p-0 whitespace-normal"
                                        >
                                            <div className="space-y-2.5 px-4 py-3">
                                                {previewRows.length > 0 ? (
                                                    <InlineAllocationsTable
                                                        idPrefix={`${linePrefix}-preview`}
                                                        lineId={line.lineId}
                                                        unit={line.unit}
                                                        rows={previewRows}
                                                        copiedKey={copiedKey}
                                                        onCopy={onCopy}
                                                        nested
                                                    />
                                                ) : null}

                                                {line.unmatchedShipped.length >
                                                0 ? (
                                                    <UnmatchedShippedStrip
                                                        unit={line.unit}
                                                        rows={
                                                            line.unmatchedShipped
                                                        }
                                                    />
                                                ) : null}

                                                {hiddenCount > 0 ? (
                                                    <div className="flex flex-wrap items-center justify-between gap-2 pt-0.5 text-xs text-muted-foreground">
                                                        <span className="num">
                                                            已展示前{" "}
                                                            {previewRows.length}{" "}
                                                            个地址（共{" "}
                                                            {
                                                                line.allocations
                                                                    .length
                                                            }{" "}
                                                            个）
                                                        </span>
                                                        <Button
                                                            id={`${linePrefix}-preview-more`}
                                                            type="button"
                                                            variant="ghost"
                                                            size="xs"
                                                            className={
                                                                detailWorkspaceStyles.inlineLinkButton
                                                            }
                                                            onClick={() =>
                                                                onInspectLine(
                                                                    line.lineId,
                                                                )
                                                            }
                                                        >
                                                            查看全部{" "}
                                                            <span className="num">
                                                                {
                                                                    line
                                                                        .allocations
                                                                        .length
                                                                }
                                                            </span>{" "}
                                                            个地址并搜索
                                                            <ChevronRightIcon
                                                                data-icon="inline-end"
                                                                aria-hidden="true"
                                                            />
                                                        </Button>
                                                    </div>
                                                ) : null}
                                            </div>
                                        </TableCell>
                                    </TableRow>
                                ) : null}
                            </React.Fragment>
                        )
                    })}
                </TableBody>
            </Table>
        </div>
    )
}

function InlineAllocationsTable({
    idPrefix,
    lineId,
    unit,
    rows,
    copiedKey,
    onCopy,
    nested = false,
}: {
    idPrefix: string
    lineId: string
    unit: string
    rows: readonly DerivedAllocationRow[]
    copiedKey: string | null
    onCopy: (
        copyId: string,
        target: { recipient: string; phone: string; fullAddress: string },
    ) => void
    nested?: boolean
}) {
    return (
        <div
            data-slot="detail-inline-table"
            className={
                nested
                    ? "overflow-x-auto rounded-md border border-border/70 bg-card"
                    : detailWorkspaceStyles.inlineTableWrapper
            }
        >
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead className="w-28">收货人</TableHead>
                        <TableHead className="w-32">联系电话</TableHead>
                        <TableHead>收货地址</TableHead>
                        <TableHead data-align="end" className="w-24">
                            数量
                        </TableHead>
                        <TableHead className="w-10 text-right">
                            <span className="sr-only">复制</span>
                        </TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {rows.map((row) => {
                        const copyId = `${lineId}:${row.key}`
                        const isCopied = copiedKey === copyId
                        return (
                            <TableRow key={row.key}>
                                <TableCell className="font-medium text-foreground">
                                    {row.recipient}
                                </TableCell>
                                <TableCell className="num whitespace-nowrap text-muted-foreground">
                                    {row.phone}
                                </TableCell>
                                <TableCell className="whitespace-normal text-foreground/90">
                                    <span className="break-words">
                                        {row.fullAddress}
                                    </span>
                                </TableCell>
                                <TableCell
                                    data-align="end"
                                    className="whitespace-nowrap"
                                >
                                    <div className="num font-medium text-foreground">
                                        {formatArrangementQuantity(
                                            row.quantity,
                                            unit,
                                        )}
                                    </div>
                                    {row.hasShipped ? (
                                        <div className="num text-xs text-info-soft-foreground">
                                            已发{" "}
                                            {formatArrangementQuantity(
                                                row.shippedQuantity,
                                                unit,
                                            )}
                                        </div>
                                    ) : null}
                                </TableCell>
                                <TableCell className="text-right">
                                    <Button
                                        id={`${idPrefix}-copy-${row.idSegment}`}
                                        type="button"
                                        variant="ghost"
                                        size="icon-xs"
                                        aria-label={`复制 ${row.recipient} 的收货地址`}
                                        className={
                                            detailWorkspaceStyles.quietButton
                                        }
                                        onClick={() => onCopy(copyId, row)}
                                    >
                                        {isCopied ? (
                                            <CheckIcon
                                                aria-hidden="true"
                                                className="size-3.5 text-success"
                                            />
                                        ) : (
                                            <CopyIcon
                                                aria-hidden="true"
                                                className="size-3.5"
                                            />
                                        )}
                                    </Button>
                                </TableCell>
                            </TableRow>
                        )
                    })}
                </TableBody>
            </Table>
        </div>
    )
}

function UnmatchedShippedStrip({
    unit,
    rows,
}: {
    unit: string
    rows: DerivedArrangementLine["unmatchedShipped"]
}) {
    return (
        <div className="space-y-1 rounded-md border border-border/70 bg-card px-3 py-2 text-xs">
            <p className="inline-flex items-center gap-1 font-medium text-foreground">
                <LockIcon
                    aria-hidden="true"
                    className="size-3 text-muted-foreground"
                />
                已发历史记录（冻结）
            </p>
            <ul className="space-y-0.5 text-muted-foreground">
                {rows.map((row) => (
                    <li
                        key={row.key}
                        className="flex flex-wrap items-baseline justify-between gap-2"
                    >
                        <span className="min-w-0 break-words">
                            {row.recipient}{" "}
                            <span className="num">{row.phone}</span> ·{" "}
                            {row.address}
                        </span>
                        <span className="num shrink-0 font-medium text-foreground">
                            {formatArrangementQuantity(row.quantity, unit)}
                        </span>
                    </li>
                ))}
            </ul>
        </div>
    )
}

function LineProgressBar({
    percent,
    complete,
}: {
    percent: string
    complete: boolean
}) {
    return (
        <div
            aria-hidden="true"
            className="h-1.5 w-24 overflow-hidden rounded-full bg-muted"
        >
            <div
                className={cn(
                    "h-full rounded-full transition-all",
                    complete ? "bg-success" : "bg-warning",
                )}
                style={{ width: `${percent}%` }}
            />
        </div>
    )
}
