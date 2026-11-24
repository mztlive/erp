"use client"

import { useMemo, useState } from "react"
import {
    ArrowRightLeftIcon,
    CheckCircle2Icon,
    ChevronDownIcon,
    ChevronLeftIcon,
    ChevronRightIcon,
    CircleAlertIcon,
    EraserIcon,
    MapPinIcon,
    Maximize2Icon,
    Minimize2Icon,
    PackageCheckIcon,
    PlusIcon,
    ScissorsIcon,
    SearchIcon,
    SplitIcon,
    TriangleAlertIcon,
    WarehouseIcon,
    XIcon,
    ZapIcon,
} from "lucide-react"
import { QuantityValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox, NativeCheckbox } from "@/components/ui/checkbox"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import {
    Popover,
    PopoverContent,
    PopoverTrigger,
} from "@/components/ui/popover"
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compactFixed, compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { PurchaseOrderCreateFormApi } from "../lib/purchase-order-create-form-types"
import { sourcingQuantityStep } from "../lib/sourcing-quantity"
import {
    buildBatchAssignPatches,
    buildClearAddressesPatches,
    buildSetSliceQuantityPatch,
    buildTransferAddressesToNonDirectPatches,
    buildTrimDirectExcessPatches,
    computeGlobalAddressState,
    computeQuotaAutoAllocation,
    entryRegionFacetKey,
    isPositiveQuantity,
    type DirectSourceInfo,
    type GlobalAddressEntry,
    type LineUpdatePatch,
    type NonDirectSourceInfo,
} from "../lib/sourcing/address-grouping"
import { deliveryKey } from "../lib/sourcing/delivery"
import {
    addressDeliverable,
    deliveryTermLabel,
} from "../lib/sourcing/delivery-terms"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import { sourceAccentForIndex } from "../lib/sourcing/source-palette"
import type { PurchaseDeliveryAllocation } from "../types"

type StatusFilter = "all" | "assigned" | "unassigned" | "issues"
type PageSizeOption = 25 | 50 | 100

const TRANSFER_VALUE_PREFIX = "__transfer__:"

function addressIdSegment(row: PurchaseDeliveryAllocation): string {
    return toAutomationIdSegment(encodeURIComponent(deliveryKey(row)))
}

function safeSumQuantities(values: readonly string[]): string {
    if (values.length === 0) return "0"
    try {
        return compactFixed(sumFixed(values, { maxScale: 6, outputScale: 6 }))
    } catch {
        return "0"
    }
}

/**
 * 混合履约感知的全局交付地址分配工作区：
 * - 与上方多源配量区同屏实时联动，支持 100+ 地址按省/市/状态/来源快速筛选与分页；
 * - 清晰区分「直发已分 / 直发缺口 / 直发超额 / 仓库或库存统筹」四种语义状态；
 * - 支持在多个直发来源之间一键改派、按量跨源调拨，并支持将选中地址原子转交给仓库/库存来源（释放直发同时增加仓库数量）。
 */
