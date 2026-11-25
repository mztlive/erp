"use client"

import { useMemo, useState } from "react"
import {
    CheckCircle2Icon,
    ChevronDownIcon,
    ChevronLeftIcon,
    ChevronRightIcon,
    CircleAlertIcon,
    MapPinIcon,
    PackageCheckIcon,
    ScissorsIcon,
    SearchIcon,
    SparklesIcon,
    SplitIcon,
    TriangleAlertIcon,
    WarehouseIcon,
    XIcon,
    ZapIcon,
} from "lucide-react"
import {
    OptionCombobox,
    QuantityValue,
    sourceAccentForIndex,
} from "@/components/business"
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
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compactFixed, compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { PurchaseOrderCreateFormApi } from "../lib/purchase-order-create-form-types"
import { sourcingQuantityStep } from "../lib/sourcing-quantity"
import {
    buildAssignDirectWithWarehouseBalancePatches,
    buildAutoAllocateLowestPricePatches,
    buildClearAddressesPatches,
    buildSetSliceQuantityPatch,
    buildTransferAddressesToWarehousePoolPatches,
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
    addressDeliveryTerm,
    deliveryTermLabel,
} from "../lib/sourcing/delivery-terms"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import { quantityUnits } from "../lib/sourcing/quantity"
import type { PurchaseDeliveryAllocation } from "../types"

export type AddressSourceFilter =
    | null
    | "__warehouse__"
    | "__unassigned__"
    | "__issues__"
    | string

type PageSizeOption = 25 | 50 | 100

const WAREHOUSE_SELECT_VALUE = "__warehouse__"
const UNASSIGNED_SELECT_VALUE = "__unassigned__"

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
 * 右侧常驻交付地址路由表（方案 B）：
 * - 默认展示「全部地址」，一眼看清每处地址归哪个直发供应商还是仓库统筹，行内 1 次点击直接切换归属并自动守恒配平；
 * - 顶部来源胶囊与左侧已选来源卡片双向同步：点击任意左侧来源（或顶部胶囊），右侧立即筛选只看该源承接的地址；
 * - 常规单件/整址归单一来源时隐藏无意义数字框，仅多件可拆地址支持按需展开多源拆分；
 * - 一键「按低价自动分」自动按价格最低 + 包邮优先分配直发，其余自动落仓库统筹。
 */
