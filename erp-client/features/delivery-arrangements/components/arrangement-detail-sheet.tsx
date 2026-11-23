"use client"

import * as React from "react"
import {
    CheckIcon,
    CopyIcon,
    LockIcon,
    MapPinIcon,
    SearchIcon,
    TriangleAlertIcon,
    XIcon,
} from "lucide-react"

import {
    DetailWorkspaceSubTabs,
    OptionCombobox,
    QuickPreviewSheet,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { SheetClose } from "@/components/ui/sheet"
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
import {
    formatArrangementQuantity,
    formatCopyableAddress,
    type DerivedAddressGroup,
    type DerivedArrangementLine,
    type DerivedArrangementSummary,
} from "../lib/arrangement-model"

export type ArrangementDetailViewMode = "product" | "address"

type QuickFilter = "all" | "pending" | "shipped"

export function ArrangementDetailSheet({
    open,
    onOpenChange,
    salesOrderId,
    data,
    summary,
    initialView = "product",
    initialLineId = null,
    canEdit = false,
    onRequestEdit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    salesOrderId: string
    data: ArrangementView
    summary: DerivedArrangementSummary
    initialView?: ArrangementDetailViewMode
    initialLineId?: string | null
    canEdit?: boolean
    onRequestEdit?: () => void
}) {
    const orderSeg = toAutomationIdSegment(salesOrderId)
    const idPrefix = `sales-arrangement-detail-${orderSeg}`

    const [viewMode, setViewMode] =
        React.useState<ArrangementDetailViewMode>(initialView)
    const [selectedLineId, setSelectedLineId] = React.useState<string>(
        initialLineId ?? "all",
    )
    const [search, setSearch] = React.useState("")
    const [quickFilter, setQuickFilter] = React.useState<QuickFilter>("all")
    const [copiedKey, setCopiedKey] = React.useState<string | null>(null)
    const copyTimerRef = React.useRef<number | null>(null)

    React.useEffect(() => {
        if (!open) return
        setViewMode(initialView)
        setSelectedLineId(initialLineId ?? "all")
        setSearch("")
        setQuickFilter("all")
    }, [open, initialView, initialLineId])

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

    const needle = search.trim().toLowerCase()

    const productOptions = React.useMemo(
        () => [
            {
                value: "all",
                label: `全部商品（${summary.totalLines} 项）`,
            },
            ...summary.lines.map((line, index) => ({
                value: line.lineId,
                label: `#${index + 1} ${line.itemName}`,
                description:
                    line.allocations.length === 0
                        ? `待补 ${formatArrangementQuantity(line.pendingQuantity, line.unit)}`
                        : line.hasPending
                          ? `${line.allocations.length} 个地址 · 待补 ${formatArrangementQuantity(line.pendingQuantity, line.unit)}`
                          : `${line.allocations.length} 个地址 · 已配齐`,
            })),
        ],
        [summary.lines, summary.totalLines],
    )

    const filteredLines = React.useMemo(() => {
        const scoped =
            selectedLineId && selectedLineId !== "all"
                ? summary.lines.filter((line) => line.lineId === selectedLineId)
                : summary.lines

        return scoped
            .filter((line) => {
                if (quickFilter === "pending") return line.hasPending
                if (quickFilter === "shipped") return line.hasShipped
                return true
            })
            .map((line) => {
                if (!needle) {
                    return {
                        line,
                        allocations: line.allocations,
                        unmatchedShipped: line.unmatchedShipped,
                        matchedByLineName: false,
                    }
                }
                const lineHit = [line.itemName, line.sku ?? ""].some((field) =>
                    field.toLowerCase().includes(needle),
                )
                const allocations = lineHit
                    ? line.allocations
                    : line.allocations.filter((row) =>
                          [
                              row.recipient,
                              row.phone,
                              row.regionName,
                              row.address,
                              row.fullAddress,
                          ].some((field) =>
                              field.toLowerCase().includes(needle),
                          ),
                      )
                const unmatchedShipped = lineHit
                    ? line.unmatchedShipped
                    : line.unmatchedShipped.filter((row) =>
                          [row.recipient, row.phone, row.address].some(
                              (field) => field.toLowerCase().includes(needle),
                          ),
                      )
                return {
                    line,
                    allocations,
                    unmatchedShipped,
                    matchedByLineName: lineHit,
                }
            })
            .filter(
                (entry) =>
                    !needle ||
                    entry.matchedByLineName ||
                    entry.allocations.length > 0 ||
                    entry.unmatchedShipped.length > 0,
            )
    }, [summary.lines, selectedLineId, quickFilter, needle])

    const filteredAddressGroups = React.useMemo(() => {
        return summary.addressGroups.filter((group) => {
            if (quickFilter === "shipped" && !group.hasShipped) return false
            if (!needle) return true
            return [
                group.recipient,
                group.phone,
                group.regionName,
                group.address,
                group.fullAddress,
                ...group.items.map((item) => item.itemName),
                ...group.items.map((item) => item.sku ?? ""),
            ].some((field) => field.toLowerCase().includes(needle))
        })
    }, [summary.addressGroups, quickFilter, needle])

    const visibleAllocationTotal = filteredLines.reduce(
        (acc, entry) => acc + entry.allocations.length,
        0,
    )
    const hasActiveFilter =
        Boolean(needle) ||
        quickFilter !== "all" ||
        (viewMode === "product" && selectedLineId !== "all")

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

    return (
        <QuickPreviewSheet
            idPrefix={idPrefix}
            open={open}
            onOpenChange={onOpenChange}
            size="detail"
            identity={`当前收货安排 · 共 ${summary.totalLines} 项商品`}
            title="收货地址明细"
            description="展示当前履约收货安排；已发包裹保持原地址，仅未发部分可补充或调整。"
            summary={
                <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
                    {statusBadge}
                    <span className="num text-xs text-muted-foreground">
                        {summary.uniqueAddressCount > 0
                            ? `${summary.uniqueAddressCount} 个收货地址`
                            : "暂未设置地址"}
                        {summary.singleUnitTotals
                            ? ` · 共 ${formatArrangementQuantity(summary.singleUnitTotals.total, summary.singleUnitTotals.unit)}`
                            : ""}
                        {summary.shippedLineCount > 0
                            ? ` · ${summary.shippedLineCount} 项含已发`
                            : ""}
                        {data.registered_packages > 0
                            ? ` · 已登记 ${data.registered_packages} 个包裹`
                            : ""}
                    </span>
                </div>
            }
            footer={
                <>
                    <p className="mr-auto text-xs text-muted-foreground">
                        {canEdit && !data.blocked_reason
                            ? "调整收货安排不影响销售审批版本；价格或数量变化请发起改单。"
                            : "当前为只读查看。"}
                    </p>
                    <SheetClose
                        id={`${idPrefix}-footer-close`}
                        render={
                            <Button type="button" variant="outline" size="sm" />
                        }
                    >
                        关闭
                    </SheetClose>
                    {canEdit && !data.blocked_reason && onRequestEdit ? (
                        <Button
                            id={`${idPrefix}-edit`}
                            type="button"
                            size="sm"
                            onClick={() => {
                                onOpenChange(false)
                                onRequestEdit()
                            }}
                        >
                            <MapPinIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {summary.overallStatus === "complete"
                                ? "调整收货安排"
                                : "补充收货地址"}
                        </Button>
                    ) : null}
                </>
            }
        >
            {/* 固定视图与筛选工具条 */}
            <div className="shrink-0 space-y-2.5 border-b border-border/60 bg-card px-6 py-3">
                <div className="flex flex-wrap items-center justify-between gap-2">
                    <DetailWorkspaceSubTabs
                        value={viewMode}
                        onValueChange={(next) => {
                            const nextMode = next as ArrangementDetailViewMode
                            setViewMode(nextMode)
                            if (
                                nextMode === "address" &&
                                quickFilter === "pending"
                            ) {
                                setQuickFilter("all")
                            }
                        }}
                        ariaLabel="收货安排查看视角"
                        idPrefix={`${idPrefix}-view`}
                        className="pt-0"
                        items={[
                            {
                                id: "product",
                                label: "按商品查看",
                                badge: (
                                    <span className="num rounded-sm bg-muted px-1.5 text-tiny text-muted-foreground">
                                        {summary.totalLines}
                                    </span>
                                ),
                            },
                            {
                                id: "address",
                                label: "按收货人汇总",
                                badge: (
                                    <span className="num rounded-sm bg-muted px-1.5 text-tiny text-muted-foreground">
                                        {summary.uniqueAddressCount}
                                    </span>
                                ),
                            },
                        ]}
                    />

                    {viewMode === "product" && summary.totalLines > 1 ? (
                        <div className="min-w-0 flex-1 sm:max-w-72">
                            <OptionCombobox
                                id={`${idPrefix}-line-filter`}
                                aria-label="筛选商品"
                                size="sm"
                                allowClear={false}
                                options={productOptions}
                                value={selectedLineId}
                                onValueChange={(next) =>
                                    setSelectedLineId(next || "all")
                                }
                                placeholder="按商品筛选…"
                            />
                        </div>
                    ) : null}
                </div>

                <div className="flex flex-wrap items-center gap-2">
                    <div className="relative min-w-52 flex-1">
                        <SearchIcon
                            aria-hidden="true"
                            className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                        />
                        <Input
                            id={`${idPrefix}-search`}
                            value={search}
                            onChange={(event) => setSearch(event.target.value)}
                            placeholder="搜索收货人、电话、地区、地址或商品…"
                            className="h-control-sm pr-7 pl-8"
                        />
                        {search ? (
                            <Button
                                id={`${idPrefix}-search-clear`}
                                type="button"
                                variant="ghost"
                                size="icon-xs"
                                aria-label="清空搜索"
                                className="absolute top-1/2 right-1 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                                onClick={() => setSearch("")}
                            >
                                <XIcon className="size-3" aria-hidden="true" />
                            </Button>
                        ) : null}
                    </div>

                    {viewMode === "product" && summary.pendingLineCount > 0 ? (
                        <Button
                            id={`${idPrefix}-filter-pending`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            aria-pressed={quickFilter === "pending"}
                            className={cn(
                                detailWorkspaceStyles.subtleButton,
                                quickFilter === "pending" &&
                                    "border-primary/25 bg-sidebar-accent/70 text-sidebar-accent-foreground hover:bg-sidebar-accent",
                            )}
                            onClick={() =>
                                setQuickFilter((current) =>
                                    current === "pending" ? "all" : "pending",
                                )
                            }
                        >
                            待补地址
                            <span className="num">
                                ({summary.pendingLineCount})
                            </span>
                        </Button>
                    ) : null}

                    {summary.shippedLineCount > 0 ? (
                        <Button
                            id={`${idPrefix}-filter-shipped`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            aria-pressed={quickFilter === "shipped"}
                            className={cn(
                                detailWorkspaceStyles.subtleButton,
                                quickFilter === "shipped" &&
                                    "border-primary/25 bg-sidebar-accent/70 text-sidebar-accent-foreground hover:bg-sidebar-accent",
                            )}
                            onClick={() =>
                                setQuickFilter((current) =>
                                    current === "shipped" ? "all" : "shipped",
                                )
                            }
                        >
                            含已发
                        </Button>
                    ) : null}

                    <span className="num ml-auto text-xs text-muted-foreground">
                        {viewMode === "product"
                            ? `${filteredLines.length} 项商品 · ${visibleAllocationTotal} 条分配`
                            : `${filteredAddressGroups.length} 个收货地址`}
                    </span>
                </div>
            </div>

            {/* 滚动正文区 */}
            <div className="min-h-0 flex-1 space-y-5 overflow-y-auto px-6 py-5">
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

                {viewMode === "product" ? (
                    filteredLines.length === 0 ? (
                        <EmptyFilterNotice
                            idPrefix={idPrefix}
                            hasActiveFilter={hasActiveFilter}
                            onReset={() => {
                                setSearch("")
                                setQuickFilter("all")
                                setSelectedLineId("all")
                            }}
                        />
                    ) : (
                        <div className="space-y-6">
                            {filteredLines.map(
                                ({ line, allocations, unmatchedShipped }) => (
                                    <ProductDetailBlock
                                        key={line.lineId}
                                        idPrefix={`${idPrefix}-line-${toAutomationIdSegment(line.lineId)}`}
                                        line={line}
                                        allocations={allocations}
                                        unmatchedShipped={unmatchedShipped}
                                        copiedKey={copiedKey}
                                        onCopy={handleCopy}
                                    />
                                ),
                            )}
                        </div>
                    )
                ) : (
                    <div className="space-y-4">
                        {summary.pendingLineCount > 0 ? (
                            <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-warning-border/75 bg-warning-soft/55 px-3.5 py-2.5 text-body-compact text-warning-soft-foreground">
                                <span>
                                    另有{" "}
                                    <span className="num font-semibold">
                                        {summary.pendingLineCount}
                                    </span>{" "}
                                    项商品尚未配齐收货地址
                                </span>
                                <Button
                                    id={`${idPrefix}-jump-pending-lines`}
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    className={
                                        detailWorkspaceStyles.inlineLinkButton
                                    }
                                    onClick={() => {
                                        setViewMode("product")
                                        setSelectedLineId("all")
                                        setQuickFilter("pending")
                                    }}
                                >
                                    查看待补商品
                                </Button>
                            </div>
                        ) : null}

                        {filteredAddressGroups.length === 0 ? (
                            <EmptyFilterNotice
                                idPrefix={idPrefix}
                                hasActiveFilter={hasActiveFilter}
                                emptyTitle="当前订单暂未设置收货地址"
                                onReset={() => {
                                    setSearch("")
                                    setQuickFilter("all")
                                }}
                            />
                        ) : (
                            <div className="divide-y divide-border/60 border-y border-border/70">
                                {filteredAddressGroups.map((group) => (
                                    <AddressGroupRow
                                        key={group.key}
                                        idPrefix={`${idPrefix}-group-${group.idSegment}`}
                                        group={group}
                                        copied={copiedKey === group.key}
                                        onCopy={() =>
                                            handleCopy(group.key, group)
                                        }
                                    />
                                ))}
                            </div>
                        )}
                    </div>
                )}
            </div>
        </QuickPreviewSheet>
    )
}

function ProductDetailBlock({
    idPrefix,
    line,
    allocations,
    unmatchedShipped,
    copiedKey,
    onCopy,
}: {
    idPrefix: string
    line: DerivedArrangementLine
    allocations: DerivedArrangementLine["allocations"]
    unmatchedShipped: DerivedArrangementLine["unmatchedShipped"]
    copiedKey: string | null
    onCopy: (
        copyId: string,
        target: { recipient: string; phone: string; fullAddress: string },
    ) => void
}) {
    return (
        <section className="space-y-3" aria-label={line.itemName}>
            <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2">
                <div className="min-w-0 flex-1 space-y-1">
                    <div className="flex flex-wrap items-center gap-2">
                        <h3 className="font-semibold text-foreground">
                            {line.itemName}
                        </h3>
                        {line.isGift ? (
                            <Badge variant="secondary">赠品</Badge>
                        ) : null}
                        {line.isComplete ? (
                            <StatusBadge
                                tone="success"
                                appearance="quiet"
                                label="已配齐"
                            />
                        ) : line.allocations.length === 0 ? (
                            <StatusBadge tone="warning" label="待补地址" />
                        ) : (
                            <StatusBadge
                                tone="warning"
                                label={`待补 ${formatArrangementQuantity(line.pendingQuantity, line.unit)}`}
                            />
                        )}
                    </div>
                    <div className="num flex flex-wrap items-center gap-x-2.5 gap-y-0.5 text-xs text-muted-foreground">
                        {line.sku ? <span>{line.sku}</span> : null}
                        <span>
                            已安排{" "}
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
                        </span>
                        <span>{line.allocations.length} 个地址</span>
                        {line.hasShipped ? (
                            <span className="inline-flex items-center gap-1 text-info-soft-foreground">
                                <LockIcon
                                    aria-hidden="true"
                                    className="size-3"
                                />
                                已发{" "}
                                {formatArrangementQuantity(
                                    line.shippedQuantity,
                                    line.unit,
                                )}
                            </span>
                        ) : null}
                    </div>
                </div>
                <div className="flex shrink-0 items-center gap-2 pt-1">
                    <div
                        aria-hidden="true"
                        className="h-1.5 w-20 overflow-hidden rounded-full bg-muted"
                    >
                        <div
                            className={cn(
                                "h-full rounded-full transition-all",
                                line.isComplete ? "bg-success" : "bg-warning",
                            )}
                            style={{ width: `${line.allocatedPercent}%` }}
                        />
                    </div>
                    <span className="num text-xs text-muted-foreground">
                        {line.allocatedPercent}%
                    </span>
                </div>
            </div>

            {allocations.length === 0 ? (
                <div className="rounded-lg border border-dashed border-border/80 bg-surface-sunken/45 px-4 py-4 text-xs text-muted-foreground">
                    {line.allocations.length === 0 ? (
                        <>
                            暂未设置收货地址，待补{" "}
                            <span className="num font-medium text-warning-soft-foreground">
                                {formatArrangementQuantity(
                                    line.pendingQuantity,
                                    line.unit,
                                )}
                            </span>
                            ；补齐后即可安排发货。
                        </>
                    ) : (
                        "当前搜索条件下该商品无匹配地址。"
                    )}
                </div>
            ) : (
                <div
                    data-slot="detail-inline-table"
                    className={detailWorkspaceStyles.inlineTableWrapper}
                >
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead className="w-40">
                                    收货人 / 电话
                                </TableHead>
                                <TableHead>收货地址</TableHead>
                                <TableHead data-align="end" className="w-28">
                                    分配数量
                                </TableHead>
                                <TableHead className="w-12 text-right">
                                    <span className="sr-only">复制</span>
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {allocations.map((row) => {
                                const copyId = `${line.lineId}:${row.key}`
                                const isCopied = copiedKey === copyId
                                return (
                                    <TableRow key={row.key}>
                                        <TableCell className="whitespace-normal align-top">
                                            <div className="font-medium text-foreground">
                                                {row.recipient}
                                            </div>
                                            <div className="num text-xs text-muted-foreground">
                                                {row.phone}
                                            </div>
                                        </TableCell>
                                        <TableCell className="whitespace-normal align-top">
                                            <span className="break-words text-foreground/90">
                                                {row.fullAddress}
                                            </span>
                                        </TableCell>
                                        <TableCell
                                            data-align="end"
                                            className="whitespace-nowrap align-top"
                                        >
                                            <div className="num font-medium text-foreground">
                                                {formatArrangementQuantity(
                                                    row.quantity,
                                                    line.unit,
                                                )}
                                            </div>
                                            {row.hasShipped ? (
                                                <div className="num mt-0.5 inline-flex items-center gap-1 text-xs text-info-soft-foreground">
                                                    <LockIcon
                                                        aria-hidden="true"
                                                        className="size-3"
                                                    />
                                                    已发{" "}
                                                    {formatArrangementQuantity(
                                                        row.shippedQuantity,
                                                        line.unit,
                                                    )}
                                                </div>
                                            ) : null}
                                        </TableCell>
                                        <TableCell className="text-right align-top">
                                            <Button
                                                id={`${idPrefix}-copy-${row.idSegment}`}
                                                type="button"
                                                variant="ghost"
                                                size="icon-xs"
                                                aria-label={`复制 ${row.recipient} 的收货地址`}
                                                className={
                                                    detailWorkspaceStyles.quietButton
                                                }
                                                onClick={() =>
                                                    onCopy(copyId, row)
                                                }
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
            )}

            {unmatchedShipped.length > 0 ? (
                <div className="space-y-1.5 rounded-lg border border-border/70 bg-surface-sunken/45 px-3.5 py-2.5 text-xs">
                    <p className="inline-flex items-center gap-1.5 font-medium text-foreground">
                        <LockIcon
                            aria-hidden="true"
                            className="size-3.5 text-muted-foreground"
                        />
                        已发记录（冻结，原地址不变）
                    </p>
                    <ul className="space-y-1 text-muted-foreground">
                        {unmatchedShipped.map((row) => (
                            <li
                                key={row.key}
                                className="flex flex-wrap items-baseline justify-between gap-2"
                            >
                                <span className="min-w-0 break-words">
                                    <span className="font-medium text-foreground/85">
                                        {row.recipient}
                                    </span>{" "}
                                    <span className="num">{row.phone}</span> ·{" "}
                                    {row.address}
                                </span>
                                <span className="num shrink-0 font-medium text-foreground">
                                    {formatArrangementQuantity(
                                        row.quantity,
                                        line.unit,
                                    )}
                                </span>
                            </li>
                        ))}
                    </ul>
                </div>
            ) : null}
        </section>
    )
}

function AddressGroupRow({
    idPrefix,
    group,
    copied,
    onCopy,
}: {
    idPrefix: string
    group: DerivedAddressGroup
    copied: boolean
    onCopy: () => void
}) {
    return (
        <div className="space-y-2.5 py-3.5">
            <div className="flex items-start justify-between gap-3">
                <div className="min-w-0 flex-1 space-y-1">
                    <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                        <MapPinIcon
                            aria-hidden="true"
                            className="size-3.5 shrink-0 text-muted-foreground"
                        />
                        <span className="font-medium text-foreground">
                            {group.recipient}
                        </span>
                        <span className="num text-xs text-muted-foreground">
                            {group.phone}
                        </span>
                        {group.lineCount > 1 ? (
                            <span className="num rounded-sm bg-info-soft px-1.5 py-0.5 text-tiny font-medium text-info-soft-foreground">
                                {group.lineCount} 项商品
                            </span>
                        ) : null}
                        {group.hasShipped ? (
                            <StatusBadge
                                tone="info"
                                appearance="quiet"
                                icon={LockIcon}
                                label="含已发"
                            />
                        ) : null}
                    </div>
                    <p className="pl-5.5 break-words text-xs text-muted-foreground">
                        {group.fullAddress}
                    </p>
                </div>

                <Button
                    id={`${idPrefix}-copy`}
                    type="button"
                    variant="ghost"
                    size="xs"
                    className={detailWorkspaceStyles.quietButton}
                    onClick={onCopy}
                >
                    {copied ? (
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
                            复制
                        </>
                    )}
                </Button>
            </div>

            <div className="flex flex-wrap gap-1.5 pl-5.5">
                {group.items.map((item) => (
                    <span
                        key={item.lineId}
                        className="inline-flex max-w-full items-center gap-1.5 rounded-md border border-border/70 bg-surface-sunken/60 px-2 py-0.5 text-xs text-foreground"
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
            </div>
        </div>
    )
}

function EmptyFilterNotice({
    idPrefix,
    hasActiveFilter,
    emptyTitle = "未找到匹配的收货地址",
    onReset,
}: {
    idPrefix: string
    hasActiveFilter: boolean
    emptyTitle?: string
    onReset: () => void
}) {
    return (
        <div className="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-border/80 bg-surface-sunken/45 px-6 py-10 text-center">
            <MapPinIcon
                aria-hidden="true"
                className="size-5 text-muted-foreground"
            />
            <p className="font-medium text-foreground">{emptyTitle}</p>
            {hasActiveFilter ? (
                <>
                    <p className="text-xs text-muted-foreground">
                        可调整关键词或清空筛选条件后重新查看。
                    </p>
                    <Button
                        id={`${idPrefix}-reset-filter`}
                        type="button"
                        variant="outline"
                        size="sm"
                        className="mt-1"
                        onClick={onReset}
                    >
                        重置筛选
                    </Button>
                </>
            ) : null}
        </div>
    )
}