export function SourcingAddressEditor({
    form,
    row,
    canSplitNewSource,
    onSplitResidual,
    onSplitAddressesToNewSource,
    sourceFilter: controlledSourceFilter,
    onSourceFilterChange,
    isMaximized = false,
    onToggleMaximize,
}: {
    form: PurchaseOrderCreateFormApi
    row: SourcingEditorRow
    canSplitNewSource: boolean
    onSplitResidual?: () => void
    onSplitAddressesToNewSource?: (addressKeys: readonly string[]) => void
    sourceFilter?: string | null
    onSourceFilterChange?: (next: string | null) => void
    isMaximized?: boolean
    onToggleMaximize?: () => void
}) {
    const { product } = row
    const allLines = form.state.values.lines
    const idPrefix = `sourcing-addr-${toAutomationIdSegment(product.salesOrderLineId)}`

    const [statusFilter, setStatusFilter] = useState<StatusFilter>("all")
    const [internalSourceFilter, setInternalSourceFilter] = useState<
        string | null
    >(null)
    const [regionFilter, setRegionFilter] = useState<string | null>(null)
    const [query, setQuery] = useState("")
    const [selectedKeys, setSelectedKeys] = useState<ReadonlySet<string>>(
        () => new Set(),
    )
    const [pageIndex, setPageIndex] = useState(0)
    const [pageSize, setPageSize] = useState<PageSizeOption>(25)

    // 「按数量自动分配」弹窗状态
    const [quotaOpen, setQuotaOpen] = useState(false)
    const [quotaTargetRowKey, setQuotaTargetRowKey] = useState<string>("")
    const [quotaQty, setQuotaQty] = useState("")
    const [quotaPreferFree, setQuotaPreferFree] = useState(true)
    const [quotaAllowReallocate, setQuotaAllowReallocate] = useState(true)
    const [quotaScopeOnlyFiltered, setQuotaScopeOnlyFiltered] = useState(false)
    const [actionFeedback, setActionFeedback] = useState<string | null>(null)

    const sourceFilter =
        controlledSourceFilter !== undefined
            ? controlledSourceFilter
            : internalSourceFilter
    const setSourceFilter = (next: string | null) => {
        if (onSourceFilterChange) onSourceFilterChange(next)
        else setInternalSourceFilter(next)
    }

    const state = useMemo(
        () => computeGlobalAddressState(product, row.allocations),
        [product, row.allocations],
    )

    const activeSourceFilter =
        sourceFilter &&
        state.directSources.some((src) => src.rowKey === sourceFilter)
            ? sourceFilter
            : null
    const activeRegionFilter =
        regionFilter &&
        state.regionFacets.items.some((item) => item.key === regionFilter)
            ? regionFilter
            : null

    const showLeftRail =
        state.regionFacets.level !== "none" ||
        state.directSources.length > 1 ||
        state.nonDirectSources.length > 0 ||
        state.entries.length >= 15

    const filteredEntries = useMemo(() => {
        const keyword = query.trim().toLocaleLowerCase()
        return state.entries.filter((entry) => {
            if (
                statusFilter === "assigned" &&
                !isPositiveQuantity(entry.assignedTotal)
            ) {
                return false
            }
            if (
                statusFilter === "unassigned" &&
                !isPositiveQuantity(entry.remainingUnassigned)
            ) {
                return false
            }
            if (
                statusFilter === "issues" &&
                !entry.hasError &&
                !entry.hasNotDeliverable &&
                !entry.hasFreightExtra
            ) {
                return false
            }
            if (
                activeSourceFilter &&
                !entry.activeSlices.some(
                    (slice) => slice.rowKey === activeSourceFilter,
                )
            ) {
                return false
            }
            if (
                activeRegionFilter &&
                entryRegionFacetKey(entry, state.regionFacets.level) !==
                    activeRegionFilter
            ) {
                return false
            }
            if (!keyword) return true
            return `${entry.baseRow.recipient} ${entry.baseRow.phone} ${entry.fullAddress} ${entry.region.province}`
                .toLocaleLowerCase()
                .includes(keyword)
        })
    }, [
        activeRegionFilter,
        activeSourceFilter,
        query,
        state.entries,
        state.regionFacets.level,
        statusFilter,
    ])

    const totalPages = Math.max(1, Math.ceil(filteredEntries.length / pageSize))
    const safePageIndex = Math.min(pageIndex, totalPages - 1)
    const pageStart = safePageIndex * pageSize
    const pageEntries = filteredEntries.slice(pageStart, pageStart + pageSize)

    const validKeySet = useMemo(
        () => new Set(state.entries.map((entry) => entry.key)),
        [state.entries],
    )
    const effectiveSelectedKeys = useMemo(() => {
        const next = new Set<string>()
        for (const key of selectedKeys) {
            if (validKeySet.has(key)) next.add(key)
        }
        return next
    }, [selectedKeys, validKeySet])

    const selectedEntries = useMemo(
        () =>
            state.entries.filter((entry) =>
                effectiveSelectedKeys.has(entry.key),
            ),
        [effectiveSelectedKeys, state.entries],
    )
    const selectedDemandTotal = useMemo(
        () =>
            safeSumQuantities(selectedEntries.map((entry) => entry.demandQty)),
        [selectedEntries],
    )

    const pageSelectedCount = pageEntries.filter((entry) =>
        effectiveSelectedKeys.has(entry.key),
    ).length
    const allPageSelected =
        pageEntries.length > 0 && pageSelectedCount === pageEntries.length
    const somePageSelected =
        pageSelectedCount > 0 && pageSelectedCount < pageEntries.length
    const allFilteredSelected =
        filteredEntries.length > 0 &&
        filteredEntries.every((entry) => effectiveSelectedKeys.has(entry.key))

    const filteredKeySet = useMemo(
        () => new Set(filteredEntries.map((entry) => entry.key)),
        [filteredEntries],
    )

    const hasActiveFilters =
        statusFilter !== "all" ||
        activeSourceFilter !== null ||
        activeRegionFilter !== null ||
        query.trim().length > 0

    const applyPatches = (patches: readonly LineUpdatePatch[]) => {
        for (const patch of patches) {
            form.setFieldValue(`lines[${patch.index}]`, patch.nextLine)
        }
    }

    const resetPage = () => setPageIndex(0)

    const toggleRowSelection = (key: string, checked: boolean) => {
        setSelectedKeys((current) => {
            const next = new Set(current)
            if (checked) next.add(key)
            else next.delete(key)
            return next
        })
    }

    const togglePageSelection = (checked: boolean) => {
        setSelectedKeys((current) => {
            const next = new Set(current)
            for (const entry of pageEntries) {
                if (checked) next.add(entry.key)
                else next.delete(entry.key)
            }
            return next
        })
    }

    const selectAllFiltered = () => {
        setSelectedKeys(new Set(filteredEntries.map((entry) => entry.key)))
    }

    const clearSelection = () => {
        setSelectedKeys(new Set())
    }

    const handleBatchAssign = (
        targetRowKey: string,
        keys: ReadonlySet<string>,
        mode: "move_full" | "fill_remaining",
    ) => {
        const result = buildBatchAssignPatches(
            product,
            form.state.values.lines,
            targetRowKey,
            keys,
            mode,
        )
        applyPatches(result.patches)
        const target = state.directSources.find(
            (src) => src.rowKey === targetRowKey,
        )
        const skippedNote =
            result.skippedUndeliverableCount > 0
                ? `，已跳过 ${result.skippedUndeliverableCount} 处不可配送地址`
                : ""
        setActionFeedback(
            `已将 ${result.assignedCount} 处地址分配给 ${target?.shortLabel ?? "所选来源"}${skippedNote}。`,
        )
        clearSelection()
    }

    const handleBatchTransferToNonDirect = (
        targetNonDirectRowKey: string,
        keys: ReadonlySet<string>,
    ) => {
        const result = buildTransferAddressesToNonDirectPatches(
            product,
            form.state.values.lines,
            targetNonDirectRowKey,
            keys,
        )
        applyPatches(result.patches)
        const target = state.nonDirectSources.find(
            (src) => src.rowKey === targetNonDirectRowKey,
        )
        const capNote = result.cappedByMax
            ? "（已按该来源上限/商品剩余需求自动封顶）"
            : ""
        setActionFeedback(
            `已将 ${result.releasedCount} 处地址从直发释放，并向 ${target?.shortLabel ?? "仓库来源"} 划入 ${result.transferredQty} ${product.unit}${capNote}。`,
        )
        clearSelection()
    }

    const handleBatchClear = (keys: ReadonlySet<string>) => {
        const patches = buildClearAddressesPatches(
            product,
            form.state.values.lines,
            keys,
        )
        applyPatches(patches)
        setActionFeedback(`已清空 ${keys.size} 处地址的直发分配。`)
        clearSelection()
    }

    const handleTrimDirectExcess = () => {
        if (!isPositiveQuantity(state.directOverTargetQty)) return
        const res = buildTrimDirectExcessPatches(
            product,
            form.state.values.lines,
            state.directOverTargetQty,
        )
        applyPatches(res.patches)
        setActionFeedback(
            `已从直发自动缩减 ${res.trimmedQty} ${product.unit}（释放 ${res.releasedAddressCount} 处地址，由仓库/库存统筹发货）。`,
        )
    }

    const resolvedQuotaTarget =
        (quotaTargetRowKey &&
        state.directSources.some((src) => src.rowKey === quotaTargetRowKey)
            ? quotaTargetRowKey
            : state.primaryDirectSource?.rowKey) ?? ""

    const quotaPreview = useMemo(() => {
        if (!quotaOpen || !resolvedQuotaTarget) return null
        return computeQuotaAutoAllocation(
            product,
            allLines,
            resolvedQuotaTarget,
            quotaQty,
            {
                scopeAddressKeys:
                    quotaScopeOnlyFiltered && hasActiveFilters
                        ? filteredKeySet
                        : undefined,
                preferFreightIncluded: quotaPreferFree,
                allowReallocateFromOtherDirect:
                    state.directSources.length > 1 && quotaAllowReallocate,
            },
        )
    }, [
        allLines,
        filteredKeySet,
        hasActiveFilters,
        product,
        quotaAllowReallocate,
        quotaOpen,
        quotaPreferFree,
        quotaQty,
        quotaScopeOnlyFiltered,
        resolvedQuotaTarget,
        state.directSources.length,
    ])

    const notDeliverableEntries = useMemo(
        () => state.entries.filter((entry) => entry.hasNotDeliverable),
        [state.entries],
    )
    const notDeliverableKeys = useMemo(
        () => notDeliverableEntries.map((entry) => entry.key),
        [notDeliverableEntries],
    )
    const allExceptionKeys = useMemo(
        () =>
            state.entries
                .filter(
                    (entry) => entry.hasNotDeliverable || entry.hasFreightExtra,
                )
                .map((entry) => entry.key),
        [state.entries],
    )

    // 若已存在另一个能覆盖不配送地址的直发来源，提供一键改派快捷按钮
    const rescueDirectSource = useMemo(() => {
        if (
            notDeliverableEntries.length === 0 ||
            state.directSources.length < 2
        )
            return undefined
        return state.directSources.find((src) =>
            notDeliverableEntries.some((entry) =>
                addressDeliverable(src.option, entry.baseRow.region_code),
            ),
        )
    }, [notDeliverableEntries, state.directSources])

    const primaryNonDirectSource =
        state.nonDirectSources.find((src) => src.selected) ??
        state.nonDirectSources[0]

    const defaultSource = state.primaryDirectSource
    const activeRegionLabel = activeRegionFilter
        ? state.regionFacets.items.find((i) => i.key === activeRegionFilter)
              ?.label
        : null
    const activeSourceLabel = activeSourceFilter
        ? state.directSources.find((s) => s.rowKey === activeSourceFilter)
              ?.shortLabel
        : null

    const unassignedLabel = state.hasNonDirectSources
        ? "仓库统筹 / 未直发"
        : "待分配"
    const hasDirectOverTarget = isPositiveQuantity(state.directOverTargetQty)
    const hasDirectShortfall = isPositiveQuantity(state.directShortfallQty)

    const statusBadge = (() => {
        if (state.counts.issues > 0) {
            return {
                tone: "destructive" as const,
                text: `${state.counts.issues} 处异常`,
            }
        }
        if (hasDirectOverTarget) {
            return {
                tone: "destructive" as const,
                text: `直发超出 ${state.directOverTargetQty} ${product.unit}`,
            }
        }
        if (hasDirectShortfall) {
            return {
                tone: "warning" as const,
                text: state.hasNonDirectSources
                    ? `直发尚缺 ${state.directShortfallQty} ${product.unit}`
                    : `待分 ${state.counts.unassigned} 处`,
            }
        }
        if (state.hasNonDirectSources) {
            return {
                tone: "success" as const,
                text: `直发已齐 ${state.directAssignedTotalQty}/${state.directTargetQty} ${product.unit}`,
            }
        }
        return {
            tone: "success" as const,
            text: `已全分 ${state.counts.total} 处`,
        }
    })()

    return (
        <section
            className="flex min-h-0 flex-1 flex-col overflow-hidden bg-card"
            aria-label="本来源交付地址"
        >
            {/* 地址工作区标题条：混合履约状态一目了然 + 展开专注按钮 */}
            <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/70 bg-sidebar px-4 py-2 sm:px-6">
                <div className="flex min-w-0 flex-wrap items-center gap-2">
                    <h3 className="flex items-center gap-1.5 text-body-compact font-semibold text-foreground">
                        <MapPinIcon
                            aria-hidden="true"
                            className="size-3.5 text-primary"
                        />
                        交付地址分配
                    </h3>
                    <span
                        className={cn(
                            "num inline-flex items-center gap-1 rounded-sm px-1.5 py-0.5 text-tiny font-medium",
                            statusBadge.tone === "destructive"
                                ? "bg-destructive-soft text-destructive-soft-foreground"
                                : statusBadge.tone === "warning"
                                  ? "bg-warning-soft text-warning-soft-foreground"
                                  : "bg-success-soft text-success-soft-foreground",
                        )}
                    >
                        {statusBadge.tone === "success" ? (
                            <CheckCircle2Icon
                                aria-hidden="true"
                                className="size-3"
                            />
                        ) : null}
                        {statusBadge.text}
                    </span>
                    {state.hasNonDirectSources ? (
                        <span className="text-xs text-muted-foreground">
                            仓库/库存统筹{" "}
                            <strong className="num font-medium text-foreground">
                                {state.nonDirectTotalQty} {product.unit}
                            </strong>
                            （出库时安排地址）· 需指定直发{" "}
                            <strong className="num font-medium text-foreground">
                                {state.directTargetQty} {product.unit}
                            </strong>
                        </span>
                    ) : (
                        <span className="num text-xs text-muted-foreground">
                            共 {state.counts.total} 处地址 · 已分直发{" "}
                            {state.directAssignedTotalQty} {product.unit}
                        </span>
                    )}
                </div>

                <div className="flex shrink-0 items-center gap-1.5">
                    {hasDirectOverTarget ? (
                        <Button
                            id={`${idPrefix}-header-trim-excess`}
                            type="button"
                            size="xs"
                            variant="destructive"
                            onClick={handleTrimDirectExcess}
                        >
                            <ScissorsIcon aria-hidden="true" />
                            自动缩减多余 {state.directOverTargetQty}{" "}
                            {product.unit}
                        </Button>
                    ) : null}
                    {onToggleMaximize ? (
                        <Button
                            id={`${idPrefix}-toggle-maximize`}
                            type="button"
                            size="xs"
                            variant="ghost"
                            onClick={onToggleMaximize}
                            aria-pressed={isMaximized}
                        >
                            {isMaximized ? (
                                <>
                                    <Minimize2Icon aria-hidden="true" />
                                    展开来源明细
                                </>
                            ) : (
                                <>
                                    <Maximize2Icon aria-hidden="true" />
                                    专注地址区
                                </>
                            )}
                        </Button>
                    ) : null}
                </div>
            </div>

            {/* 配送异常一键处理条 */}
            {state.counts.notDeliverable > 0 ||
            state.counts.freightExtra > 0 ? (
                <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-warning-border/80 bg-warning-soft/65 px-4 py-1.5 text-xs text-warning-soft-foreground sm:px-6">
                    <div className="flex flex-wrap items-center gap-2">
                        <TriangleAlertIcon
                            aria-hidden="true"
                            className="size-3.5 shrink-0"
                        />
                        <span>
                            配送提醒：
                            {state.counts.notDeliverable > 0 ? (
                                <strong className="num font-semibold text-destructive">
                                    {state.counts.notDeliverable} 处不配送
                                </strong>
                            ) : null}
                            {state.counts.notDeliverable > 0 &&
                            state.counts.freightExtra > 0
                                ? "、"
                                : null}
                            {state.counts.freightExtra > 0 ? (
                                <strong className="num font-semibold">
                                    {state.counts.freightExtra} 处运费另计
                                </strong>
                            ) : null}
                        </span>
                    </div>
                    <div className="flex flex-wrap items-center gap-1.5">
                        <Button
                            id={`${idPrefix}-banner-filter-issues`}
                            type="button"
                            variant="outline"
                            size="xs"
                            className="bg-card text-foreground"
                            onClick={() => {
                                setStatusFilter("issues")
                                setSourceFilter(null)
                                setRegionFilter(null)
                                resetPage()
                            }}
                        >
                            只看异常（{state.counts.issues}）
                        </Button>
                        {rescueDirectSource && notDeliverableKeys.length > 0 ? (
                            <Button
                                id={`${idPrefix}-banner-reassign-undeliverable`}
                                type="button"
                                variant="secondary"
                                size="xs"
                                onClick={() =>
                                    handleBatchAssign(
                                        rescueDirectSource.rowKey,
                                        new Set(notDeliverableKeys),
                                        "move_full",
                                    )
                                }
                            >
                                不配送 {notDeliverableKeys.length} 处改派给{" "}
                                {rescueDirectSource.shortLabel}
                            </Button>
                        ) : null}
                        {primaryNonDirectSource &&
                        notDeliverableKeys.length > 0 ? (
                            <Button
                                id={`${idPrefix}-banner-transfer-undeliverable`}
                                type="button"
                                variant="secondary"
                                size="xs"
                                onClick={() =>
                                    handleBatchTransferToNonDirect(
                                        primaryNonDirectSource.rowKey,
                                        new Set(notDeliverableKeys),
                                    )
                                }
                            >
                                <ArrowRightLeftIcon aria-hidden="true" />
                                不配送 {notDeliverableKeys.length} 处转交 源
                                {primaryNonDirectSource.sourceNumber}
                            </Button>
                        ) : null}
                        {canSplitNewSource &&
                        onSplitAddressesToNewSource &&
                        notDeliverableKeys.length > 0 ? (
                            <Button
                                id={`${idPrefix}-banner-split-undeliverable`}
                                type="button"
                                variant="outline"
                                size="xs"
                                className="bg-card text-foreground"
                                onClick={() => {
                                    onSplitAddressesToNewSource(
                                        notDeliverableKeys,
                                    )
                                    setActionFeedback(
                                        `已将 ${notDeliverableKeys.length} 处不配送地址拆给新来源。`,
                                    )
                                }}
                            >
                                <SplitIcon aria-hidden="true" />
                                拆给新来源
                            </Button>
                        ) : null}
                        {canSplitNewSource &&
                        onSplitAddressesToNewSource &&
                        allExceptionKeys.length > notDeliverableKeys.length ? (
                            <Button
                                id={`${idPrefix}-banner-split-all-issues`}
                                type="button"
                                variant="outline"
                                size="xs"
                                className="bg-card text-foreground"
                                onClick={() => {
                                    onSplitAddressesToNewSource(
                                        allExceptionKeys,
                                    )
                                    setActionFeedback(
                                        `已将 ${allExceptionKeys.length} 处异常地址拆给新来源。`,
                                    )
                                }}
                            >
                                <SplitIcon aria-hidden="true" />
                                {notDeliverableKeys.length > 0
                                    ? `含运费另计共 ${allExceptionKeys.length} 处一并拆分`
                                    : `${allExceptionKeys.length} 处运费另计拆给新来源`}
                            </Button>
                        ) : null}
                    </div>
                </div>
            ) : null}

            {/* 主工作区：左窄分组导航 + 右宽独立滚动表格 */}
            <div className="flex min-h-0 flex-1 overflow-hidden">
                {showLeftRail ? (
                    <aside
                        aria-label="地址分组导航"
                        className="hidden w-52 shrink-0 flex-col gap-3.5 overflow-y-auto overscroll-contain border-r border-border/60 bg-sidebar/60 p-3 md:flex"
                    >
                        <div className="space-y-0.5">
                            <p className="px-2 pb-0.5 text-tiny font-semibold text-muted-foreground">
                                按分配状态
                            </p>
                            {(
                                [
                                    ["all", "全部地址", state.counts.total],
                                    [
                                        "assigned",
                                        "已分直发",
                                        state.counts.assigned,
                                    ],
                                    [
                                        "unassigned",
                                        unassignedLabel,
                                        state.counts.unassigned,
                                    ],
                                    ...(state.counts.issues > 0
                                        ? ([
                                              [
                                                  "issues",
                                                  "配送 / 运费异常",
                                                  state.counts.issues,
                                              ],
                                          ] as const)
                                        : []),
                                ] as const
                            ).map(([value, label, count]) => {
                                const active =
                                    statusFilter === value &&
                                    !activeSourceFilter &&
                                    !activeRegionFilter
                                return (
                                    <button
                                        key={value}
                                        id={`${idPrefix}-rail-status-${value}`}
                                        type="button"
                                        aria-pressed={active}
                                        onClick={() => {
                                            setStatusFilter(value)
                                            setSourceFilter(null)
                                            setRegionFilter(null)
                                            resetPage()
                                        }}
                                        className={cn(
                                            "flex w-full cursor-pointer items-center justify-between gap-2 rounded-md px-2 py-1.5 text-left text-xs transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                            active
                                                ? "bg-card font-medium text-foreground shadow-2xs ring-1 ring-border/70"
                                                : "text-muted-foreground hover:bg-card/70 hover:text-foreground",
                                        )}
                                    >
                                        <span className="truncate">
                                            {label}
                                        </span>
                                        <span
                                            className={cn(
                                                "num shrink-0 rounded-sm px-1.5 py-0 text-tiny font-medium",
                                                value === "issues" && count > 0
                                                    ? "bg-destructive-soft text-destructive-soft-foreground"
                                                    : value === "unassigned" &&
                                                        count > 0 &&
                                                        !state.directBalanced
                                                      ? "bg-warning-soft text-warning-soft-foreground"
                                                      : "text-muted-foreground",
                                            )}
                                        >
                                            {count}
                                        </span>
                                    </button>
                                )
                            })}
                        </div>

                        {state.directSources.length > 0 ? (
                            <div className="space-y-0.5">
                                <p className="px-2 pb-0.5 text-tiny font-semibold text-muted-foreground">
                                    供应商直发来源
                                </p>
                                {state.directSources.map((src) => {
                                    const active =
                                        activeSourceFilter === src.rowKey
                                    const accent = sourceAccentForIndex(
                                        src.sourceNumber - 1,
                                    )
                                    return (
                                        <button
                                            key={src.rowKey}
                                            id={`${idPrefix}-rail-source-${toAutomationIdSegment(src.rowKey)}`}
                                            type="button"
                                            aria-pressed={active}
                                            onClick={() => {
                                                setSourceFilter(
                                                    active ? null : src.rowKey,
                                                )
                                                setStatusFilter("all")
                                                resetPage()
                                            }}
                                            className={cn(
                                                "flex w-full cursor-pointer flex-col gap-0.5 rounded-md px-2 py-1.5 text-left text-xs transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                                active
                                                    ? "bg-card font-medium text-foreground shadow-2xs ring-1 ring-primary/35"
                                                    : "text-muted-foreground hover:bg-card/70 hover:text-foreground",
                                            )}
                                        >
                                            <span
                                                className="flex items-center gap-1.5 truncate font-medium text-foreground"
                                                title={src.shortLabel}
                                            >
                                                <span
                                                    aria-hidden="true"
                                                    className={cn(
                                                        "size-2 shrink-0 rounded-full",
                                                        accent.barClass,
                                                    )}
                                                />
                                                <span className="truncate">
                                                    {src.shortLabel}
                                                </span>
                                            </span>
                                            <span className="num pl-3.5 text-tiny text-muted-foreground">
                                                {src.assignedAddressCount} 处 ·{" "}
                                                {src.assignedTotalQty}{" "}
                                                {product.unit}
                                            </span>
                                        </button>
                                    )
                                })}
                            </div>
                        ) : null}

                        {state.nonDirectSources.length > 0 ? (
                            <div className="space-y-1">
                                <p className="px-2 text-tiny font-semibold text-muted-foreground">
                                    仓库 / 库存统筹
                                </p>
                                <div className="space-y-1">
                                    {state.nonDirectSources.map((src) => {
                                        const accent = sourceAccentForIndex(
                                            src.sourceNumber - 1,
                                        )
                                        return (
                                            <div
                                                key={src.rowKey}
                                                className={cn(
                                                    "rounded-md border border-border/60 bg-card/70 px-2 py-1.5 text-xs",
                                                    !src.selected &&
                                                        "border-dashed opacity-65",
                                                )}
                                            >
                                                <div className="flex items-center gap-1.5">
                                                    <span
                                                        aria-hidden="true"
                                                        className={cn(
                                                            "size-2 shrink-0 rounded-full",
                                                            accent.barClass,
                                                        )}
                                                    />
                                                    <span
                                                        className="truncate font-medium text-foreground"
                                                        title={src.shortLabel}
                                                    >
                                                        源{src.sourceNumber} ·{" "}
                                                        {src.routeLabel}
                                                    </span>
                                                </div>
                                                <p className="num mt-0.5 pl-3.5 text-tiny text-muted-foreground">
                                                    承担 {src.quantity}{" "}
                                                    {product.unit}（出库选址）
                                                </p>
                                            </div>
                                        )
                                    })}
                                </div>
                            </div>
                        ) : null}

                        {state.regionFacets.level !== "none" ? (
                            <div className="space-y-0.5">
                                <div className="flex items-center justify-between px-2 pb-0.5">
                                    <p className="text-tiny font-semibold text-muted-foreground">
                                        {state.regionFacets.level === "province"
                                            ? "按省 / 直辖市"
                                            : "按市 / 区县"}
                                    </p>
                                    <span className="num text-tiny text-muted-foreground">
                                        {state.regionFacets.items.length} 个
                                    </span>
                                </div>
                                <div className="space-y-0.5">
                                    {state.regionFacets.items.map((item) => {
                                        const active =
                                            activeRegionFilter === item.key
                                        const hasAlert =
                                            item.notDeliverableCount > 0 ||
                                            item.freightExtraCount > 0
                                        return (
                                            <button
                                                key={item.key}
                                                id={`${idPrefix}-rail-region-${toAutomationIdSegment(encodeURIComponent(item.key))}`}
                                                type="button"
                                                aria-pressed={active}
                                                onClick={() => {
                                                    setRegionFilter(
                                                        active
                                                            ? null
                                                            : item.key,
                                                    )
                                                    resetPage()
                                                }}
                                                className={cn(
                                                    "flex w-full cursor-pointer items-center justify-between gap-1.5 rounded-md px-2 py-1.5 text-left text-xs transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                                    active
                                                        ? "bg-card font-medium text-foreground shadow-2xs ring-1 ring-primary/35"
                                                        : "text-muted-foreground hover:bg-card/70 hover:text-foreground",
                                                )}
                                            >
                                                <span
                                                    className="flex min-w-0 items-center gap-1 truncate"
                                                    title={item.label}
                                                >
                                                    <MapPinIcon
                                                        aria-hidden="true"
                                                        className={cn(
                                                            "size-3 shrink-0",
                                                            item.notDeliverableCount >
                                                                0
                                                                ? "text-destructive"
                                                                : item.freightExtraCount >
                                                                    0
                                                                  ? "text-warning"
                                                                  : "text-muted-foreground/60",
                                                        )}
                                                    />
                                                    <span className="truncate">
                                                        {item.label}
                                                    </span>
                                                </span>
                                                <span
                                                    className={cn(
                                                        "num shrink-0 text-tiny",
                                                        hasAlert
                                                            ? "font-medium text-warning-soft-foreground"
                                                            : item.unassignedCount >
                                                                0
                                                              ? "text-foreground"
                                                              : "text-muted-foreground",
                                                    )}
                                                >
                                                    {item.fullyAssignedCount}/
                                                    {item.totalCount}
                                                </span>
                                            </button>
                                        )
                                    })}
                                </div>
                            </div>
                        ) : null}
                    </aside>
                ) : null}

                {/* 右侧地址工作表：固定工具栏 + 批量条 + 固定表头 + 独立滚动表身 + 固定分页 */}
                <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
                    {/* 固定工具栏 */}
                    <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-card px-4 py-2 sm:px-5">
                        <div className="flex min-w-0 flex-wrap items-center gap-2">
                            <div className="relative w-52 sm:w-56">
                                <SearchIcon
                                    aria-hidden="true"
                                    className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                                />
                                <Input
                                    id={`${idPrefix}-search`}
                                    aria-label="搜索收件人、电话或收货地址"
                                    placeholder="搜收件人、电话、省市区或地址"
                                    className="h-control-sm bg-card pr-7 pl-8 text-xs"
                                    value={query}
                                    onChange={(event) => {
                                        setQuery(event.target.value)
                                        resetPage()
                                    }}
                                />
                                {query ? (
                                    <Button
                                        id={`${idPrefix}-search-clear`}
                                        type="button"
                                        variant="ghost"
                                        size="icon-xs"
                                        aria-label="清除搜索关键词"
                                        className="absolute top-1/2 right-0.5 -translate-y-1/2 text-muted-foreground"
                                        onClick={() => {
                                            setQuery("")
                                            resetPage()
                                        }}
                                    >
                                        <XIcon />
                                    </Button>
                                ) : null}
                            </div>

                            {!showLeftRail ? (
                                <div
                                    role="group"
                                    aria-label="按状态筛选地址"
                                    className="flex flex-wrap items-center gap-1"
                                >
                                    {(
                                        [
                                            [
                                                "all",
                                                `全部 ${state.counts.total}`,
                                            ],
                                            [
                                                "assigned",
                                                `已分直发 ${state.counts.assigned}`,
                                            ],
                                            [
                                                "unassigned",
                                                `${unassignedLabel} ${state.counts.unassigned}`,
                                            ],
                                            ...(state.counts.issues > 0
                                                ? ([
                                                      [
                                                          "issues",
                                                          `异常 ${state.counts.issues}`,
                                                      ],
                                                  ] as const)
                                                : []),
                                        ] as const
                                    ).map(([value, label]) => (
                                        <Button
                                            key={value}
                                            id={`${idPrefix}-filter-${value}`}
                                            type="button"
                                            size="xs"
                                            variant={
                                                statusFilter === value
                                                    ? "secondary"
                                                    : "ghost"
                                            }
                                            aria-pressed={
                                                statusFilter === value
                                            }
                                            onClick={() => {
                                                setStatusFilter(value)
                                                resetPage()
                                            }}
                                        >
                                            {label}
                                        </Button>
                                    ))}
                                </div>
                            ) : (
                                <div className="flex flex-wrap items-center gap-1.5">
                                    {activeRegionLabel ? (
                                        <Badge
                                            variant="secondary"
                                            className="gap-1 pr-1"
                                        >
                                            地区：{activeRegionLabel}
                                            <button
                                                id={`${idPrefix}-chip-clear-region`}
                                                type="button"
                                                aria-label="清除地区筛选"
                                                className="cursor-pointer rounded-xs hover:text-foreground"
                                                onClick={() => {
                                                    setRegionFilter(null)
                                                    resetPage()
                                                }}
                                            >
                                                <XIcon className="size-3" />
                                            </button>
                                        </Badge>
                                    ) : null}
                                    {activeSourceLabel ? (
                                        <Badge
                                            variant="secondary"
                                            className="gap-1 pr-1"
                                        >
                                            来源：{activeSourceLabel}
                                            <button
                                                id={`${idPrefix}-chip-clear-source`}
                                                type="button"
                                                aria-label="清除来源筛选"
                                                className="cursor-pointer rounded-xs hover:text-foreground"
                                                onClick={() => {
                                                    setSourceFilter(null)
                                                    resetPage()
                                                }}
                                            >
                                                <XIcon className="size-3" />
                                            </button>
                                        </Badge>
                                    ) : null}
                                    {statusFilter !== "all" ? (
                                        <Badge
                                            variant="secondary"
                                            className="gap-1 pr-1"
                                        >
                                            {statusFilter === "assigned"
                                                ? "仅看已分直发"
                                                : statusFilter === "unassigned"
                                                  ? `仅看${unassignedLabel}`
                                                  : "仅看配送异常"}
                                            <button
                                                id={`${idPrefix}-chip-clear-status`}
                                                type="button"
                                                aria-label="清除状态筛选"
                                                className="cursor-pointer rounded-xs hover:text-foreground"
                                                onClick={() => {
                                                    setStatusFilter("all")
                                                    resetPage()
                                                }}
                                            >
                                                <XIcon className="size-3" />
                                            </button>
                                        </Badge>
                                    ) : null}
                                </div>
                            )}
                        </div>

                        {/* 右侧批量提效工具 */}
                        <div className="flex flex-wrap items-center gap-1.5">
                            {state.directSources.length > 0 ? (
                                <Popover
                                    open={quotaOpen}
                                    onOpenChange={(next) => {
                                        setQuotaOpen(next)
                                        if (next) {
                                            setQuotaTargetRowKey(
                                                activeSourceFilter ??
                                                    state.primaryDirectSource
                                                        ?.rowKey ??
                                                    "",
                                            )
                                            setQuotaScopeOnlyFiltered(
                                                hasActiveFilters,
                                            )
                                        }
                                    }}
                                >
                                    <PopoverTrigger
                                        render={
                                            <Button
                                                id={`${idPrefix}-quota-open`}
                                                type="button"
                                                variant="outline"
                                                size="xs"
                                            />
                                        }
                                    >
                                        <ZapIcon aria-hidden="true" />
                                        按数量自动分配
                                    </PopoverTrigger>
                                    <PopoverContent
                                        align="end"
                                        className="w-80 space-y-3 p-4"
                                    >
                                        <div className="space-y-0.5">
                                            <p className="text-body-compact font-semibold text-foreground">
                                                按目标数量自动凑单分配
                                            </p>
                                            <p className="text-xs text-muted-foreground">
                                                按目标量快速挑选可配送地址，自动跳过不可配送地区。
                                            </p>
                                        </div>

                                        {state.directSources.length > 1 ? (
                                            <div className="space-y-1">
                                                <p className="text-xs font-medium text-foreground/75">
                                                    目标直发来源
                                                </p>
                                                <Select
                                                    value={resolvedQuotaTarget}
                                                    items={state.directSources.map(
                                                        (src) => ({
                                                            value: src.rowKey,
                                                            label: src.shortLabel,
                                                        }),
                                                    )}
                                                    onValueChange={(next) => {
                                                        if (next)
                                                            setQuotaTargetRowKey(
                                                                next,
                                                            )
                                                    }}
                                                >
                                                    <SelectTrigger
                                                        id={`${idPrefix}-quota-source`}
                                                        size="sm"
                                                        aria-label="目标直发来源"
                                                        className="w-full"
                                                    >
                                                        <SelectValue />
                                                    </SelectTrigger>
                                                    <SelectContent>
                                                        {state.directSources.map(
                                                            (src) => (
                                                                <SelectItem
                                                                    key={
                                                                        src.rowKey
                                                                    }
                                                                    idPrefix={`${idPrefix}-quota-source`}
                                                                    value={
                                                                        src.rowKey
                                                                    }
                                                                >
                                                                    {
                                                                        src.shortLabel
                                                                    }
                                                                </SelectItem>
                                                            ),
                                                        )}
                                                    </SelectContent>
                                                </Select>
                                            </div>
                                        ) : null}

                                        <div className="space-y-1.5">
                                            <label
                                                htmlFor={`${idPrefix}-quota-qty`}
                                                className="text-xs font-medium text-foreground/75"
                                            >
                                                目标承接总量（{product.unit}）
                                            </label>
                                            <Input
                                                id={`${idPrefix}-quota-qty`}
                                                type="number"
                                                inputMode="decimal"
                                                min="0"
                                                step={
                                                    sourcingQuantityStep(
                                                        product.quantityScale,
                                                    ) ?? "1"
                                                }
                                                placeholder={`例如 ${state.directTargetQty}`}
                                                className="num h-control-sm"
                                                value={quotaQty}
                                                onChange={(event) =>
                                                    setQuotaQty(
                                                        event.target.value,
                                                    )
                                                }
                                            />
                                        </div>

                                        <div className="space-y-2">
                                            <label
                                                htmlFor={`${idPrefix}-quota-prefer-free`}
                                                className="flex cursor-pointer items-center gap-2 text-xs text-foreground"
                                            >
                                                <Checkbox
                                                    id={`${idPrefix}-quota-prefer-free`}
                                                    checked={quotaPreferFree}
                                                    onCheckedChange={(c) =>
                                                        setQuotaPreferFree(
                                                            c === true,
                                                        )
                                                    }
                                                />
                                                优先分配包邮 / 免运费地址
                                            </label>
                                            {state.directSources.length > 1 ? (
                                                <label
                                                    htmlFor={`${idPrefix}-quota-allow-reallocate`}
                                                    className="flex cursor-pointer items-center gap-2 text-xs text-foreground"
                                                >
                                                    <Checkbox
                                                        id={`${idPrefix}-quota-allow-reallocate`}
                                                        checked={
                                                            quotaAllowReallocate
                                                        }
                                                        onCheckedChange={(c) =>
                                                            setQuotaAllowReallocate(
                                                                c === true,
                                                            )
                                                        }
                                                    />
                                                    未分配不足时从其他直发源调拨
                                                </label>
                                            ) : null}
                                            {hasActiveFilters ? (
                                                <label
                                                    htmlFor={`${idPrefix}-quota-scope-filtered`}
                                                    className="flex cursor-pointer items-center gap-2 text-xs text-foreground"
                                                >
                                                    <Checkbox
                                                        id={`${idPrefix}-quota-scope-filtered`}
                                                        checked={
                                                            quotaScopeOnlyFiltered
                                                        }
                                                        onCheckedChange={(c) =>
                                                            setQuotaScopeOnlyFiltered(
                                                                c === true,
                                                            )
                                                        }
                                                    />
                                                    仅对当前筛选的{" "}
                                                    {filteredEntries.length}{" "}
                                                    处地址生效
                                                </label>
                                            ) : null}
                                        </div>

                                        {quotaPreview ? (
                                            <div
                                                className={cn(
                                                    "rounded-md px-3 py-2 text-xs",
                                                    quotaPreview.valid
                                                        ? "bg-surface-sunken text-foreground"
                                                        : "bg-destructive-soft text-destructive-soft-foreground",
                                                )}
                                            >
                                                {quotaPreview.valid ? (
                                                    <>
                                                        将分配{" "}
                                                        <strong className="num font-semibold">
                                                            {
                                                                quotaPreview.allocatedAddressCount
                                                            }{" "}
                                                            处
                                                        </strong>
                                                        地址，合计{" "}
                                                        <strong className="num font-semibold">
                                                            {
                                                                quotaPreview.allocatedQty
                                                            }{" "}
                                                            {product.unit}
                                                        </strong>
                                                        {isPositiveQuantity(
                                                            quotaPreview.shortfallQty,
                                                        ) ? (
                                                            <span className="block pt-0.5 text-warning-soft-foreground">
                                                                可配余量不足，尚差{" "}
                                                                {
                                                                    quotaPreview.shortfallQty
                                                                }{" "}
                                                                {product.unit}
                                                            </span>
                                                        ) : null}
                                                    </>
                                                ) : (
                                                    quotaPreview.error
                                                )}
                                            </div>
                                        ) : null}

                                        <div className="flex justify-end gap-2 pt-1">
                                            <Button
                                                id={`${idPrefix}-quota-cancel`}
                                                type="button"
                                                variant="ghost"
                                                size="xs"
                                                onClick={() =>
                                                    setQuotaOpen(false)
                                                }
                                            >
                                                取消
                                            </Button>
                                            <Button
                                                id={`${idPrefix}-quota-apply`}
                                                type="button"
                                                size="xs"
                                                disabled={!quotaPreview?.valid}
                                                onClick={() => {
                                                    if (!quotaPreview?.valid)
                                                        return
                                                    applyPatches(
                                                        quotaPreview.patches,
                                                    )
                                                    setActionFeedback(
                                                        `已按数量自动分配 ${quotaPreview.allocatedAddressCount} 处地址（共 ${quotaPreview.allocatedQty} ${product.unit}）。`,
                                                    )
                                                    setQuotaOpen(false)
                                                }}
                                            >
                                                确认分配
                                            </Button>
                                        </div>
                                    </PopoverContent>
                                </Popover>
                            ) : null}

                            {state.directSources.length === 1 &&
                            defaultSource ? (
                                <>
                                    <Button
                                        id={`${idPrefix}-fill-all`}
                                        type="button"
                                        variant="outline"
                                        size="xs"
                                        disabled={
                                            filteredEntries.filter(
                                                (e) =>
                                                    e.deliverableByPrimary &&
                                                    isPositiveQuantity(
                                                        e.remainingUnassigned,
                                                    ),
                                            ).length === 0
                                        }
                                        onClick={() =>
                                            handleBatchAssign(
                                                defaultSource.rowKey,
                                                filteredKeySet,
                                                "fill_remaining",
                                            )
                                        }
                                    >
                                        {hasActiveFilters
                                            ? "填满当前筛选"
                                            : "一键全分可配"}
                                    </Button>
                                    <Button
                                        id={`${idPrefix}-clear-all`}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        disabled={
                                            filteredEntries.filter((e) =>
                                                isPositiveQuantity(
                                                    e.assignedTotal,
                                                ),
                                            ).length === 0
                                        }
                                        onClick={() =>
                                            handleBatchClear(filteredKeySet)
                                        }
                                    >
                                        <EraserIcon aria-hidden="true" />
                                        {hasActiveFilters
                                            ? "清空筛选"
                                            : "清空直发"}
                                    </Button>
                                </>
                            ) : null}

                            {canSplitNewSource &&
                            onSplitResidual &&
                            hasDirectShortfall &&
                            state.counts.unassigned > 0 ? (
                                <Button
                                    id={`${idPrefix}-split-residual`}
                                    type="button"
                                    variant="secondary"
                                    size="xs"
                                    onClick={() => {
                                        onSplitResidual()
                                        setActionFeedback(
                                            `已将未分配余量拆给新来源。`,
                                        )
                                    }}
                                >
                                    <SplitIcon aria-hidden="true" />
                                    未分余量拆给新来源
                                </Button>
                            ) : null}
                        </div>
                    </div>

                    {/* 勾选后的批量操作条：支持直发改派 + 跨模式转交仓库/库存 + 拆新来源 */}
                    {effectiveSelectedKeys.size > 0 ? (
                        <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-primary/25 bg-primary/5 px-4 py-2 text-xs sm:px-5">
                            <div className="flex flex-wrap items-center gap-2">
                                <span className="font-medium text-foreground">
                                    已选{" "}
                                    <span className="num font-semibold text-primary">
                                        {effectiveSelectedKeys.size}
                                    </span>{" "}
                                    处地址（共{" "}
                                    <span className="num font-semibold">
                                        {selectedDemandTotal} {product.unit}
                                    </span>
                                    ）
                                </span>
                                {allPageSelected &&
                                !allFilteredSelected &&
                                filteredEntries.length > pageEntries.length ? (
                                    <button
                                        id={`${idPrefix}-select-all-filtered`}
                                        type="button"
                                        onClick={selectAllFiltered}
                                        className="cursor-pointer font-medium text-primary hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                                    >
                                        全选当前筛选全部{" "}
                                        {filteredEntries.length} 处
                                    </button>
                                ) : null}
                                <button
                                    id={`${idPrefix}-clear-selection`}
                                    type="button"
                                    onClick={clearSelection}
                                    className="cursor-pointer text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                                >
                                    取消选择
                                </button>
                            </div>

                            <div className="flex flex-wrap items-center gap-1.5">
                                {state.directSources.length === 1 &&
                                defaultSource ? (
                                    <Button
                                        id={`${idPrefix}-bulk-assign-single`}
                                        type="button"
                                        size="xs"
                                        variant="secondary"
                                        onClick={() =>
                                            handleBatchAssign(
                                                defaultSource.rowKey,
                                                effectiveSelectedKeys,
                                                "move_full",
                                            )
                                        }
                                    >
                                        分配给 {defaultSource.shortLabel}
                                    </Button>
                                ) : state.directSources.length > 1 ? (
                                    <DropdownMenu>
                                        <DropdownMenuTrigger
                                            render={
                                                <Button
                                                    id={`${idPrefix}-bulk-reassign-trigger`}
                                                    type="button"
                                                    size="xs"
                                                    variant="secondary"
                                                />
                                            }
                                        >
                                            改派给直发来源
                                            <ChevronDownIcon aria-hidden="true" />
                                        </DropdownMenuTrigger>
                                        <DropdownMenuContent
                                            align="end"
                                            className="w-60"
                                        >
                                            {state.directSources.map((src) => (
                                                <DropdownMenuItem
                                                    key={src.rowKey}
                                                    id={`${idPrefix}-bulk-reassign-${toAutomationIdSegment(src.rowKey)}`}
                                                    onClick={() =>
                                                        handleBatchAssign(
                                                            src.rowKey,
                                                            effectiveSelectedKeys,
                                                            "move_full",
                                                        )
                                                    }
                                                >
                                                    <span className="truncate">
                                                        {src.shortLabel}
                                                    </span>
                                                </DropdownMenuItem>
                                            ))}
                                        </DropdownMenuContent>
                                    </DropdownMenu>
                                ) : null}

                                {/* 混合履约：将选中地址释放直发并把对应数量划入仓库/库存来源 */}
                                {state.nonDirectSources.length === 1 &&
                                primaryNonDirectSource ? (
                                    <Button
                                        id={`${idPrefix}-bulk-transfer-warehouse`}
                                        type="button"
                                        size="xs"
                                        variant="outline"
                                        className="bg-card"
                                        onClick={() =>
                                            handleBatchTransferToNonDirect(
                                                primaryNonDirectSource.rowKey,
                                                effectiveSelectedKeys,
                                            )
                                        }
                                    >
                                        <ArrowRightLeftIcon aria-hidden="true" />
                                        转入 源
                                        {primaryNonDirectSource.sourceNumber}·
                                        {primaryNonDirectSource.routeLabel}
                                    </Button>
                                ) : state.nonDirectSources.length > 1 ? (
                                    <DropdownMenu>
                                        <DropdownMenuTrigger
                                            render={
                                                <Button
                                                    id={`${idPrefix}-bulk-transfer-trigger`}
                                                    type="button"
                                                    size="xs"
                                                    variant="outline"
                                                    className="bg-card"
                                                />
                                            }
                                        >
                                            <ArrowRightLeftIcon aria-hidden="true" />
                                            转入仓库 / 库存
                                            <ChevronDownIcon aria-hidden="true" />
                                        </DropdownMenuTrigger>
                                        <DropdownMenuContent
                                            align="end"
                                            className="w-64"
                                        >
                                            {state.nonDirectSources.map(
                                                (src) => (
                                                    <DropdownMenuItem
                                                        key={src.rowKey}
                                                        id={`${idPrefix}-bulk-transfer-${toAutomationIdSegment(src.rowKey)}`}
                                                        onClick={() =>
                                                            handleBatchTransferToNonDirect(
                                                                src.rowKey,
                                                                effectiveSelectedKeys,
                                                            )
                                                        }
                                                    >
                                                        <span className="flex min-w-0 items-center gap-1.5">
                                                            {src.isStock ? (
                                                                <PackageCheckIcon
                                                                    aria-hidden="true"
                                                                    className="size-3.5 shrink-0 text-teal-soft-foreground"
                                                                />
                                                            ) : (
                                                                <WarehouseIcon
                                                                    aria-hidden="true"
                                                                    className="size-3.5 shrink-0 text-orange-soft-foreground"
                                                                />
                                                            )}
                                                            <span className="truncate">
                                                                {src.shortLabel}
                                                            </span>
                                                        </span>
                                                    </DropdownMenuItem>
                                                ),
                                            )}
                                        </DropdownMenuContent>
                                    </DropdownMenu>
                                ) : null}

                                {canSplitNewSource &&
                                onSplitAddressesToNewSource ? (
                                    <Button
                                        id={`${idPrefix}-bulk-split-new`}
                                        type="button"
                                        size="xs"
                                        variant="outline"
                                        className="bg-card"
                                        onClick={() => {
                                            const keys = [
                                                ...effectiveSelectedKeys,
                                            ]
                                            onSplitAddressesToNewSource(keys)
                                            setActionFeedback(
                                                `已将选中的 ${keys.length} 处地址拆给新来源。`,
                                            )
                                            clearSelection()
                                        }}
                                    >
                                        <SplitIcon aria-hidden="true" />
                                        拆给新来源
                                    </Button>
                                ) : null}

                                <Button
                                    id={`${idPrefix}-bulk-clear`}
                                    type="button"
                                    size="xs"
                                    variant="ghost"
                                    onClick={() =>
                                        handleBatchClear(effectiveSelectedKeys)
                                    }
                                >
                                    仅取消直发
                                </Button>
                            </div>
                        </div>
                    ) : actionFeedback ? (
                        <div
                            role="status"
                            className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken px-4 py-1.5 text-xs text-muted-foreground sm:px-5"
                        >
                            <span>{actionFeedback}</span>
                            <button
                                id={`${idPrefix}-feedback-dismiss`}
                                type="button"
                                aria-label="关闭操作提示"
                                className="cursor-pointer hover:text-foreground"
                                onClick={() => setActionFeedback(null)}
                            >
                                <XIcon className="size-3.5" />
                            </button>
                        </div>
                    ) : null}

                    {/* 固定表头 */}
                    <div className="grid shrink-0 grid-cols-[1.75rem_minmax(0,1fr)_5.25rem_12.5rem_8.5rem] items-center gap-3 border-b border-border/70 bg-table-header px-4 py-1.5 text-xs font-medium text-muted-foreground sm:px-5">
                        <div className="flex items-center justify-center">
                            <NativeCheckbox
                                id={`${idPrefix}-select-page`}
                                aria-label="全选当前页地址用于批量操作"
                                checked={allPageSelected}
                                indeterminate={somePageSelected}
                                disabled={pageEntries.length === 0}
                                onCheckedChange={(checked) =>
                                    togglePageSelection(checked)
                                }
                            />
                        </div>
                        <span>收件人 / 交付地址</span>
                        <span>配送状态</span>
                        <span>承接去向</span>
                        <span className="text-right">直发量 / 需求</span>
                    </div>

                    {/* 独立滚动表身 */}
                    <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
                        {pageEntries.length === 0 ? (
                            <div className="flex flex-col items-center justify-center gap-2 px-6 py-10 text-center text-xs text-muted-foreground">
                                <p>当前筛选条件下没有交付地址</p>
                                {hasActiveFilters ? (
                                    <Button
                                        id={`${idPrefix}-reset-filters`}
                                        type="button"
                                        variant="outline"
                                        size="xs"
                                        onClick={() => {
                                            setStatusFilter("all")
                                            setSourceFilter(null)
                                            setRegionFilter(null)
                                            setQuery("")
                                            resetPage()
                                        }}
                                    >
                                        查看全部 {state.counts.total} 处地址
                                    </Button>
                                ) : null}
                            </div>
                        ) : (
                            <div className="divide-y divide-grid">
                                {pageEntries.map((entry) => (
                                    <GlobalAddressRow
                                        key={entry.key}
                                        idPrefix={idPrefix}
                                        entry={entry}
                                        unit={product.unit}
                                        quantityScale={product.quantityScale}
                                        directSources={state.directSources}
                                        nonDirectSources={
                                            state.nonDirectSources
                                        }
                                        hasNonDirectSources={
                                            state.hasNonDirectSources
                                        }
                                        hasDirectShortfall={hasDirectShortfall}
                                        directBalanced={state.directBalanced}
                                        primarySource={defaultSource}
                                        checked={effectiveSelectedKeys.has(
                                            entry.key,
                                        )}
                                        onToggleChecked={(checked) =>
                                            toggleRowSelection(
                                                entry.key,
                                                checked,
                                            )
                                        }
                                        onAssignExclusive={(targetRowKey) => {
                                            const keySet = new Set([entry.key])
                                            if (!targetRowKey) {
                                                applyPatches(
                                                    buildClearAddressesPatches(
                                                        product,
                                                        form.state.values.lines,
                                                        keySet,
                                                    ),
                                                )
                                                return
                                            }
                                            if (
                                                targetRowKey.startsWith(
                                                    TRANSFER_VALUE_PREFIX,
                                                )
                                            ) {
                                                const nonDirectKey =
                                                    targetRowKey.slice(
                                                        TRANSFER_VALUE_PREFIX.length,
                                                    )
                                                handleBatchTransferToNonDirect(
                                                    nonDirectKey,
                                                    keySet,
                                                )
                                                return
                                            }
                                            const res = buildBatchAssignPatches(
                                                product,
                                                form.state.values.lines,
                                                targetRowKey,
                                                keySet,
                                                "move_full",
                                            )
                                            applyPatches(res.patches)
                                        }}
                                        onSetSliceQty={(
                                            targetRowKey,
                                            nextQty,
                                            removeIfZero,
                                        ) => {
                                            const patch =
                                                buildSetSliceQuantityPatch(
                                                    product,
                                                    form.state.values.lines,
                                                    targetRowKey,
                                                    entry.baseRow,
                                                    nextQty,
                                                    removeIfZero,
                                                )
                                            if (patch) applyPatches([patch])
                                        }}
                                        onSplitRemainderToSource={(
                                            targetRowKey,
                                        ) => {
                                            const patch =
                                                buildSetSliceQuantityPatch(
                                                    product,
                                                    form.state.values.lines,
                                                    targetRowKey,
                                                    entry.baseRow,
                                                    entry.remainingUnassigned,
                                                )
                                            if (patch) applyPatches([patch])
                                        }}
                                    />
                                ))}
                            </div>
                        )}
                    </div>

                    {/* 固定分页与汇总底栏 */}
                    <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-border/70 bg-surface-sunken px-4 py-1.5 text-xs text-muted-foreground sm:px-5">
                        <div className="flex flex-wrap items-center gap-3">
                            <span className="num">
                                {filteredEntries.length === 0
                                    ? "共 0 处"
                                    : filteredEntries.length > pageSize
                                      ? `第 ${pageStart + 1}–${Math.min(pageStart + pageSize, filteredEntries.length)} 处，共 ${filteredEntries.length} 处`
                                      : `共 ${filteredEntries.length} 处地址`}
                                {hasActiveFilters &&
                                filteredEntries.length !== state.counts.total
                                    ? `（总计 ${state.counts.total} 处）`
                                    : ""}
                            </span>
                            <span aria-hidden="true" className="opacity-40">
                                ·
                            </span>
                            <span>
                                直发合计{" "}
                                <QuantityValue
                                    value={state.directAssignedTotalQty}
                                    unit={product.unit}
                                    className="font-semibold text-foreground"
                                />
                                {state.hasNonDirectSources ? (
                                    <span className="num text-muted-foreground">
                                        {" "}
                                        / 额度 {state.directTargetQty}{" "}
                                        {product.unit}
                                    </span>
                                ) : null}
                            </span>
                        </div>

                        {filteredEntries.length > 25 ? (
                            <div className="flex items-center gap-2">
                                <div className="flex items-center gap-1.5">
                                    <span>每页</span>
                                    <Select
                                        value={String(pageSize)}
                                        items={[
                                            { value: "25", label: "25 条" },
                                            { value: "50", label: "50 条" },
                                            { value: "100", label: "100 条" },
                                        ]}
                                        onValueChange={(raw) => {
                                            if (!raw) return
                                            const nextSize: PageSizeOption =
                                                raw === "100"
                                                    ? 100
                                                    : raw === "50"
                                                      ? 50
                                                      : 25
                                            setPageSize(nextSize)
                                            setPageIndex(0)
                                        }}
                                    >
                                        <SelectTrigger
                                            id={`${idPrefix}-page-size`}
                                            size="sm"
                                            aria-label="每页显示地址条数"
                                            className="w-22 bg-card"
                                        >
                                            <SelectValue />
                                        </SelectTrigger>
                                        <SelectContent align="end">
                                            <SelectItem
                                                idPrefix={`${idPrefix}-page-size`}
                                                value="25"
                                            >
                                                25 条
                                            </SelectItem>
                                            <SelectItem
                                                idPrefix={`${idPrefix}-page-size`}
                                                value="50"
                                            >
                                                50 条
                                            </SelectItem>
                                            <SelectItem
                                                idPrefix={`${idPrefix}-page-size`}
                                                value="100"
                                            >
                                                100 条
                                            </SelectItem>
                                        </SelectContent>
                                    </Select>
                                </div>
                                <div className="inline-flex items-center gap-1">
                                    <Button
                                        id={`${idPrefix}-page-prev`}
                                        type="button"
                                        variant="outline"
                                        size="icon-xs"
                                        aria-label="上一页"
                                        disabled={safePageIndex <= 0}
                                        onClick={() =>
                                            setPageIndex((p) =>
                                                Math.max(0, p - 1),
                                            )
                                        }
                                    >
                                        <ChevronLeftIcon />
                                    </Button>
                                    <span className="num min-w-12 text-center font-medium text-foreground">
                                        {safePageIndex + 1} / {totalPages}
                                    </span>
                                    <Button
                                        id={`${idPrefix}-page-next`}
                                        type="button"
                                        variant="outline"
                                        size="icon-xs"
                                        aria-label="下一页"
                                        disabled={
                                            safePageIndex >= totalPages - 1
                                        }
                                        onClick={() =>
                                            setPageIndex((p) =>
                                                Math.min(totalPages - 1, p + 1),
                                            )
                                        }
                                    >
                                        <ChevronRightIcon />
                                    </Button>
                                </div>
                            </div>
                        ) : null}
                    </div>
                </div>
            </div>
        </section>
    )
}