export function SourcingAddressEditor({
    form,
    row,
    sourceFilter,
    onSourceFilterChange,
    previewDirectRowKey,
    hasUnusedCandidates,
    onRescueAddresses,
}: {
    form: PurchaseOrderCreateFormApi
    row: SourcingEditorRow
    sourceFilter: AddressSourceFilter
    onSourceFilterChange: (next: AddressSourceFilter) => void
    /** 刚新选入但尚无地址的直发源，用于在全局视图下提示该源的配送状态。 */
    previewDirectRowKey?: string | null
    hasUnusedCandidates: boolean
    onRescueAddresses?: (label: string, keys: ReadonlySet<string>) => void
}) {
    const { product } = row
    const allLines = form.state.values.lines
    const idPrefix = `sourcing-addr-${toAutomationIdSegment(product.salesOrderLineId)}`

    const [regionFilter, setRegionFilter] = useState<string | null>(null)
    const [query, setQuery] = useState("")
    const [selectedKeys, setSelectedKeys] = useState<ReadonlySet<string>>(
        () => new Set(),
    )
    const [expandedSplitKeys, setExpandedSplitKeys] = useState<
        ReadonlySet<string>
    >(() => new Set())
    const [pageIndex, setPageIndex] = useState(0)
    const [pageSize, setPageSize] = useState<PageSizeOption>(25)

    // 按数量凑单弹窗状态（单一入口）
    const [quotaOpen, setQuotaOpen] = useState(false)
    const [quotaTargetRowKey, setQuotaTargetRowKey] = useState<string>("")
    const [quotaQty, setQuotaQty] = useState("")
    const [quotaPreferFree, setQuotaPreferFree] = useState(true)
    const [quotaAllowReallocate, setQuotaAllowReallocate] = useState(true)
    const [quotaScopeOnlyFiltered, setQuotaScopeOnlyFiltered] = useState(false)
    const [actionFeedback, setActionFeedback] = useState<string | null>(null)

    const state = useMemo(
        () => computeGlobalAddressState(product, row.allocations),
        [product, row.allocations],
    )

    const activeDirectSources = useMemo(
        () => state.directSources.filter((src) => src.selected),
        [state.directSources],
    )
    const activeNonDirectSources = useMemo(
        () => state.nonDirectSources.filter((src) => src.selected),
        [state.nonDirectSources],
    )
    const primaryNonDirectSource = activeNonDirectSources[0]
    const previewDirectSource = useMemo(
        () =>
            previewDirectRowKey
                ? activeDirectSources.find(
                      (src) => src.rowKey === previewDirectRowKey,
                  )
                : undefined,
        [activeDirectSources, previewDirectRowKey],
    )

    // 规范化来源筛选：若筛选的直发源已被移除或状态不再适用，自动回落到全部
    const resolvedSourceFilter: AddressSourceFilter = (() => {
        if (!sourceFilter) return null
        if (sourceFilter === "__warehouse__") {
            return state.hasNonDirectSources ? "__warehouse__" : null
        }
        if (sourceFilter === "__unassigned__") {
            return !state.hasNonDirectSources && state.counts.unassigned > 0
                ? "__unassigned__"
                : null
        }
        if (sourceFilter === "__issues__") {
            return state.counts.issues > 0 ? "__issues__" : null
        }
        return state.directSources.some((src) => src.rowKey === sourceFilter)
            ? sourceFilter
            : null
    })()

    const filteredDirectSource = useMemo(
        () =>
            resolvedSourceFilter && !resolvedSourceFilter.startsWith("__")
                ? state.directSources.find(
                      (src) => src.rowKey === resolvedSourceFilter,
                  )
                : undefined,
        [resolvedSourceFilter, state.directSources],
    )

    const activeRegionFilter =
        regionFilter &&
        state.regionFacets.items.some((item) => item.key === regionFilter)
            ? regionFilter
            : null

    const filteredEntries = useMemo(() => {
        const keyword = query.trim().toLocaleLowerCase()
        return state.entries.filter((entry) => {
            if (resolvedSourceFilter === "__warehouse__") {
                if (!isPositiveQuantity(entry.remainingUnassigned)) return false
            } else if (resolvedSourceFilter === "__unassigned__") {
                if (!isPositiveQuantity(entry.remainingUnassigned)) return false
            } else if (resolvedSourceFilter === "__issues__") {
                if (
                    !entry.hasError &&
                    !entry.hasNotDeliverable &&
                    !entry.hasFreightExtra
                ) {
                    return false
                }
            } else if (resolvedSourceFilter) {
                if (
                    !entry.activeSlices.some(
                        (slice) => slice.rowKey === resolvedSourceFilter,
                    )
                ) {
                    return false
                }
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
        query,
        resolvedSourceFilter,
        state.entries,
        state.regionFacets.level,
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

    const filteredKeySet = useMemo(
        () => new Set(filteredEntries.map((entry) => entry.key)),
        [filteredEntries],
    )

    const hasActiveFilters =
        resolvedSourceFilter !== null ||
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

    const toggleKeyGroupSelection = (
        keys: readonly string[],
        checked: boolean,
    ) => {
        setSelectedKeys((current) => {
            const next = new Set(current)
            for (const key of keys) {
                if (checked) next.add(key)
                else next.delete(key)
            }
            return next
        })
    }

    const toggleSplitRow = (key: string) => {
        setExpandedSplitKeys((current) => {
            const next = new Set(current)
            if (next.has(key)) next.delete(key)
            else next.add(key)
            return next
        })
    }

    const clearSelection = () => setSelectedKeys(new Set())

    const handleAssignDirect = (
        targetRowKey: string,
        keys: ReadonlySet<string>,
        showFeedback = false,
    ) => {
        const result = buildAssignDirectWithWarehouseBalancePatches(
            product,
            form.state.values.lines,
            targetRowKey,
            keys,
        )
        applyPatches(result.patches)
        if (showFeedback) {
            const target = state.directSources.find(
                (src) => src.rowKey === targetRowKey,
            )
            const parts: string[] = [
                `已将 ${result.assignedCount} 处地址分配给 ${target?.shortLabel ?? "所选直发源"}`,
            ]
            if (isPositiveQuantity(result.autoReducedWarehouseQty)) {
                parts.push(
                    `仓库统筹已自动同步扣减 ${result.autoReducedWarehouseQty} ${product.unit}`,
                )
            }
            if (result.skippedUndeliverableCount > 0) {
                parts.push(
                    `跳过 ${result.skippedUndeliverableCount} 处不配送地址`,
                )
            }
            setActionFeedback(`${parts.join("，")}。`)
            clearSelection()
        }
    }

    const handleAssignWarehouse = (
        keys: ReadonlySet<string>,
        preferredNonDirectRowKey?: string,
        showFeedback = false,
    ) => {
        const result = buildTransferAddressesToWarehousePoolPatches(
            product,
            form.state.values.lines,
            keys,
            preferredNonDirectRowKey,
        )
        applyPatches(result.patches)
        if (showFeedback) {
            const targetLabel =
                activeNonDirectSources.length === 1 && primaryNonDirectSource
                    ? `源${primaryNonDirectSource.sourceNumber}·${primaryNonDirectSource.routeLabel}`
                    : "仓库/库存统筹"
            const capNote = result.cappedByMax ? "（已按仓库可用上限封顶）" : ""
            setActionFeedback(
                `已将 ${result.releasedCount} 处地址划入 ${targetLabel}${isPositiveQuantity(result.transferredQty) ? `（+${result.transferredQty} ${product.unit}）` : ""}${capNote}。`,
            )
            clearSelection()
        }
    }

    const handleClearDirect = (
        keys: ReadonlySet<string>,
        showFeedback = false,
    ) => {
        const patches = buildClearAddressesPatches(
            product,
            form.state.values.lines,
            keys,
        )
        applyPatches(patches)
        if (showFeedback) {
            setActionFeedback(`已将 ${keys.size} 处地址移出直发分配。`)
            clearSelection()
        }
    }

    const handleAutoAllocateLowestPrice = () => {
        const result = buildAutoAllocateLowestPricePatches(
            product,
            form.state.values.lines,
        )
        if (result.patches.length === 0) return
        applyPatches(result.patches)
        onSourceFilterChange(null)
        const summaryParts: string[] = []
        if (result.directAddressCount > 0) {
            summaryParts.push(`直发 ${result.directAddressCount} 处`)
        }
        if (result.warehouseAddressCount > 0) {
            summaryParts.push(`仓库统筹 ${result.warehouseAddressCount} 处`)
        }
        if (result.unassignedCount > 0) {
            summaryParts.push(`待配 ${result.unassignedCount} 处`)
        }
        setActionFeedback(
            `已按低价包邮优先自动分配（${summaryParts.join("，")}）。`,
        )
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
            `已自动缩减多余直发 ${res.trimmedQty} ${product.unit}（释放 ${res.releasedAddressCount} 处地址）。`,
        )
    }

    const resolvedQuotaTarget =
        (quotaTargetRowKey &&
        activeDirectSources.some((src) => src.rowKey === quotaTargetRowKey)
            ? quotaTargetRowKey
            : (filteredDirectSource?.rowKey ??
              state.primaryDirectSource?.rowKey)) ?? ""

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
                    activeDirectSources.length > 1 && quotaAllowReallocate,
            },
        )
    }, [
        activeDirectSources.length,
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
    ])

    const notDeliverableEntries = useMemo(
        () => state.entries.filter((entry) => entry.hasNotDeliverable),
        [state.entries],
    )
    const unassignedEntries = useMemo(
        () =>
            state.entries.filter((entry) =>
                isPositiveQuantity(entry.remainingUnassigned),
            ),
        [state.entries],
    )

    const warehousePillLabel =
        activeNonDirectSources.length === 1 && primaryNonDirectSource
            ? `源${primaryNonDirectSource.sourceNumber}·${primaryNonDirectSource.isStock ? "现货" : "入仓"}`
            : "仓库统筹"
    const warehouseAccent =
        activeNonDirectSources.length === 1 && primaryNonDirectSource
            ? sourceAccentForIndex(primaryNonDirectSource.sourceNumber - 1)
            : sourceAccentForIndex(1)

    const hasDirectOverTarget = isPositiveQuantity(state.directOverTargetQty)
    const canAutoLowestPrice =
        activeDirectSources.length > 1 ||
        (activeDirectSources.length >= 1 && state.hasNonDirectSources)

    const rescueTarget = useMemo(() => {
        if (!onRescueAddresses || !hasUnusedCandidates) return null
        if (notDeliverableEntries.length > 0) {
            return {
                label: `${notDeliverableEntries.length} 处不配送地址`,
                keys: new Set(notDeliverableEntries.map((e) => e.key)),
            }
        }
        if (!state.hasNonDirectSources && unassignedEntries.length > 0) {
            return {
                label: `${unassignedEntries.length} 处待分配地址`,
                keys: new Set(unassignedEntries.map((e) => e.key)),
            }
        }
        return null
    }, [
        hasUnusedCandidates,
        notDeliverableEntries,
        onRescueAddresses,
        state.hasNonDirectSources,
        unassignedEntries,
    ])

    const minStepStr = sourcingQuantityStep(product.quantityScale) ?? "1"
    const minStepUnits = quantityUnits(minStepStr)
    const totalRouteTargets =
        activeDirectSources.length + (state.hasNonDirectSources ? 1 : 0)
    const rowGridClass =
        totalRouteTargets === 3
            ? "grid-cols-[1.75rem_minmax(0,1fr)_4.75rem_4.25rem_16.5rem]"
            : "grid-cols-[1.75rem_minmax(0,1fr)_5rem_4.75rem_13.5rem]"

    return (
        <section
            className="flex min-h-0 flex-1 flex-col overflow-hidden bg-card"
            aria-label="交付地址分配"
        >
            {/* 顶部来源筛选胶囊条（与左侧已选来源卡双向同步：默认全部地址，点源只看该源自己的） */}
            <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/70 bg-surface-sunken/50 px-5 py-2.5">
                <div
                    role="group"
                    aria-label="按承接来源筛选交付地址"
                    className="flex flex-wrap items-center gap-1.5"
                >
                    <button
                        id={`${idPrefix}-filter-source-all`}
                        type="button"
                        aria-pressed={resolvedSourceFilter === null}
                        onClick={() => {
                            onSourceFilterChange(null)
                            resetPage()
                        }}
                        className={cn(
                            "num inline-flex h-6.5 cursor-pointer items-center gap-1.5 rounded-md border px-2.5 text-xs font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                            resolvedSourceFilter === null
                                ? "border-primary/40 bg-card text-primary shadow-2xs"
                                : "border-transparent bg-card/60 text-muted-foreground hover:bg-card hover:text-foreground",
                        )}
                    >
                        <span>全部地址</span>
                        <span className="opacity-75">{state.counts.total}</span>
                    </button>

                    {activeDirectSources.map((src) => {
                        const active = resolvedSourceFilter === src.rowKey
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
                                    onSourceFilterChange(
                                        active ? null : src.rowKey,
                                    )
                                    resetPage()
                                }}
                                className={cn(
                                    "num inline-flex h-6.5 cursor-pointer items-center gap-1.5 rounded-md border px-2.5 text-xs font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                    active
                                        ? accent.chipClass
                                        : "border-transparent bg-card/60 text-muted-foreground hover:bg-card hover:text-foreground",
                                )}
                            >
                                <span
                                    aria-hidden="true"
                                    className={cn(
                                        "size-1.5 rounded-full",
                                        accent.barClass,
                                    )}
                                />
                                <span>
                                    源{src.sourceNumber}·
                                    {src.routeLabel === "集采直发"
                                        ? "集采直发"
                                        : "直发"}
                                </span>
                                <span className="opacity-75">
                                    {src.assignedAddressCount}
                                </span>
                            </button>
                        )
                    })}

                    {state.hasNonDirectSources ? (
                        <button
                            id={`${idPrefix}-filter-source-warehouse`}
                            type="button"
                            aria-pressed={
                                resolvedSourceFilter === "__warehouse__"
                            }
                            onClick={() => {
                                onSourceFilterChange(
                                    resolvedSourceFilter === "__warehouse__"
                                        ? null
                                        : "__warehouse__",
                                )
                                resetPage()
                            }}
                            className={cn(
                                "num inline-flex h-6.5 cursor-pointer items-center gap-1.5 rounded-md border px-2.5 text-xs font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                resolvedSourceFilter === "__warehouse__"
                                    ? warehouseAccent.chipClass
                                    : "border-transparent bg-card/60 text-muted-foreground hover:bg-card hover:text-foreground",
                            )}
                        >
                            <span
                                aria-hidden="true"
                                className={cn(
                                    "size-1.5 rounded-full",
                                    warehouseAccent.barClass,
                                )}
                            />
                            <span>{warehousePillLabel}</span>
                            <span className="opacity-75">
                                {state.counts.unassigned}
                            </span>
                            {!state.directBalanced &&
                            isPositiveQuantity(state.directShortfallQty) ? (
                                <span className="rounded-sm bg-warning-soft px-1 text-tiny text-warning-soft-foreground">
                                    缺{state.directShortfallQty}
                                </span>
                            ) : null}
                        </button>
                    ) : state.counts.unassigned > 0 ? (
                        <button
                            id={`${idPrefix}-filter-source-unassigned`}
                            type="button"
                            aria-pressed={
                                resolvedSourceFilter === "__unassigned__"
                            }
                            onClick={() => {
                                onSourceFilterChange(
                                    resolvedSourceFilter === "__unassigned__"
                                        ? null
                                        : "__unassigned__",
                                )
                                resetPage()
                            }}
                            className={cn(
                                "num inline-flex h-6.5 cursor-pointer items-center gap-1.5 rounded-md border px-2.5 text-xs font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                resolvedSourceFilter === "__unassigned__"
                                    ? "border-warning-border bg-warning-soft text-warning-soft-foreground"
                                    : "border-transparent bg-warning-soft/55 text-warning-soft-foreground hover:bg-warning-soft",
                            )}
                        >
                            <span>待分配</span>
                            <span>{state.counts.unassigned}</span>
                        </button>
                    ) : null}

                    {state.counts.issues > 0 ? (
                        <button
                            id={`${idPrefix}-filter-source-issues`}
                            type="button"
                            aria-pressed={resolvedSourceFilter === "__issues__"}
                            onClick={() => {
                                onSourceFilterChange(
                                    resolvedSourceFilter === "__issues__"
                                        ? null
                                        : "__issues__",
                                )
                                resetPage()
                            }}
                            className={cn(
                                "num inline-flex h-6.5 cursor-pointer items-center gap-1 rounded-md border px-2.5 text-xs font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                resolvedSourceFilter === "__issues__"
                                    ? "border-destructive-border bg-destructive-soft text-destructive-soft-foreground"
                                    : "border-transparent bg-destructive-soft/60 text-destructive-soft-foreground hover:bg-destructive-soft",
                            )}
                        >
                            <TriangleAlertIcon
                                aria-hidden="true"
                                className="size-3"
                            />
                            <span>异常 {state.counts.issues}</span>
                        </button>
                    ) : null}
                </div>

                {/* 右侧提效动作：按低价自动分 / 按数量凑单 */}
                <div className="flex flex-wrap items-center gap-1.5">
                    {hasDirectOverTarget ? (
                        <Button
                            id={`${idPrefix}-header-trim-excess`}
                            type="button"
                            size="xs"
                            variant="destructive"
                            onClick={handleTrimDirectExcess}
                        >
                            <ScissorsIcon aria-hidden="true" />
                            缩减多余 {state.directOverTargetQty} {product.unit}
                        </Button>
                    ) : null}

                    {canAutoLowestPrice ? (
                        <Button
                            id={`${idPrefix}-auto-lowest-price`}
                            type="button"
                            size="xs"
                            variant="secondary"
                            title="按已选来源中价格最低且包邮优先自动路由全部地址，不可配送地址自动归仓库统筹"
                            onClick={handleAutoAllocateLowestPrice}
                        >
                            <SparklesIcon aria-hidden="true" />
                            按低价自动分
                        </Button>
                    ) : null}

                    {activeDirectSources.length > 0 ? (
                        <Popover
                            open={quotaOpen}
                            onOpenChange={(next) => {
                                setQuotaOpen(next)
                                if (next) {
                                    setQuotaTargetRowKey(
                                        filteredDirectSource?.rowKey ??
                                            state.primaryDirectSource?.rowKey ??
                                            "",
                                    )
                                    setQuotaScopeOnlyFiltered(hasActiveFilters)
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
                                        className="bg-card"
                                    />
                                }
                            >
                                <ZapIcon aria-hidden="true" />
                                按量凑单
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

                                {activeDirectSources.length > 1 ? (
                                    <div className="space-y-1">
                                        <p className="text-xs font-medium text-foreground/75">
                                            目标直发来源
                                        </p>
                                        <OptionCombobox
                                            id={`${idPrefix}-quota-source`}
                                            size="sm"
                                            allowClear={false}
                                            aria-label="目标直发来源"
                                            className="w-full"
                                            value={resolvedQuotaTarget}
                                            options={activeDirectSources.map(
                                                (src) => ({
                                                    value: src.rowKey,
                                                    label: src.shortLabel,
                                                }),
                                            )}
                                            onValueChange={(next) => {
                                                if (next)
                                                    setQuotaTargetRowKey(next)
                                            }}
                                        />
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
                                        size="sm"
                                        type="number"
                                        inputMode="decimal"
                                        min="0"
                                        step={minStepStr}
                                        placeholder={`例如 ${state.directTargetQty}`}
                                        className="num"
                                        value={quotaQty}
                                        onChange={(event) =>
                                            setQuotaQty(event.target.value)
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
                                                setQuotaPreferFree(c === true)
                                            }
                                        />
                                        优先分配包邮 / 免运费地址
                                    </label>
                                    {activeDirectSources.length > 1 ? (
                                        <label
                                            htmlFor={`${idPrefix}-quota-allow-reallocate`}
                                            className="flex cursor-pointer items-center gap-2 text-xs text-foreground"
                                        >
                                            <Checkbox
                                                id={`${idPrefix}-quota-allow-reallocate`}
                                                checked={quotaAllowReallocate}
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
                                                checked={quotaScopeOnlyFiltered}
                                                onCheckedChange={(c) =>
                                                    setQuotaScopeOnlyFiltered(
                                                        c === true,
                                                    )
                                                }
                                            />
                                            仅对当前筛选的{" "}
                                            {filteredEntries.length} 处地址生效
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
                                                    {quotaPreview.allocatedQty}{" "}
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
                                        onClick={() => setQuotaOpen(false)}
                                    >
                                        取消
                                    </Button>
                                    <Button
                                        id={`${idPrefix}-quota-apply`}
                                        type="button"
                                        size="xs"
                                        disabled={!quotaPreview?.valid}
                                        onClick={() => {
                                            if (!quotaPreview?.valid) return
                                            applyPatches(quotaPreview.patches)
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
                </div>
            </div>

            {/* 配送异常 / 补缺引导条（仅存在不配送/运费另计或可向左侧找方案时出现） */}
            {state.counts.notDeliverable > 0 ||
            state.counts.freightExtra > 0 ? (
                <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-warning-border/70 bg-warning-soft/55 px-5 py-1.5 text-xs text-warning-soft-foreground">
                    <div className="flex flex-wrap items-center gap-1.5">
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
                        {state.hasNonDirectSources &&
                        notDeliverableEntries.length > 0 ? (
                            <Button
                                id={`${idPrefix}-banner-transfer-undeliverable`}
                                type="button"
                                variant="secondary"
                                size="xs"
                                onClick={() =>
                                    handleAssignWarehouse(
                                        new Set(
                                            notDeliverableEntries.map(
                                                (e) => e.key,
                                            ),
                                        ),
                                        undefined,
                                        true,
                                    )
                                }
                            >
                                不配送转交{warehousePillLabel}
                            </Button>
                        ) : null}
                        {rescueTarget && onRescueAddresses ? (
                            <Button
                                id={`${idPrefix}-banner-rescue`}
                                type="button"
                                variant="outline"
                                size="xs"
                                className="bg-card text-foreground"
                                onClick={() =>
                                    onRescueAddresses(
                                        rescueTarget.label,
                                        rescueTarget.keys,
                                    )
                                }
                            >
                                <MapPinIcon aria-hidden="true" />
                                在左侧找可配低价方案
                            </Button>
                        ) : null}
                    </div>
                </div>
            ) : null}

            {/* 多非直发源并存且筛选仓库统筹时，透明说明由哪些库存/入仓共同承接 */}
            {resolvedSourceFilter === "__warehouse__" &&
            activeNonDirectSources.length > 1 ? (
                <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-info-soft/45 px-5 py-1.5 text-xs text-info-soft-foreground">
                    <span>
                        以下 {filteredEntries.length} 处地址不走直发，由{" "}
                        {activeNonDirectSources
                            .map(
                                (s) =>
                                    `源${s.sourceNumber} ${s.routeLabel}（${s.quantity} ${product.unit}）`,
                            )
                            .join(" + ")}{" "}
                        在出库时共同统筹安排。
                    </span>
                </div>
            ) : null}

            {/* 搜索栏 + 轻量地区筛选条 */}
            <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-card px-5 py-2">
                <div className="relative w-56">
                    <SearchIcon
                        aria-hidden="true"
                        className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                    />
                    <Input
                        id={`${idPrefix}-search`}
                        size="sm"
                        aria-label="搜索收件人、电话或收货地址"
                        placeholder="搜收件人、电话或地址"
                        className="bg-card pr-7 pl-8"
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

                {state.regionFacets.level !== "none" ? (
                    <div className="flex min-w-0 flex-1 items-center justify-end gap-1 overflow-x-auto">
                        <span className="inline-flex shrink-0 items-center gap-0.5 text-tiny text-muted-foreground">
                            <MapPinIcon aria-hidden="true" className="size-3" />
                            {state.regionFacets.level === "province"
                                ? "地区"
                                : "区县"}
                        </span>
                        <button
                            id={`${idPrefix}-region-all`}
                            type="button"
                            aria-pressed={!activeRegionFilter}
                            onClick={() => {
                                setRegionFilter(null)
                                resetPage()
                            }}
                            className={cn(
                                "shrink-0 cursor-pointer rounded-sm px-2 py-0.5 text-xs transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                !activeRegionFilter
                                    ? "bg-muted font-medium text-foreground"
                                    : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
                            )}
                        >
                            全部
                        </button>
                        {state.regionFacets.items.map((item) => {
                            const active = activeRegionFilter === item.key
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
                                            active ? null : item.key,
                                        )
                                        resetPage()
                                    }}
                                    className={cn(
                                        "inline-flex shrink-0 cursor-pointer items-center gap-1 rounded-sm px-2 py-0.5 text-xs transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                        active
                                            ? "bg-primary/10 font-medium text-primary"
                                            : hasAlert
                                              ? "bg-warning-soft/70 text-warning-soft-foreground hover:bg-warning-soft"
                                              : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
                                    )}
                                >
                                    <span>{item.label}</span>
                                    <span className="num text-tiny opacity-75">
                                        {item.totalCount}
                                    </span>
                                </button>
                            )
                        })}
                    </div>
                ) : null}
            </div>

            {/* 勾选后的批量操作条 / 操作反馈条 */}
            {effectiveSelectedKeys.size > 0 ? (
                <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-info-border/70 bg-info-soft/55 px-5 py-2 text-xs">
                    <div className="flex items-center gap-2">
                        <span className="num font-medium text-foreground">
                            已选 {effectiveSelectedKeys.size} 处（共{" "}
                            {selectedDemandTotal} {product.unit}）
                        </span>
                        <button
                            id={`${idPrefix}-bulk-clear-selection`}
                            type="button"
                            onClick={clearSelection}
                            className="cursor-pointer text-xs text-muted-foreground hover:text-foreground"
                        >
                            取消选择
                        </button>
                    </div>

                    <div className="flex flex-wrap items-center gap-1.5">
                        {activeDirectSources.length <= 2 ? (
                            activeDirectSources.map((src) => (
                                <Button
                                    key={src.rowKey}
                                    id={`${idPrefix}-bulk-assign-${toAutomationIdSegment(src.rowKey)}`}
                                    type="button"
                                    size="xs"
                                    variant="secondary"
                                    onClick={() =>
                                        handleAssignDirect(
                                            src.rowKey,
                                            effectiveSelectedKeys,
                                            true,
                                        )
                                    }
                                >
                                    分配给 源{src.sourceNumber}
                                </Button>
                            ))
                        ) : (
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
                                    分配给直发源
                                    <ChevronDownIcon aria-hidden="true" />
                                </DropdownMenuTrigger>
                                <DropdownMenuContent
                                    align="end"
                                    className="w-60"
                                >
                                    {activeDirectSources.map((src) => (
                                        <DropdownMenuItem
                                            key={src.rowKey}
                                            id={`${idPrefix}-bulk-reassign-${toAutomationIdSegment(src.rowKey)}`}
                                            onClick={() =>
                                                handleAssignDirect(
                                                    src.rowKey,
                                                    effectiveSelectedKeys,
                                                    true,
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
                        )}

                        {state.hasNonDirectSources ? (
                            <Button
                                id={`${idPrefix}-bulk-transfer-warehouse`}
                                type="button"
                                size="xs"
                                variant="outline"
                                className="bg-card"
                                onClick={() =>
                                    handleAssignWarehouse(
                                        effectiveSelectedKeys,
                                        undefined,
                                        true,
                                    )
                                }
                            >
                                {activeNonDirectSources.length === 1 &&
                                primaryNonDirectSource?.isStock ? (
                                    <PackageCheckIcon aria-hidden="true" />
                                ) : (
                                    <WarehouseIcon aria-hidden="true" />
                                )}
                                交给{warehousePillLabel}
                            </Button>
                        ) : (
                            <Button
                                id={`${idPrefix}-bulk-clear`}
                                type="button"
                                size="xs"
                                variant="ghost"
                                className="text-destructive hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                onClick={() =>
                                    handleClearDirect(
                                        effectiveSelectedKeys,
                                        true,
                                    )
                                }
                            >
                                <XIcon aria-hidden="true" />
                                取消直发
                            </Button>
                        )}

                        {hasUnusedCandidates && onRescueAddresses ? (
                            <Button
                                id={`${idPrefix}-bulk-find-candidates`}
                                type="button"
                                size="xs"
                                variant="outline"
                                className="bg-card"
                                onClick={() =>
                                    onRescueAddresses(
                                        `所选 ${effectiveSelectedKeys.size} 处地址`,
                                        new Set(effectiveSelectedKeys),
                                    )
                                }
                            >
                                <MapPinIcon aria-hidden="true" />
                                为这 {effectiveSelectedKeys.size} 处找方案
                            </Button>
                        ) : null}
                    </div>
                </div>
            ) : actionFeedback ? (
                <div
                    role="status"
                    className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken px-5 py-1.5 text-xs text-muted-foreground"
                >
                    <span className="flex items-center gap-1.5">
                        <CheckCircle2Icon
                            aria-hidden="true"
                            className="size-3.5 shrink-0 text-success-soft-foreground"
                        />
                        {actionFeedback}
                    </span>
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

            {/* 全局地址路由表头 */}
            <div
                className={cn(
                    "grid shrink-0 items-center gap-3 border-b border-border/70 bg-table-header px-5 py-2 text-xs font-medium text-muted-foreground",
                    rowGridClass,
                )}
            >
                <div className="flex items-center justify-center">
                    <NativeCheckbox
                        id={`${idPrefix}-select-page`}
                        aria-label="全选当前页地址"
                        checked={allPageSelected}
                        indeterminate={somePageSelected}
                        disabled={pageEntries.length === 0}
                        onCheckedChange={(checked) =>
                            toggleKeyGroupSelection(
                                pageEntries.map((e) => e.key),
                                checked,
                            )
                        }
                    />
                </div>
                <span>收件人 / 交付地址</span>
                <span>配送判定</span>
                <span className="text-right">数量</span>
                <span className="text-right">分配去向</span>
            </div>

            {/* 地址表体 */}
            <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
                {pageEntries.length === 0 ? (
                    <div className="flex flex-col items-center justify-center gap-2.5 px-6 py-16 text-center text-xs text-muted-foreground">
                        <p>
                            {filteredDirectSource
                                ? `源${filteredDirectSource.sourceNumber}（${filteredDirectSource.supplierName}）当前暂未承接地址`
                                : resolvedSourceFilter === "__warehouse__"
                                  ? "当前全部地址均已走供应商直发，暂无留待仓库统筹的地址"
                                  : "当前筛选条件下没有交付地址"}
                        </p>
                        {hasActiveFilters ? (
                            <Button
                                id={`${idPrefix}-reset-filters`}
                                type="button"
                                variant="outline"
                                size="xs"
                                onClick={() => {
                                    onSourceFilterChange(null)
                                    setRegionFilter(null)
                                    setQuery("")
                                    resetPage()
                                }}
                            >
                                返回全部 {state.counts.total} 处地址
                            </Button>
                        ) : null}
                    </div>
                ) : (
                    <div className="divide-y divide-grid">
                        {pageEntries.map((entry) => (
                            <AddressRouteRow
                                key={entry.key}
                                idPrefix={idPrefix}
                                rowGridClass={rowGridClass}
                                entry={entry}
                                unit={product.unit}
                                minStepStr={minStepStr}
                                minStepUnits={minStepUnits}
                                directSources={activeDirectSources}
                                nonDirectSources={activeNonDirectSources}
                                warehousePillLabel={warehousePillLabel}
                                warehouseAccent={warehouseAccent}
                                previewDirectSource={previewDirectSource}
                                hasNonDirectSources={state.hasNonDirectSources}
                                directBalanced={state.directBalanced}
                                checked={effectiveSelectedKeys.has(entry.key)}
                                splitExpanded={expandedSplitKeys.has(entry.key)}
                                onToggleChecked={(checked) =>
                                    toggleRowSelection(entry.key, checked)
                                }
                                onToggleSplit={() => toggleSplitRow(entry.key)}
                                onAssignDirect={(targetRowKey) =>
                                    handleAssignDirect(
                                        targetRowKey,
                                        new Set([entry.key]),
                                    )
                                }
                                onAssignWarehouse={() =>
                                    handleAssignWarehouse(new Set([entry.key]))
                                }
                                onClearDirect={() =>
                                    handleClearDirect(new Set([entry.key]))
                                }
                                onSetSliceQty={(
                                    targetRowKey,
                                    nextQty,
                                    removeIfZero,
                                ) => {
                                    const patch = buildSetSliceQuantityPatch(
                                        product,
                                        form.state.values.lines,
                                        targetRowKey,
                                        entry.baseRow,
                                        nextQty,
                                        removeIfZero,
                                    )
                                    if (patch) applyPatches([patch])
                                }}
                            />
                        ))}
                    </div>
                )}
            </div>

            {/* 精简底栏（仅保留计数与大数量分页，移除重复的直发额度术语） */}
            <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-border/60 bg-surface-sunken/70 px-5 py-2 text-xs text-muted-foreground">
                <span className="num">
                    {filteredEntries.length === 0
                        ? "共 0 处地址"
                        : filteredEntries.length > pageSize
                          ? `第 ${pageStart + 1}–${Math.min(pageStart + pageSize, filteredEntries.length)} 处，共 ${filteredEntries.length} 处`
                          : hasActiveFilters
                            ? `筛选显示 ${filteredEntries.length} / 共 ${state.counts.total} 处地址`
                            : `共 ${state.counts.total} 处地址`}
                </span>

                {filteredEntries.length > 25 ? (
                    <div className="flex items-center gap-2">
                        <div className="flex items-center gap-1.5">
                            <span>每页</span>
                            <OptionCombobox
                                id={`${idPrefix}-page-size`}
                                size="sm"
                                allowClear={false}
                                aria-label="每页显示地址条数"
                                className="w-24"
                                inputClassName="bg-card"
                                value={String(pageSize)}
                                options={[
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
                            />
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
                                    setPageIndex((p) => Math.max(0, p - 1))
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
                                disabled={safePageIndex >= totalPages - 1}
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
        </section>
    )
}

function AddressRouteRow({
    idPrefix,
    rowGridClass,
    entry,
    unit,
    minStepStr,
    minStepUnits,
    directSources,
    nonDirectSources,
    warehousePillLabel,
    warehouseAccent,
    previewDirectSource,
    hasNonDirectSources,
    directBalanced,
    checked,
    splitExpanded,
    onToggleChecked,
    onToggleSplit,
    onAssignDirect,
    onAssignWarehouse,
    onClearDirect,
    onSetSliceQty,
}: {
    idPrefix: string
    rowGridClass: string
    entry: GlobalAddressEntry
    unit: string
    minStepStr: string
    minStepUnits: bigint
    directSources: readonly DirectSourceInfo[]
    nonDirectSources: readonly NonDirectSourceInfo[]
    warehousePillLabel: string
    warehouseAccent: ReturnType<typeof sourceAccentForIndex>
    previewDirectSource?: DirectSourceInfo
    hasNonDirectSources: boolean
    directBalanced: boolean
    checked: boolean
    splitExpanded: boolean
    onToggleChecked: (checked: boolean) => void
    onToggleSplit: () => void
    onAssignDirect: (targetRowKey: string) => void
    onAssignWarehouse: () => void
    onClearDirect: () => void
    onSetSliceQty: (
        targetRowKey: string,
        nextQty: string,
        removeIfZero?: boolean,
    ) => void
}) {
    const rowId = `${idPrefix}-${addressIdSegment(entry.baseRow)}`
    const activeSlices = entry.activeSlices
    const hasAssignedDirect = activeSlices.length > 0
    const isPartialOrSplit =
        activeSlices.length > 1 ||
        (activeSlices.length === 1 &&
            isPositiveQuantity(entry.remainingUnassigned))
    const singleDirectSlice =
        activeSlices.length === 1 && !isPartialOrSplit
            ? activeSlices[0]
            : undefined
    const isWarehouseOwned = !hasAssignedDirect && hasNonDirectSources

    const demandUnits = (() => {
        try {
            return quantityUnits(entry.demandQty)
        } catch {
            return BigInt(0)
        }
    })()
    const canSplitAddress =
        demandUnits > minStepUnits &&
        (directSources.length >= 2 || hasNonDirectSources)
    const showSplitDrawer = isPartialOrSplit || splitExpanded

    // 配送判定展示：已分直发时看实际直发源；未分直发时若有刚选的新直发源则预览它，否则看主直发源
    const termSource = singleDirectSlice
        ? directSources.find((s) => s.rowKey === singleDirectSlice.rowKey)
        : (previewDirectSource ?? directSources[0])
    const displayTerm = termSource
        ? addressDeliveryTerm(termSource.option, entry.baseRow.region_code)
        : entry.effectiveTerm

    const errorMessages = [
        ...new Set(
            entry.slices
                .map((slice) => slice.error)
                .filter((msg): msg is string => Boolean(msg)),
        ),
    ]

    const totalRouteTargets =
        directSources.length + (hasNonDirectSources ? 1 : 0)

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
            // 保留非法输入交由校验提示
        }
    }

    const selectCurrentValue = singleDirectSlice
        ? singleDirectSlice.rowKey
        : isWarehouseOwned
          ? WAREHOUSE_SELECT_VALUE
          : UNASSIGNED_SELECT_VALUE

    return (
        <div
            className={cn(
                "transition-colors hover:bg-row-hover",
                entry.hasError &&
                    "bg-destructive-soft/30 hover:bg-destructive-soft/45",
            )}
        >
            <div
                className={cn(
                    "grid items-center gap-3 px-5 py-2",
                    rowGridClass,
                )}
            >
                <div className="flex items-center justify-center">
                    <NativeCheckbox
                        id={`${rowId}-select`}
                        checked={checked}
                        aria-label={`选择 ${entry.baseRow.recipient}`}
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

                {/* 配送判定 */}
                <div>
                    {termSource && displayTerm ? (
                        <StatusBadge
                            tone={
                                displayTerm === "NOT_DELIVERABLE"
                                    ? "destructive"
                                    : displayTerm === "FREIGHT_EXTRA"
                                      ? "warning"
                                      : "neutral"
                            }
                            label={deliveryTermLabel(
                                termSource.option,
                                displayTerm,
                            )}
                            className="h-5 px-1.5 text-tiny"
                        />
                    ) : (
                        <span className="text-xs text-muted-foreground">—</span>
                    )}
                </div>

                {/* 需求量（单件直接静态显示，多件可拆地址才展示极简「拆」按钮） */}
                <div className="flex items-center justify-end gap-1">
                    <span className="num text-xs font-medium text-foreground">
                        <QuantityValue value={entry.demandQty} unit={unit} />
                    </span>
                    {canSplitAddress ? (
                        <button
                            id={`${rowId}-split-toggle`}
                            type="button"
                            aria-expanded={showSplitDrawer}
                            title="将此地址的数量拆给多个来源"
                            onClick={onToggleSplit}
                            className={cn(
                                "inline-flex size-5 cursor-pointer items-center justify-center rounded-sm transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                showSplitDrawer
                                    ? "bg-primary/12 text-primary"
                                    : "text-muted-foreground hover:bg-muted hover:text-foreground",
                            )}
                        >
                            <SplitIcon className="size-3" />
                        </button>
                    ) : null}
                </div>

                {/* 1-click 分配去向：2–3 个来源用分段按钮一步切换，4+ 来源用下拉 */}
                <div className="flex justify-end">
                    {isPartialOrSplit ? (
                        <Badge
                            variant="secondary"
                            className="num h-6 px-2 text-tiny"
                        >
                            已拆分（直发 {entry.assignedTotal}/{entry.demandQty}
                            ）
                        </Badge>
                    ) : totalRouteTargets >= 2 && totalRouteTargets <= 3 ? (
                        <div
                            role="group"
                            aria-label={`${entry.baseRow.recipient} 分配去向`}
                            className="inline-flex items-center gap-0.5 rounded-md border border-border/75 bg-surface-sunken/70 p-0.5"
                        >
                            {directSources.map((src) => {
                                const active =
                                    singleDirectSlice?.rowKey === src.rowKey
                                const accent = sourceAccentForIndex(
                                    src.sourceNumber - 1,
                                )
                                const deliverable = addressDeliverable(
                                    src.option,
                                    entry.baseRow.region_code,
                                )
                                return (
                                    <button
                                        key={src.rowKey}
                                        id={`${rowId}-pill-${toAutomationIdSegment(src.rowKey)}`}
                                        type="button"
                                        aria-pressed={active}
                                        disabled={!deliverable && !active}
                                        title={
                                            !deliverable
                                                ? `${src.supplierName} 不配送此地区`
                                                : `分配给 源${src.sourceNumber} · ${src.supplierName}（${src.routeLabel}）`
                                        }
                                        onClick={() => {
                                            if (!active) {
                                                onAssignDirect(src.rowKey)
                                            }
                                        }}
                                        className={cn(
                                            "num inline-flex h-6 items-center gap-1 rounded-sm border px-2 text-tiny font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                            active
                                                ? accent.chipClass
                                                : !deliverable
                                                  ? "cursor-not-allowed border-transparent text-muted-foreground/45 line-through"
                                                  : "cursor-pointer border-transparent text-muted-foreground hover:bg-card hover:text-foreground",
                                        )}
                                    >
                                        {active ? (
                                            <span
                                                aria-hidden="true"
                                                className={cn(
                                                    "size-1.5 rounded-full",
                                                    accent.barClass,
                                                )}
                                            />
                                        ) : null}
                                        <span>源{src.sourceNumber}·直发</span>
                                    </button>
                                )
                            })}

                            {hasNonDirectSources ? (
                                <button
                                    id={`${rowId}-pill-warehouse`}
                                    type="button"
                                    aria-pressed={isWarehouseOwned}
                                    title={`交给 ${warehousePillLabel}（同步自动配平仓库数量）`}
                                    onClick={() => {
                                        if (
                                            !isWarehouseOwned ||
                                            !directBalanced
                                        ) {
                                            onAssignWarehouse()
                                        }
                                    }}
                                    className={cn(
                                        "num inline-flex h-6 cursor-pointer items-center gap-1 rounded-sm border px-2 text-tiny font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                        isWarehouseOwned
                                            ? warehouseAccent.chipClass
                                            : "border-transparent text-muted-foreground hover:bg-card hover:text-foreground",
                                    )}
                                >
                                    {isWarehouseOwned ? (
                                        <span
                                            aria-hidden="true"
                                            className={cn(
                                                "size-1.5 rounded-full",
                                                warehouseAccent.barClass,
                                            )}
                                        />
                                    ) : null}
                                    <span>{warehousePillLabel}</span>
                                </button>
                            ) : null}
                        </div>
                    ) : totalRouteTargets === 1 && directSources[0] ? (
                        (() => {
                            const only = directSources[0]
                            const active =
                                singleDirectSlice?.rowKey === only.rowKey
                            const accent = sourceAccentForIndex(
                                only.sourceNumber - 1,
                            )
                            const deliverable = addressDeliverable(
                                only.option,
                                entry.baseRow.region_code,
                            )
                            if (!deliverable && !active) {
                                return (
                                    <span className="text-xs text-destructive">
                                        源{only.sourceNumber} 不配送
                                    </span>
                                )
                            }
                            return (
                                <button
                                    id={`${rowId}-toggle-assign`}
                                    type="button"
                                    aria-pressed={active}
                                    onClick={() =>
                                        active
                                            ? onClearDirect()
                                            : onAssignDirect(only.rowKey)
                                    }
                                    className={cn(
                                        "num inline-flex h-6.5 cursor-pointer items-center gap-1.5 rounded-md border px-2.5 text-xs font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                        active
                                            ? accent.chipClass
                                            : "border-dashed border-border bg-card text-muted-foreground hover:border-primary/45 hover:text-primary",
                                    )}
                                >
                                    <span
                                        aria-hidden="true"
                                        className={cn(
                                            "size-1.5 rounded-full",
                                            active
                                                ? accent.barClass
                                                : "bg-muted-foreground/40",
                                        )}
                                    />
                                    <span>
                                        {active
                                            ? `源${only.sourceNumber}·直发`
                                            : `分配给 源${only.sourceNumber}`}
                                    </span>
                                </button>
                            )
                        })()
                    ) : (
                        <OptionCombobox
                            id={`${rowId}-source`}
                            size="sm"
                            allowClear={false}
                            aria-label={`${entry.baseRow.recipient} 分配去向`}
                            className="w-48"
                            inputClassName="bg-card"
                            value={selectCurrentValue}
                            options={[
                                ...directSources.map((src) => {
                                    const term = addressDeliveryTerm(
                                        src.option,
                                        entry.baseRow.region_code,
                                    )
                                    const undeliverable =
                                        term === "NOT_DELIVERABLE"
                                    const suffix = undeliverable
                                        ? "（不配送）"
                                        : term === "FREIGHT_EXTRA"
                                          ? "（运费另计）"
                                          : ""
                                    return {
                                        value: src.rowKey,
                                        label: `${src.shortLabel}${suffix}`,
                                        disabled:
                                            undeliverable &&
                                            singleDirectSlice?.rowKey !==
                                                src.rowKey,
                                    }
                                }),
                                ...(hasNonDirectSources
                                    ? [
                                          {
                                              value: WAREHOUSE_SELECT_VALUE,
                                              label: `${warehousePillLabel}${
                                                  nonDirectSources.length > 1
                                                      ? "（自动配平）"
                                                      : ""
                                              }`,
                                          },
                                      ]
                                    : [
                                          {
                                              value: UNASSIGNED_SELECT_VALUE,
                                              label: "暂不直发（待分配）",
                                          },
                                      ]),
                            ]}
                            onValueChange={(next) => {
                                if (!next || next === UNASSIGNED_SELECT_VALUE) {
                                    onClearDirect()
                                    return
                                }
                                if (next === WAREHOUSE_SELECT_VALUE) {
                                    onAssignWarehouse()
                                    return
                                }
                                onAssignDirect(next)
                            }}
                        />
                    )}
                </div>
            </div>

            {/* 多件地址按需展开的多来源拆分行 */}
            {showSplitDrawer ? (
                <div className="flex flex-wrap items-center gap-3 border-t border-border/50 bg-surface-sunken/45 px-12 py-2 text-xs">
                    <span className="text-muted-foreground">
                        按源拆分数量：
                    </span>
                    {directSources.map((src) => {
                        const existing = entry.slices.find(
                            (s) => s.rowKey === src.rowKey,
                        )
                        const deliverable = addressDeliverable(
                            src.option,
                            entry.baseRow.region_code,
                        )
                        const accent = sourceAccentForIndex(
                            src.sourceNumber - 1,
                        )
                        return (
                            <label
                                key={src.rowKey}
                                htmlFor={`${rowId}-split-${toAutomationIdSegment(src.rowKey)}`}
                                className="inline-flex items-center gap-1.5"
                            >
                                <span
                                    aria-hidden="true"
                                    className={cn(
                                        "size-1.5 rounded-full",
                                        accent.barClass,
                                    )}
                                />
                                <span className="num text-foreground">
                                    源{src.sourceNumber}
                                </span>
                                <Input
                                    id={`${rowId}-split-${toAutomationIdSegment(src.rowKey)}`}
                                    size="sm"
                                    type="number"
                                    inputMode="decimal"
                                    min="0"
                                    max={entry.demandQty}
                                    step={minStepStr}
                                    disabled={!deliverable && !existing}
                                    className="num w-16 bg-card text-right"
                                    value={existing?.quantity ?? ""}
                                    placeholder="0"
                                    onChange={(event) =>
                                        onSetSliceQty(
                                            src.rowKey,
                                            event.target.value,
                                        )
                                    }
                                    onBlur={(event) =>
                                        normalizeOnBlur(
                                            src.rowKey,
                                            event.target.value,
                                        )
                                    }
                                />
                            </label>
                        )
                    })}
                    {isPositiveQuantity(entry.remainingUnassigned) ? (
                        <span className="num text-muted-foreground">
                            {hasNonDirectSources
                                ? `余 ${entry.remainingUnassigned} ${unit} 归${warehousePillLabel}`
                                : `未配 ${entry.remainingUnassigned} ${unit}`}
                        </span>
                    ) : null}
                </div>
            ) : null}

            {/* 行内错误提示 */}
            {errorMessages.length > 0 ? (
                <div className="px-12 pb-2">
                    {errorMessages.map((msg) => (
                        <p
                            key={msg}
                            role="alert"
                            className="flex items-center gap-1 text-xs text-destructive"
                        >
                            <CircleAlertIcon className="size-3 shrink-0" />
                            <span>{msg}</span>
                        </p>
                    ))}
                </div>
            ) : null}
        </div>
    )
}