function GlobalAddressRow({
    idPrefix,
    entry,
    unit,
    quantityScale,
    directSources,
    nonDirectSources,
    hasNonDirectSources,
    hasDirectShortfall,
    directBalanced,
    primarySource,
    checked,
    onToggleChecked,
    onAssignExclusive,
    onSetSliceQty,
    onSplitRemainderToSource,
}: {
    idPrefix: string
    entry: GlobalAddressEntry
    unit: string
    quantityScale?: number | null
    directSources: readonly DirectSourceInfo[]
    nonDirectSources: readonly NonDirectSourceInfo[]
    hasNonDirectSources: boolean
    hasDirectShortfall: boolean
    directBalanced: boolean
    primarySource?: DirectSourceInfo
    checked: boolean
    onToggleChecked: (checked: boolean) => void
    onAssignExclusive: (targetRowKey: string | null) => void
    onSetSliceQty: (
        targetRowKey: string,
        nextQty: string,
        removeIfZero?: boolean,
    ) => void
    onSplitRemainderToSource: (targetRowKey: string) => void
}) {
    const rowId = `${idPrefix}-${addressIdSegment(entry.baseRow)}`
    const isSplitMulti = entry.slices.length > 1
    const singleSlice = entry.slices.length === 1 ? entry.slices[0] : undefined
    const editableSource = singleSlice
        ? directSources.find((src) => src.rowKey === singleSlice.rowKey)
        : primarySource
    const hasPartialRemainder =
        !isSplitMulti &&
        Boolean(singleSlice?.positive) &&
        isPositiveQuantity(entry.remainingUnassigned)
    const fallbackSplitSource = hasPartialRemainder
        ? directSources.find(
              (src) =>
                  src.rowKey !== singleSlice?.rowKey &&
                  src.selected &&
                  src.option.delivery?.regionTerms[
                      entry.baseRow.region_code
                  ] !== "NOT_DELIVERABLE",
          )
        : undefined

    const termOption = singleSlice?.option ?? primarySource?.option
    const termBadge =
        termOption && entry.effectiveTerm ? (
            <Badge
                variant={
                    entry.effectiveTerm === "NOT_DELIVERABLE"
                        ? "destructive"
                        : entry.effectiveTerm === "FREIGHT_EXTRA"
                          ? "warning"
                          : "neutral"
                }
                className="h-5 px-1.5 text-tiny"
            >
                {deliveryTermLabel(termOption, entry.effectiveTerm)}
            </Badge>
        ) : (
            <span className="text-xs text-muted-foreground">—</span>
        )

    const errorMessages = [
        ...new Set(
            entry.slices
                .map((slice) => slice.error)
                .filter((msg): msg is string => Boolean(msg)),
        ),
    ]

    const normalizeOnBlur = (targetRowKey: string, rawQty: string) => {
        const trimmed = rawQty.trim()
        if (trimmed === "") {
            onSetSliceQty(targetRowKey, "0", true)
            return
        }
        try {
            const normalized = compactFixed(trimmed)
            onSetSliceQty(
                targetRowKey,
                normalized,
                compareDecimal(normalized, "0", 6) === 0,
            )
        } catch {
            // 保留非法输入交由行内错误提示
        }
    }

    const useRichSelect =
        directSources.length > 1 || nonDirectSources.length > 0
    const canTransferRowToNonDirect =
        nonDirectSources.length > 0 &&
        (Boolean(singleSlice?.positive) || hasDirectShortfall)
    const unassignedDisplayLabel = hasNonDirectSources
        ? directBalanced
            ? "仓库 / 库存统筹"
            : "暂不直发（留给仓库）"
        : "未分配"

    return (
        <div
            className={cn(
                "transition-colors hover:bg-row-hover",
                entry.hasError &&
                    "bg-destructive-soft/30 hover:bg-destructive-soft/45",
                !entry.hasError &&
                    !isPositiveQuantity(entry.assignedTotal) &&
                    "bg-surface-sunken/35",
            )}
        >
            <div className="grid grid-cols-[1.75rem_minmax(0,1fr)_5.25rem_12.5rem_8.5rem] items-center gap-3 px-4 py-1.5 sm:px-5">
                <div className="flex items-center justify-center">
                    <NativeCheckbox
                        id={`${rowId}-select`}
                        checked={checked}
                        aria-label={`选择 ${entry.baseRow.recipient} 用于批量操作`}
                        onCheckedChange={onToggleChecked}
                    />
                </div>

                {/* 收件人与地址 */}
                <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
                        <span className="text-body-compact font-medium text-foreground">
                            {entry.baseRow.recipient}
                        </span>
                        <span className="num text-xs text-muted-foreground">
                            {entry.baseRow.phone}
                        </span>
                    </div>
                    <p
                        className="mt-0.5 truncate text-xs text-muted-foreground"
                        title={entry.fullAddress}
                    >
                        {entry.fullAddress}
                    </p>
                </div>

                {/* 配送状态 */}
                <div>{termBadge}</div>

                {/* 承接去向（支持单源一键开关 / 多源直发改派 / 跨模式转交仓库或库存） */}
                <div className="min-w-0">
                    {isSplitMulti ? (
                        <Badge variant="secondary" className="h-5 px-2 text-xs">
                            已拆 {entry.slices.length} 源
                        </Badge>
                    ) : !useRichSelect && primarySource ? (
                        !entry.deliverableByPrimary &&
                        !singleSlice?.positive ? (
                            <span className="text-xs text-muted-foreground">
                                不可配送
                            </span>
                        ) : (
                            <button
                                id={`${rowId}-toggle-assign`}
                                type="button"
                                aria-pressed={Boolean(singleSlice?.positive)}
                                onClick={() =>
                                    onAssignExclusive(
                                        singleSlice?.positive
                                            ? null
                                            : primarySource.rowKey,
                                    )
                                }
                                className={cn(
                                    "inline-flex max-w-full cursor-pointer items-center gap-1.5 rounded-md border px-2 py-1 text-xs font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                    singleSlice?.positive
                                        ? "border-primary/35 bg-primary/8 text-foreground hover:bg-primary/14"
                                        : "border-dashed border-border bg-card text-muted-foreground hover:border-primary/40 hover:text-foreground",
                                )}
                            >
                                <span
                                    aria-hidden="true"
                                    className={cn(
                                        "size-1.5 shrink-0 rounded-full",
                                        singleSlice?.positive
                                            ? "bg-primary"
                                            : "bg-muted-foreground/40",
                                    )}
                                />
                                <span className="truncate">
                                    {singleSlice?.positive
                                        ? primarySource.supplierName
                                        : "未分配（点击分配）"}
                                </span>
                            </button>
                        )
                    ) : (
                        <Select
                            value={singleSlice?.rowKey ?? "__unassigned__"}
                            items={[
                                {
                                    value: "__unassigned__",
                                    label: unassignedDisplayLabel,
                                },
                                ...directSources.map((src) => ({
                                    value: src.rowKey,
                                    label: src.shortLabel,
                                })),
                                ...(canTransferRowToNonDirect
                                    ? nonDirectSources.map((src) => ({
                                          value: `${TRANSFER_VALUE_PREFIX}${src.rowKey}`,
                                          label: `转入 源${src.sourceNumber}·${src.routeLabel}（+${entry.demandQty}）`,
                                      }))
                                    : []),
                            ]}
                            onValueChange={(next) =>
                                onAssignExclusive(
                                    !next || next === "__unassigned__"
                                        ? null
                                        : next,
                                )
                            }
                        >
                            <SelectTrigger
                                id={`${rowId}-source`}
                                size="sm"
                                aria-label={`${entry.baseRow.recipient} 的承接去向`}
                                className="w-full bg-card text-xs"
                            >
                                <SelectValue />
                            </SelectTrigger>
                            <SelectContent align="start" className="min-w-56">
                                <SelectItem
                                    idPrefix={`${rowId}-source`}
                                    value="__unassigned__"
                                >
                                    {hasNonDirectSources
                                        ? "仅取消直发（留给仓库统筹）"
                                        : "未分配"}
                                </SelectItem>
                                {directSources.map((src) => {
                                    const term =
                                        src.option.delivery?.regionTerms[
                                            entry.baseRow.region_code
                                        ]
                                    const undeliverable =
                                        term === "NOT_DELIVERABLE"
                                    const suffix = undeliverable
                                        ? "（不配送）"
                                        : term === "FREIGHT_EXTRA"
                                          ? "（运费另计）"
                                          : ""
                                    return (
                                        <SelectItem
                                            key={src.rowKey}
                                            idPrefix={`${rowId}-source`}
                                            value={src.rowKey}
                                            disabled={
                                                undeliverable &&
                                                singleSlice?.rowKey !==
                                                    src.rowKey
                                            }
                                        >
                                            {src.shortLabel}
                                            {suffix}
                                        </SelectItem>
                                    )
                                })}
                                {canTransferRowToNonDirect
                                    ? nonDirectSources.map((src) => (
                                          <SelectItem
                                              key={`${TRANSFER_VALUE_PREFIX}${src.rowKey}`}
                                              idPrefix={`${rowId}-source`}
                                              value={`${TRANSFER_VALUE_PREFIX}${src.rowKey}`}
                                          >
                                              转入 源{src.sourceNumber}·
                                              {src.routeLabel}（仓量 +
                                              {entry.demandQty} {unit}）
                                          </SelectItem>
                                      ))
                                    : null}
                            </SelectContent>
                        </Select>
                    )}
                </div>

                {/* 分配 / 需求 */}
                <div className="flex flex-col items-end gap-0.5">
                    {isSplitMulti ? (
                        <div className="flex items-baseline gap-1 text-xs">
                            <span
                                className={cn(
                                    "num text-body-compact font-semibold",
                                    entry.overAllocated
                                        ? "text-destructive"
                                        : isPositiveQuantity(
                                                entry.remainingUnassigned,
                                            )
                                          ? "text-warning-soft-foreground"
                                          : "text-foreground",
                                )}
                            >
                                {entry.assignedTotal}
                            </span>
                            <span className="num text-muted-foreground">
                                / {entry.demandQty} {unit}
                            </span>
                        </div>
                    ) : editableSource &&
                      (entry.anySourceDeliverable || singleSlice?.positive) ? (
                        <div className="flex items-center justify-end gap-1">
                            <Input
                                id={`${rowId}-quantity`}
                                type="number"
                                inputMode="decimal"
                                min="0"
                                max={entry.demandQty}
                                step={
                                    sourcingQuantityStep(quantityScale) ?? "1"
                                }
                                placeholder="0"
                                aria-label={`${entry.baseRow.recipient}，${entry.fullAddress}，本次交付数量（${unit}）`}
                                aria-invalid={entry.hasError || undefined}
                                className="num h-control-sm w-15 bg-card text-right"
                                value={singleSlice?.quantity ?? ""}
                                onChange={(event) =>
                                    onSetSliceQty(
                                        editableSource.rowKey,
                                        event.target.value,
                                    )
                                }
                                onBlur={() =>
                                    normalizeOnBlur(
                                        editableSource.rowKey,
                                        singleSlice?.quantity ?? "",
                                    )
                                }
                            />
                            <span className="num shrink-0 text-xs text-muted-foreground">
                                / {entry.demandQty} {unit}
                            </span>
                        </div>
                    ) : (
                        <span className="num text-xs text-muted-foreground">
                            0 / {entry.demandQty} {unit}
                        </span>
                    )}

                    {hasPartialRemainder ? (
                        <div className="flex items-center gap-1.5 text-tiny">
                            <span className="num text-warning-soft-foreground">
                                剩 {entry.remainingUnassigned} {unit}
                            </span>
                            {fallbackSplitSource ? (
                                <button
                                    id={`${rowId}-split-remainder`}
                                    type="button"
                                    onClick={() =>
                                        onSplitRemainderToSource(
                                            fallbackSplitSource.rowKey,
                                        )
                                    }
                                    className="inline-flex cursor-pointer items-center gap-0.5 font-medium text-primary hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                                >
                                    <PlusIcon className="size-2.5" />
                                    分给源{fallbackSplitSource.sourceNumber}
                                </button>
                            ) : null}
                        </div>
                    ) : null}
                </div>
            </div>

            {/* 单地址拆给多来源时的清晰子行 */}
            {isSplitMulti ? (
                <div className="space-y-1 border-t border-border/40 bg-surface-sunken/50 py-1.5 pr-4 pl-12 sm:pr-5">
                    {entry.slices.map((slice) => (
                        <div
                            key={slice.rowKey}
                            className="flex flex-wrap items-center justify-between gap-2 text-xs"
                        >
                            <span className="text-muted-foreground">
                                {slice.shortLabel}
                            </span>
                            <div className="flex items-center gap-1.5">
                                <Input
                                    id={`${rowId}-slice-${toAutomationIdSegment(slice.rowKey)}-qty`}
                                    type="number"
                                    inputMode="decimal"
                                    min="0"
                                    max={entry.demandQty}
                                    step={
                                        sourcingQuantityStep(quantityScale) ??
                                        "1"
                                    }
                                    aria-label={`${entry.baseRow.recipient} 分配给 ${slice.shortLabel} 的数量（${unit}）`}
                                    className="num h-6 w-16 bg-card text-right text-xs"
                                    value={slice.quantity}
                                    onChange={(event) =>
                                        onSetSliceQty(
                                            slice.rowKey,
                                            event.target.value,
                                        )
                                    }
                                    onBlur={() =>
                                        normalizeOnBlur(
                                            slice.rowKey,
                                            slice.quantity,
                                        )
                                    }
                                />
                                <span className="text-muted-foreground">
                                    {unit}
                                </span>
                                <Button
                                    id={`${rowId}-slice-${toAutomationIdSegment(slice.rowKey)}-remove`}
                                    type="button"
                                    variant="ghost"
                                    size="icon-xs"
                                    aria-label={`移除 ${slice.shortLabel} 对 ${entry.baseRow.recipient} 的分配`}
                                    onClick={() =>
                                        onSetSliceQty(slice.rowKey, "0", true)
                                    }
                                >
                                    <XIcon />
                                </Button>
                            </div>
                        </div>
                    ))}
                </div>
            ) : null}

            {/* 行内错误提示 */}
            {errorMessages.length > 0 ? (
                <div
                    role="alert"
                    className="flex items-center gap-1.5 px-4 pb-2 pl-11 text-xs text-destructive sm:px-5 sm:pl-12"
                >
                    <CircleAlertIcon
                        aria-hidden="true"
                        className="size-3.5 shrink-0"
                    />
                    <span>{errorMessages.join("；")}</span>
                </div>
            ) : null}
        </div>
    )
}
