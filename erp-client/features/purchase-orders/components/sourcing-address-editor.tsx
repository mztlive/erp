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
import {
    compactFixed,
    compareDecimal,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { PurchaseOrderCreateFormApi } from "../lib/purchase-order-create-form-types"
import { sourcingQuantityStep } from "../lib/sourcing-quantity"
import {
    buildAddSliceQuantityPatch,
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
    addressDeliveryTerm,
    deliveryTermLabel,
} from "../lib/sourcing/delivery-terms"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import {
    formatQuantityUnits,
    minimumUnits,
    quantityUnits,
} from "../lib/sourcing/quantity"
import {
    sourceAccentForIndex,
    sourceRouteBadgeVariant,
} from "../lib/sourcing/source-palette"
import type { PurchaseDeliveryAllocation } from "../types"

type ViewMode = "follow" | "all"
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
 * 右侧全高交付地址分配表（支持「跟随左侧当前源」与「全部地址总览」双视角）：
 * - 跟随模式：点击左侧任意来源（直发 / 集采入仓 / 现有库存），右侧立即切换至该来源视角；
 *   已配地址逐行提供直观「移出」按钮，移出后立刻进入「未覆盖地址池」；
 * - 切换到其他适合的来源（直发或仓发/库存）后，可将未覆盖地址逐条或一键全部直接填入，轻松完成多源配平；
 * - 全部模式：扁平分页表呈现所有地址（适配 100+ 地址），同样保留直达「移出」与「+ 填入」动作。
 */
export function SourcingAddressEditor({
    form,
    row,
    activeRowKey,
    canSplitNewSource,
    onSplitResidual,
    onSplitAddressesToNewSource,
    sourceFilter: controlledSourceFilter,
    onSourceFilterChange,
}: {
    form: PurchaseOrderCreateFormApi
    row: SourcingEditorRow
    activeRowKey?: string
    canSplitNewSource: boolean
    onSplitResidual?: () => void
    onSplitAddressesToNewSource?: (addressKeys: readonly string[]) => void
    sourceFilter?: string | null
    onSourceFilterChange?: (next: string | null) => void
}) {
    const { product } = row
    const allLines = form.state.values.lines
    const idPrefix = `sourcing-addr-${toAutomationIdSegment(product.salesOrderLineId)}`

    const [viewMode, setViewMode] = useState<ViewMode>("follow")
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

    // 当前在左侧选中的来源（可能是直发来源，也可能是集采入仓/现有库存）
    const activeDirectSource = useMemo(
        () =>
            state.directSources.find((src) => src.rowKey === activeRowKey) ??
            undefined,
        [activeRowKey, state.directSources],
    )
    const activeNonDirectSource = useMemo(
        () =>
            state.nonDirectSources.find((src) => src.rowKey === activeRowKey) ??
            undefined,
        [activeRowKey, state.nonDirectSources],
    )
    const effectiveDirectSource =
        activeDirectSource ?? state.primaryDirectSource

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

    // 基础搜索 + 地区筛选（跟随模式与全部模式共用）
    const baseFilteredEntries = useMemo(() => {
        const keyword = query.trim().toLocaleLowerCase()
        return state.entries.filter((entry) => {
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
    }, [activeRegionFilter, query, state.entries, state.regionFacets.level])

    // 全部模式下的进一步状态/来源筛选
    const filteredEntries = useMemo(() => {
        return baseFilteredEntries.filter((entry) => {
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
            return true
        })
    }, [activeSourceFilter, baseFilteredEntries, statusFilter])

    // 跟随模式下的分组数据：
    // 1) 当前选中直发源：本源已承接地址 + 全局未覆盖地址池
    // 2) 当前选中仓库/库存源：未覆盖/仓库统筹地址 + 其他直发源已承接地址（可一键移出或转入本仓源）
    const followAssignedEntries = useMemo(() => {
        if (!activeDirectSource) return []
        return baseFilteredEntries.filter((entry) =>
            entry.slices.some(
                (slice) =>
                    slice.rowKey === activeDirectSource.rowKey &&
                    (slice.positive || slice.quantity.trim() !== ""),
            ),
        )
    }, [activeDirectSource, baseFilteredEntries])

    const followUncoveredEntries = useMemo(
        () =>
            baseFilteredEntries.filter((entry) =>
                isPositiveQuantity(entry.remainingUnassigned),
            ),
        [baseFilteredEntries],
    )

    const followDirectAssignedAll = useMemo(
        () =>
            baseFilteredEntries.filter((entry) =>
                isPositiveQuantity(entry.assignedTotal),
            ),
        [baseFilteredEntries],
    )

    const allUncoveredEntries = useMemo(
        () =>
            state.entries.filter((entry) =>
                isPositiveQuantity(entry.remainingUnassigned),
            ),
        [state.entries],
    )
    const allUncoveredQty = useMemo(
        () =>
            safeSumQuantities(
                allUncoveredEntries.map((entry) => entry.remainingUnassigned),
            ),
        [allUncoveredEntries],
    )

    // 当前选中直发源可承接的未覆盖地址列表（自动排除该源不配送地区）
    const deliverableUncoveredForActive = useMemo(() => {
        if (!activeDirectSource) return []
        return followUncoveredEntries.filter((entry) =>
            addressDeliverable(
                activeDirectSource.option,
                entry.baseRow.region_code,
            ),
        )
    }, [activeDirectSource, followUncoveredEntries])
    const deliverableUncoveredQty = useMemo(
        () =>
            safeSumQuantities(
                deliverableUncoveredForActive.map(
                    (entry) => entry.remainingUnassigned,
                ),
            ),
        [deliverableUncoveredForActive],
    )

    // 当前选中仓库/库存源可承接的未覆盖余量（受商品总缺口与该源可用上限约束）
    const nonDirectFillableQty = useMemo(() => {
        if (!activeNonDirectSource) return "0"
        try {
            const demandUnits = quantityUnits(product.remainingQuantity)
            const allocatedStr = sumFixed(
                row.allocations
                    .filter(({ line }) => line.selected)
                    .map(({ line }) => line.quantity || "0"),
                { maxScale: 6, outputScale: 6 },
            )
            const allocatedUnits = quantityUnits(allocatedStr)
            if (allocatedUnits >= demandUnits) return "0"
            const productShortfall = demandUnits - allocatedUnits
            const selfUnits = isPositiveQuantity(activeNonDirectSource.quantity)
                ? quantityUnits(compactFixed(activeNonDirectSource.quantity))
                : BigInt(0)
            const capUnits = quantityUnits(
                activeNonDirectSource.maxCreateQuantity,
            )
            const headroom =
                capUnits > selfUnits ? capUnits - selfUnits : BigInt(0)
            const addable = minimumUnits(productShortfall, headroom)
            return addable > BigInt(0) ? formatQuantityUnits(addable) : "0"
        } catch {
            return "0"
        }
    }, [activeNonDirectSource, product.remainingQuantity, row.allocations])

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
            `已将 ${result.assignedCount} 处地址填入 ${target?.shortLabel ?? "所选来源"}${skippedNote}。`,
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
            `已从直发移出 ${result.releasedCount} 处地址，并向 ${target?.shortLabel ?? "仓库来源"} 划入 ${result.transferredQty} ${product.unit}${capNote}。`,
        )
        clearSelection()
    }

    const handleBatchClear = (
        keys: ReadonlySet<string>,
        onlyRowKey?: string,
    ) => {
        const patches = buildClearAddressesPatches(
            product,
            form.state.values.lines,
            keys,
            onlyRowKey,
        )
        applyPatches(patches)
        const srcLabel = onlyRowKey
            ? state.directSources.find((s) => s.rowKey === onlyRowKey)
                  ?.shortLabel
            : undefined
        setActionFeedback(
            srcLabel
                ? `已从 ${srcLabel} 移出 ${keys.size} 处地址至未覆盖地址池。`
                : `已移出 ${keys.size} 处地址的直发分配至未覆盖地址池。`,
        )
        clearSelection()
    }

    const handleRemoveAddressFromSource = (
        targetRowKey: string,
        entry: GlobalAddressEntry,
    ) => {
        const patch = buildSetSliceQuantityPatch(
            product,
            form.state.values.lines,
            targetRowKey,
            entry.baseRow,
            "0",
            true,
        )
        if (patch) applyPatches([patch])
    }

    const handleFillUncoveredToDirect = (
        targetRowKey: string,
        entry: GlobalAddressEntry,
    ) => {
        const patch = buildAddSliceQuantityPatch(
            product,
            form.state.values.lines,
            targetRowKey,
            entry.baseRow,
            entry.remainingUnassigned,
        )
        if (patch) applyPatches([patch])
    }

    const handleFillAllUncoveredForActiveDirect = () => {
        if (!activeDirectSource || deliverableUncoveredForActive.length === 0)
            return
        // 逐条累加填充未覆盖余量（保留该源已有分配，不覆盖原切片）
        let workingLines = [...form.state.values.lines]
        let filledCount = 0
        for (const entry of deliverableUncoveredForActive) {
            const patch = buildAddSliceQuantityPatch(
                product,
                workingLines,
                activeDirectSource.rowKey,
                entry.baseRow,
                entry.remainingUnassigned,
            )
            if (!patch) continue
            workingLines = workingLines.map((item, idx) =>
                idx === patch.index ? patch.nextLine : item,
            )
            filledCount += 1
        }
        const targetLine = workingLines[activeDirectSource.index]
        if (targetLine) {
            form.setFieldValue(`lines[${activeDirectSource.index}]`, targetLine)
        }
        const skipped =
            followUncoveredEntries.length - deliverableUncoveredForActive.length
        setActionFeedback(
            `已向 ${activeDirectSource.shortLabel} 填入 ${filledCount} 处未覆盖地址（共 ${deliverableUncoveredQty} ${product.unit}）${skipped > 0 ? `，跳过 ${skipped} 处不配送地址` : ""}。`,
        )
    }

    const handleFillUncoveredToActiveNonDirect = () => {
        if (
            !activeNonDirectSource ||
            !isPositiveQuantity(nonDirectFillableQty)
        ) {
            return
        }
        const currentQty = isPositiveQuantity(activeNonDirectSource.quantity)
            ? compactFixed(activeNonDirectSource.quantity)
            : "0"
        const nextQty = safeSumQuantities([currentQty, nonDirectFillableQty])
        form.setFieldValue(
            `lines[${activeNonDirectSource.index}].selected`,
            true,
        )
        form.setFieldValue(
            `lines[${activeNonDirectSource.index}].quantity`,
            nextQty,
        )
        setActionFeedback(
            `已将未覆盖余量 ${nonDirectFillableQty} ${product.unit} 填入 源${activeNonDirectSource.sourceNumber}·${activeNonDirectSource.routeLabel}（当前合计 ${nextQty} ${product.unit}）。`,
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
            `已自动移出多余直发 ${res.trimmedQty} ${product.unit}（释放 ${res.releasedAddressCount} 处地址）。`,
        )
    }

    const resolvedQuotaTarget =
        (quotaTargetRowKey &&
        state.directSources.some((src) => src.rowKey === quotaTargetRowKey)
            ? quotaTargetRowKey
            : effectiveDirectSource?.rowKey) ?? ""

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

    const unassignedLabel = state.hasNonDirectSources ? "未直发/仓发" : "未覆盖"
    const hasDirectOverTarget = isPositiveQuantity(state.directOverTargetQty)
    const hasDirectShortfall = isPositiveQuantity(state.directShortfallQty)

    const activeAccent = activeDirectSource
        ? sourceAccentForIndex(activeDirectSource.sourceNumber - 1)
        : activeNonDirectSource
          ? sourceAccentForIndex(activeNonDirectSource.sourceNumber - 1)
          : sourceAccentForIndex(0)

    const activeFollowSelectedInAssigned = useMemo(
        () =>
            followAssignedEntries
                .filter((e) => effectiveSelectedKeys.has(e.key))
                .map((e) => e.key),
        [effectiveSelectedKeys, followAssignedEntries],
    )
    const activeFollowSelectedInUncovered = useMemo(
        () =>
            followUncoveredEntries
                .filter((e) => effectiveSelectedKeys.has(e.key))
                .map((e) => e.key),
        [effectiveSelectedKeys, followUncoveredEntries],
    )

    // 用于全部模式中：把当前选中地址直接从所有直发源移出
    const selectedAssignedKeysInAll = useMemo(
        () =>
            selectedEntries
                .filter((e) => isPositiveQuantity(e.assignedTotal))
                .map((e) => e.key),
        [selectedEntries],
    )

    const normalizeOnBlur = (
        targetRowKey: string,
        rawQty: string,
        baseRow: PurchaseDeliveryAllocation,
    ) => {
        const trimmed = rawQty.trim()
        if (trimmed === "") {
            const patch = buildSetSliceQuantityPatch(
                product,
                form.state.values.lines,
                targetRowKey,
                baseRow,
                "0",
                true,
            )
            if (patch) applyPatches([patch])
            return
        }
        try {
            const normalized = compactFixed(trimmed)
            const patch = buildSetSliceQuantityPatch(
                product,
                form.state.values.lines,
                targetRowKey,
                baseRow,
                normalized,
                compareDecimal(normalized, "0", 6) === 0,
            )
            if (patch) applyPatches([patch])
        } catch {
            // 保留非法输入交由行内错误提示
        }
    }

    return (
        <section
            className="flex min-h-0 flex-1 flex-col overflow-hidden bg-card"
            aria-label="本来源交付地址"
        >
            {/* 顶部语境条：左侧跟随当前选中源身份 + 右侧视角切换（跟随当前源 / 全部地址） */}
            <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken/60 px-5 py-2">
                <div className="flex min-w-0 flex-wrap items-center gap-2 text-xs">
                    {activeDirectSource ? (
                        <>
                            <span className="inline-flex items-center gap-1.5 font-medium text-foreground">
                                <span
                                    aria-hidden="true"
                                    className={cn(
                                        "size-2 shrink-0 rounded-full",
                                        activeAccent.barClass,
                                    )}
                                />
                                <span>
                                    当前源：源{activeDirectSource.sourceNumber}{" "}
                                    · {activeDirectSource.supplierName}
                                </span>
                            </span>
                            <Badge
                                variant={sourceRouteBadgeVariant(
                                    activeDirectSource.option,
                                )}
                                className="h-5 px-1.5"
                            >
                                {activeDirectSource.routeLabel}
                            </Badge>
                            <span className="num text-muted-foreground">
                                已承接{" "}
                                <strong className="font-semibold text-foreground">
                                    {activeDirectSource.assignedAddressCount} 处
                                </strong>
                                （{activeDirectSource.assignedTotalQty}{" "}
                                {product.unit}）
                            </span>
                            {allUncoveredEntries.length > 0 ? (
                                <span className="num rounded-sm bg-warning-soft px-1.5 py-0.5 text-tiny font-medium text-warning-soft-foreground">
                                    未覆盖 {allUncoveredEntries.length} 处（
                                    {allUncoveredQty} {product.unit}）
                                </span>
                            ) : null}
                        </>
                    ) : activeNonDirectSource ? (
                        <>
                            <span className="inline-flex items-center gap-1.5 font-medium text-foreground">
                                <span
                                    aria-hidden="true"
                                    className={cn(
                                        "size-2 shrink-0 rounded-full",
                                        activeAccent.barClass,
                                    )}
                                />
                                <span>
                                    当前源：源
                                    {activeNonDirectSource.sourceNumber} ·{" "}
                                    {activeNonDirectSource.name}
                                </span>
                            </span>
                            <Badge
                                variant={sourceRouteBadgeVariant(
                                    activeNonDirectSource.option,
                                )}
                                className="h-5 px-1.5"
                            >
                                {activeNonDirectSource.routeLabel}
                            </Badge>
                            <span className="text-muted-foreground">
                                当前承担{" "}
                                <strong className="num font-semibold text-foreground">
                                    {activeNonDirectSource.quantity}{" "}
                                    {product.unit}
                                </strong>
                                （出库时统一安排客户地址）
                            </span>
                        </>
                    ) : null}
                </div>

                {/* 视角切换：跟随当前源（默认，按源移出与填充） ↔ 全部地址总览 */}
                <div
                    role="group"
                    aria-label="地址视图模式"
                    className="inline-flex shrink-0 items-center rounded-md border border-border/70 bg-card p-0.5"
                >
                    <button
                        id={`${idPrefix}-mode-follow`}
                        type="button"
                        aria-pressed={viewMode === "follow"}
                        onClick={() => {
                            setViewMode("follow")
                            clearSelection()
                        }}
                        className={cn(
                            "cursor-pointer rounded-sm px-2.5 py-1 text-xs font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                            viewMode === "follow"
                                ? "bg-primary/10 text-primary"
                                : "text-muted-foreground hover:text-foreground",
                        )}
                    >
                        跟随左侧当前源
                    </button>
                    <button
                        id={`${idPrefix}-mode-all`}
                        type="button"
                        aria-pressed={viewMode === "all"}
                        onClick={() => {
                            setViewMode("all")
                            clearSelection()
                        }}
                        className={cn(
                            "num cursor-pointer rounded-sm px-2.5 py-1 text-xs font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                            viewMode === "all"
                                ? "bg-primary/10 text-primary"
                                : "text-muted-foreground hover:text-foreground",
                        )}
                    >
                        全部地址 ({state.counts.total})
                    </button>
                </div>
            </div>

            {/* 配送异常提醒条（仅存在异常时出现） */}
            {state.counts.notDeliverable > 0 ||
            state.counts.freightExtra > 0 ? (
                <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-warning-border/70 bg-warning-soft/55 px-5 py-2 text-xs text-warning-soft-foreground">
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
                                setViewMode("all")
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
                                改派给 {rescueDirectSource.shortLabel}
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
                                转交 源{primaryNonDirectSource.sourceNumber}
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
                                异常 {allExceptionKeys.length} 处一并拆分
                            </Button>
                        ) : null}
                    </div>
                </div>
            ) : null}

            {/* 工具栏：搜索 +（全部模式状态/来源筛选）+ 快捷配量与清空调度 */}
            <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-card px-5 py-2">
                <div className="flex min-w-0 flex-wrap items-center gap-2">
                    <div className="relative w-52">
                        <SearchIcon
                            aria-hidden="true"
                            className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                        />
                        <Input
                            id={`${idPrefix}-search`}
                            aria-label="搜索收件人、电话或收货地址"
                            placeholder="搜收件人、电话或地址"
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

                    {viewMode === "all" ? (
                        <>
                            <div
                                role="group"
                                aria-label="按状态筛选地址"
                                className="flex flex-wrap items-center gap-1"
                            >
                                {(
                                    [
                                        ["all", `全部 ${state.counts.total}`],
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
                                        aria-pressed={statusFilter === value}
                                        className={
                                            value === "issues"
                                                ? "text-destructive hover:text-destructive"
                                                : undefined
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

                            {state.directSources.length > 1 ? (
                                <div className="flex flex-wrap items-center gap-1 border-l border-border/60 pl-2">
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
                                                        active
                                                            ? null
                                                            : src.rowKey,
                                                    )
                                                    resetPage()
                                                }}
                                                className={cn(
                                                    "inline-flex h-6 cursor-pointer items-center gap-1.5 rounded-md border px-2 text-xs transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                                    active
                                                        ? accent.chipClass
                                                        : "border-transparent bg-muted/60 text-muted-foreground hover:bg-muted hover:text-foreground",
                                                )}
                                            >
                                                <span
                                                    aria-hidden="true"
                                                    className={cn(
                                                        "size-1.5 rounded-full",
                                                        accent.barClass,
                                                    )}
                                                />
                                                <span className="num font-medium">
                                                    源{src.sourceNumber}
                                                </span>
                                                <span className="num opacity-75">
                                                    {src.assignedAddressCount}处
                                                </span>
                                            </button>
                                        )
                                    })}
                                </div>
                            ) : null}
                        </>
                    ) : null}
                </div>

                {/* 右侧提效动作 */}
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

                    {/* 当左侧选中仓库/库存源且存在未覆盖缺口时，直接一键把未覆盖余量填入该仓源 */}
                    {viewMode === "follow" &&
                    activeNonDirectSource &&
                    isPositiveQuantity(nonDirectFillableQty) ? (
                        <Button
                            id={`${idPrefix}-fill-non-direct-uncovered`}
                            type="button"
                            size="xs"
                            variant="default"
                            onClick={handleFillUncoveredToActiveNonDirect}
                        >
                            <ZapIcon aria-hidden="true" />
                            未覆盖余量 +{nonDirectFillableQty} {product.unit}{" "}
                            填入 源{activeNonDirectSource.sourceNumber}
                        </Button>
                    ) : null}

                    {state.directSources.length > 0 ? (
                        <Popover
                            open={quotaOpen}
                            onOpenChange={(next) => {
                                setQuotaOpen(next)
                                if (next) {
                                    setQuotaTargetRowKey(
                                        activeDirectSource?.rowKey ??
                                            activeSourceFilter ??
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
                                                    setQuotaTargetRowKey(next)
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
                                                            key={src.rowKey}
                                                            idPrefix={`${idPrefix}-quota-source`}
                                                            value={src.rowKey}
                                                        >
                                                            {src.shortLabel}
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
                                    {state.directSources.length > 1 ? (
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
                                    {hasActiveFilters && viewMode === "all" ? (
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

                    {viewMode === "follow" && activeDirectSource ? (
                        <Button
                            id={`${idPrefix}-clear-active-source`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            disabled={followAssignedEntries.length === 0}
                            onClick={() =>
                                handleBatchClear(
                                    new Set(
                                        followAssignedEntries.map((e) => e.key),
                                    ),
                                    activeDirectSource.rowKey,
                                )
                            }
                        >
                            <EraserIcon aria-hidden="true" />
                            清空 源{activeDirectSource.sourceNumber}
                        </Button>
                    ) : viewMode === "all" &&
                      state.directSources.length === 1 &&
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
                                {hasActiveFilters ? "填满筛选" : "一键全分"}
                            </Button>
                            <Button
                                id={`${idPrefix}-clear-all`}
                                type="button"
                                variant="ghost"
                                size="xs"
                                disabled={
                                    filteredEntries.filter((e) =>
                                        isPositiveQuantity(e.assignedTotal),
                                    ).length === 0
                                }
                                onClick={() => handleBatchClear(filteredKeySet)}
                            >
                                <EraserIcon aria-hidden="true" />
                                全部移出
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
                                setActionFeedback("已将未覆盖余量拆给新来源。")
                            }}
                        >
                            <SplitIcon aria-hidden="true" />
                            余量拆新源
                        </Button>
                    ) : null}
                </div>
            </div>

            {/* 轻量横向省/市筛选条 */}
            {state.regionFacets.level !== "none" ? (
                <div className="flex shrink-0 items-center gap-1.5 overflow-x-auto border-b border-border/50 bg-surface-sunken/40 px-5 py-1.5">
                    <span className="inline-flex shrink-0 items-center gap-1 text-tiny font-medium text-muted-foreground">
                        <MapPinIcon aria-hidden="true" className="size-3" />
                        {state.regionFacets.level === "province"
                            ? "按地区"
                            : "按区县"}
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
                                ? "bg-card font-medium text-foreground shadow-2xs ring-1 ring-border/70"
                                : "text-muted-foreground hover:bg-card/70 hover:text-foreground",
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
                                    setRegionFilter(active ? null : item.key)
                                    resetPage()
                                }}
                                className={cn(
                                    "inline-flex shrink-0 cursor-pointer items-center gap-1 rounded-sm px-2 py-0.5 text-xs transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                    active
                                        ? "bg-card font-medium text-primary shadow-2xs ring-1 ring-primary/35"
                                        : hasAlert
                                          ? "bg-warning-soft/70 text-warning-soft-foreground hover:bg-warning-soft"
                                          : "text-muted-foreground hover:bg-card/70 hover:text-foreground",
                                )}
                            >
                                <span>{item.label}</span>
                                <span className="num text-tiny opacity-75">
                                    {item.fullyAssignedCount}/{item.totalCount}
                                </span>
                            </button>
                        )
                    })}
                </div>
            ) : null}

            {/* 勾选后的批量操作条 */}
            {effectiveSelectedKeys.size > 0 ? (
                <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-primary/25 bg-primary/5 px-5 py-2 text-xs">
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
                        {viewMode === "all" &&
                        allPageSelected &&
                        !allFilteredSelected &&
                        filteredEntries.length > pageEntries.length ? (
                            <button
                                id={`${idPrefix}-select-all-filtered`}
                                type="button"
                                onClick={selectAllFiltered}
                                className="cursor-pointer font-medium text-primary hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                            >
                                全选筛选全部 {filteredEntries.length} 处
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
                        {/* 跟随模式下，针对当前选中源的一键批量填入 / 移出 */}
                        {viewMode === "follow" &&
                        activeDirectSource &&
                        activeFollowSelectedInUncovered.length > 0 ? (
                            <Button
                                id={`${idPrefix}-bulk-fill-active`}
                                type="button"
                                size="xs"
                                variant="default"
                                onClick={() =>
                                    handleBatchAssign(
                                        activeDirectSource.rowKey,
                                        new Set(
                                            activeFollowSelectedInUncovered,
                                        ),
                                        "fill_remaining",
                                    )
                                }
                            >
                                <PlusIcon aria-hidden="true" />
                                填入 源{activeDirectSource.sourceNumber}（
                                {activeFollowSelectedInUncovered.length} 处）
                            </Button>
                        ) : null}

                        {viewMode === "follow" &&
                        activeDirectSource &&
                        activeFollowSelectedInAssigned.length > 0 ? (
                            <Button
                                id={`${idPrefix}-bulk-remove-active`}
                                type="button"
                                size="xs"
                                variant="outline"
                                className="border-destructive-border bg-card text-destructive hover:bg-destructive-soft"
                                onClick={() =>
                                    handleBatchClear(
                                        new Set(activeFollowSelectedInAssigned),
                                        activeDirectSource.rowKey,
                                    )
                                }
                            >
                                <XIcon aria-hidden="true" />
                                从 源{activeDirectSource.sourceNumber} 移出（
                                {activeFollowSelectedInAssigned.length} 处）
                            </Button>
                        ) : null}

                        {viewMode === "all" &&
                        state.directSources.length === 1 &&
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
                                填入 {defaultSource.shortLabel}
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
                                转入 源{primaryNonDirectSource.sourceNumber}·
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
                                    {state.nonDirectSources.map((src) => (
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
                                    ))}
                                </DropdownMenuContent>
                            </DropdownMenu>
                        ) : null}

                        {canSplitNewSource && onSplitAddressesToNewSource ? (
                            <Button
                                id={`${idPrefix}-bulk-split-new`}
                                type="button"
                                size="xs"
                                variant="outline"
                                className="bg-card"
                                onClick={() => {
                                    const keys = [...effectiveSelectedKeys]
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

                        {viewMode === "all" &&
                        selectedAssignedKeysInAll.length > 0 ? (
                            <Button
                                id={`${idPrefix}-bulk-clear`}
                                type="button"
                                size="xs"
                                variant="ghost"
                                className="text-destructive hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                onClick={() =>
                                    handleBatchClear(
                                        new Set(selectedAssignedKeysInAll),
                                    )
                                }
                            >
                                <XIcon aria-hidden="true" />
                                移出直发（{selectedAssignedKeysInAll.length}）
                            </Button>
                        ) : null}
                    </div>
                </div>
            ) : actionFeedback ? (
                <div
                    role="status"
                    className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken px-5 py-1.5 text-xs text-muted-foreground"
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

            {/* 主内容区：跟随模式（当前源已配 + 未覆盖地址池） vs 全部地址总览 */}
            {viewMode === "follow" ? (
                <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
                    {activeDirectSource ? (
                        <div className="divide-y divide-border/70">
                            {/* 区块一：当前选中直发源已分配地址（每行直达「移出」按钮） */}
                            <div>
                                <div className="sticky top-0 z-10 flex items-center justify-between gap-2 border-b border-border/60 bg-table-header px-5 py-2 text-xs">
                                    <div className="flex items-center gap-2">
                                        <NativeCheckbox
                                            id={`${idPrefix}-follow-assigned-select-all`}
                                            aria-label={`全选 源${activeDirectSource.sourceNumber} 已分配地址`}
                                            disabled={
                                                followAssignedEntries.length ===
                                                0
                                            }
                                            checked={
                                                followAssignedEntries.length >
                                                    0 &&
                                                followAssignedEntries.every(
                                                    (e) =>
                                                        effectiveSelectedKeys.has(
                                                            e.key,
                                                        ),
                                                )
                                            }
                                            indeterminate={
                                                followAssignedEntries.some(
                                                    (e) =>
                                                        effectiveSelectedKeys.has(
                                                            e.key,
                                                        ),
                                                ) &&
                                                !followAssignedEntries.every(
                                                    (e) =>
                                                        effectiveSelectedKeys.has(
                                                            e.key,
                                                        ),
                                                )
                                            }
                                            onCheckedChange={(checked) =>
                                                toggleKeyGroupSelection(
                                                    followAssignedEntries.map(
                                                        (e) => e.key,
                                                    ),
                                                    checked,
                                                )
                                            }
                                        />
                                        <span className="font-semibold text-foreground">
                                            源{activeDirectSource.sourceNumber}{" "}
                                            已承接地址
                                        </span>
                                        <span className="num text-muted-foreground">
                                            {followAssignedEntries.length} 处 ·{" "}
                                            {
                                                activeDirectSource.assignedTotalQty
                                            }{" "}
                                            {product.unit}
                                        </span>
                                    </div>
                                    <span className="text-tiny text-muted-foreground">
                                        点右侧「移出」释放到未覆盖地址池，即可在其他源直接填入
                                    </span>
                                </div>

                                {followAssignedEntries.length === 0 ? (
                                    <div className="px-5 py-6 text-center text-xs text-muted-foreground">
                                        源{activeDirectSource.sourceNumber}{" "}
                                        暂未分配直发地址
                                        {deliverableUncoveredForActive.length >
                                        0
                                            ? "，可在下方「未覆盖地址」直接点击填入。"
                                            : "；如需从其他来源调出，可切换到其他来源移出或点右上角「全部地址」。"}
                                    </div>
                                ) : (
                                    <div className="divide-y divide-grid">
                                        {followAssignedEntries.map((entry) => {
                                            const slice = entry.slices.find(
                                                (s) =>
                                                    s.rowKey ===
                                                    activeDirectSource.rowKey,
                                            )!
                                            const rowId = `${idPrefix}-follow-${addressIdSegment(entry.baseRow)}`
                                            const term = addressDeliveryTerm(
                                                activeDirectSource.option,
                                                entry.baseRow.region_code,
                                            )
                                            const otherDirectQty = (() => {
                                                try {
                                                    const selfQty =
                                                        isPositiveQuantity(
                                                            slice.quantity,
                                                        )
                                                            ? compactFixed(
                                                                  slice.quantity.trim(),
                                                              )
                                                            : "0"
                                                    const diff = subtractFixed(
                                                        entry.assignedTotal,
                                                        selfQty,
                                                        {
                                                            maxScale: 6,
                                                            outputScale: 6,
                                                        },
                                                    )
                                                    return compareDecimal(
                                                        diff,
                                                        "0",
                                                        6,
                                                    ) > 0
                                                        ? compactFixed(diff)
                                                        : "0"
                                                } catch {
                                                    return "0"
                                                }
                                            })()

                                            return (
                                                <div
                                                    key={entry.key}
                                                    className={cn(
                                                        "grid grid-cols-[1.75rem_minmax(0,1fr)_5rem_8.5rem_4.5rem] items-center gap-3 px-5 py-2 transition-colors hover:bg-row-hover",
                                                        Boolean(slice.error) &&
                                                            "bg-destructive-soft/30",
                                                    )}
                                                >
                                                    <div className="flex items-center justify-center">
                                                        <NativeCheckbox
                                                            id={`${rowId}-select`}
                                                            checked={effectiveSelectedKeys.has(
                                                                entry.key,
                                                            )}
                                                            aria-label={`选择 ${entry.baseRow.recipient}`}
                                                            onCheckedChange={(
                                                                checked,
                                                            ) =>
                                                                toggleRowSelection(
                                                                    entry.key,
                                                                    checked,
                                                                )
                                                            }
                                                        />
                                                    </div>

                                                    <div className="min-w-0">
                                                        <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
                                                            <span className="text-body-compact font-medium text-foreground">
                                                                {
                                                                    entry
                                                                        .baseRow
                                                                        .recipient
                                                                }
                                                            </span>
                                                            <span className="num text-xs text-muted-foreground">
                                                                {
                                                                    entry
                                                                        .baseRow
                                                                        .phone
                                                                }
                                                            </span>
                                                            {isPositiveQuantity(
                                                                otherDirectQty,
                                                            ) ? (
                                                                <Badge
                                                                    variant="secondary"
                                                                    className="h-4.5 px-1.5 text-tiny"
                                                                >
                                                                    他源已分{" "}
                                                                    {
                                                                        otherDirectQty
                                                                    }{" "}
                                                                    {
                                                                        product.unit
                                                                    }
                                                                </Badge>
                                                            ) : null}
                                                        </div>
                                                        <p
                                                            className="mt-0.5 truncate text-xs text-muted-foreground"
                                                            title={
                                                                entry.fullAddress
                                                            }
                                                        >
                                                            {entry.fullAddress}
                                                        </p>
                                                        {slice.error ? (
                                                            <p
                                                                role="alert"
                                                                className="mt-0.5 flex items-center gap-1 text-xs text-destructive"
                                                            >
                                                                <CircleAlertIcon className="size-3 shrink-0" />
                                                                <span>
                                                                    {
                                                                        slice.error
                                                                    }
                                                                </span>
                                                            </p>
                                                        ) : null}
                                                    </div>

                                                    <div>
                                                        {term ? (
                                                            <Badge
                                                                variant={
                                                                    term ===
                                                                    "NOT_DELIVERABLE"
                                                                        ? "destructive"
                                                                        : term ===
                                                                            "FREIGHT_EXTRA"
                                                                          ? "warning"
                                                                          : "neutral"
                                                                }
                                                                className="h-5 px-1.5 text-tiny"
                                                            >
                                                                {deliveryTermLabel(
                                                                    activeDirectSource.option,
                                                                    term,
                                                                )}
                                                            </Badge>
                                                        ) : (
                                                            <span className="text-xs text-muted-foreground">
                                                                —
                                                            </span>
                                                        )}
                                                    </div>

                                                    <div className="flex items-center justify-end gap-1">
                                                        <Input
                                                            id={`${rowId}-quantity`}
                                                            type="number"
                                                            inputMode="decimal"
                                                            min="0"
                                                            max={
                                                                entry.demandQty
                                                            }
                                                            step={
                                                                sourcingQuantityStep(
                                                                    product.quantityScale,
                                                                ) ?? "1"
                                                            }
                                                            aria-label={`${entry.baseRow.recipient} 在 源${activeDirectSource.sourceNumber} 的数量`}
                                                            aria-invalid={
                                                                Boolean(
                                                                    slice.error,
                                                                ) || undefined
                                                            }
                                                            className="num h-control-sm w-16 bg-card text-right"
                                                            value={
                                                                slice.quantity
                                                            }
                                                            onChange={(
                                                                event,
                                                            ) => {
                                                                const patch =
                                                                    buildSetSliceQuantityPatch(
                                                                        product,
                                                                        form
                                                                            .state
                                                                            .values
                                                                            .lines,
                                                                        activeDirectSource.rowKey,
                                                                        entry.baseRow,
                                                                        event
                                                                            .target
                                                                            .value,
                                                                    )
                                                                if (patch)
                                                                    applyPatches(
                                                                        [patch],
                                                                    )
                                                            }}
                                                            onBlur={() =>
                                                                normalizeOnBlur(
                                                                    activeDirectSource.rowKey,
                                                                    slice.quantity,
                                                                    entry.baseRow,
                                                                )
                                                            }
                                                        />
                                                        <span className="num shrink-0 text-xs text-muted-foreground">
                                                            / {entry.demandQty}{" "}
                                                            {product.unit}
                                                        </span>
                                                    </div>

                                                    <div className="flex justify-end">
                                                        <Button
                                                            id={`${rowId}-remove`}
                                                            type="button"
                                                            size="xs"
                                                            variant="ghost"
                                                            className="text-muted-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                                            aria-label={`从 源${activeDirectSource.sourceNumber} 移出 ${entry.baseRow.recipient}`}
                                                            onClick={() =>
                                                                handleRemoveAddressFromSource(
                                                                    activeDirectSource.rowKey,
                                                                    entry,
                                                                )
                                                            }
                                                        >
                                                            <XIcon aria-hidden="true" />
                                                            移出
                                                        </Button>
                                                    </div>
                                                </div>
                                            )
                                        })}
                                    </div>
                                )}
                            </div>

                            {/* 区块二：未覆盖地址池（可在当前适合的源逐条或一键全部填入） */}
                            <div>
                                <div className="sticky top-0 z-10 flex flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken px-5 py-2 text-xs">
                                    <div className="flex flex-wrap items-center gap-2">
                                        <NativeCheckbox
                                            id={`${idPrefix}-follow-uncovered-select-all`}
                                            aria-label="全选未覆盖地址"
                                            disabled={
                                                followUncoveredEntries.length ===
                                                0
                                            }
                                            checked={
                                                followUncoveredEntries.length >
                                                    0 &&
                                                followUncoveredEntries.every(
                                                    (e) =>
                                                        effectiveSelectedKeys.has(
                                                            e.key,
                                                        ),
                                                )
                                            }
                                            indeterminate={
                                                followUncoveredEntries.some(
                                                    (e) =>
                                                        effectiveSelectedKeys.has(
                                                            e.key,
                                                        ),
                                                ) &&
                                                !followUncoveredEntries.every(
                                                    (e) =>
                                                        effectiveSelectedKeys.has(
                                                            e.key,
                                                        ),
                                                )
                                            }
                                            onCheckedChange={(checked) =>
                                                toggleKeyGroupSelection(
                                                    followUncoveredEntries.map(
                                                        (e) => e.key,
                                                    ),
                                                    checked,
                                                )
                                            }
                                        />
                                        <span className="font-semibold text-foreground">
                                            未覆盖地址池
                                        </span>
                                        <span className="num text-muted-foreground">
                                            {followUncoveredEntries.length} 处 ·{" "}
                                            {safeSumQuantities(
                                                followUncoveredEntries.map(
                                                    (e) =>
                                                        e.remainingUnassigned,
                                                ),
                                            )}{" "}
                                            {product.unit}
                                        </span>
                                        {state.hasNonDirectSources &&
                                        state.directBalanced &&
                                        followUncoveredEntries.length > 0 ? (
                                            <span className="text-tiny text-success-soft-foreground">
                                                （当前余量已由仓库/库存统筹覆盖）
                                            </span>
                                        ) : null}
                                    </div>

                                    {deliverableUncoveredForActive.length >
                                    0 ? (
                                        <Button
                                            id={`${idPrefix}-fill-all-uncovered-active`}
                                            type="button"
                                            size="xs"
                                            variant="secondary"
                                            onClick={
                                                handleFillAllUncoveredForActiveDirect
                                            }
                                        >
                                            <PlusIcon aria-hidden="true" />
                                            一键填入 源
                                            {activeDirectSource.sourceNumber}（
                                            {
                                                deliverableUncoveredForActive.length
                                            }{" "}
                                            处 · {deliverableUncoveredQty}{" "}
                                            {product.unit}）
                                        </Button>
                                    ) : null}
                                </div>

                                {followUncoveredEntries.length === 0 ? (
                                    <div className="flex items-center justify-center gap-1.5 px-5 py-6 text-xs text-success-soft-foreground">
                                        <CheckCircle2Icon className="size-3.5" />
                                        <span>
                                            全部地址已由直发来源完整覆盖，无剩余未覆盖地址。
                                        </span>
                                    </div>
                                ) : (
                                    <div className="divide-y divide-grid">
                                        {followUncoveredEntries.map((entry) => {
                                            const rowId = `${idPrefix}-uncovered-${addressIdSegment(entry.baseRow)}`
                                            const term = addressDeliveryTerm(
                                                activeDirectSource.option,
                                                entry.baseRow.region_code,
                                            )
                                            const canDeliver =
                                                addressDeliverable(
                                                    activeDirectSource.option,
                                                    entry.baseRow.region_code,
                                                )

                                            return (
                                                <div
                                                    key={entry.key}
                                                    className="grid grid-cols-[1.75rem_minmax(0,1fr)_5rem_7rem_6.5rem] items-center gap-3 bg-surface-sunken/25 px-5 py-2 transition-colors hover:bg-row-hover"
                                                >
                                                    <div className="flex items-center justify-center">
                                                        <NativeCheckbox
                                                            id={`${rowId}-select`}
                                                            checked={effectiveSelectedKeys.has(
                                                                entry.key,
                                                            )}
                                                            aria-label={`选择未覆盖地址 ${entry.baseRow.recipient}`}
                                                            onCheckedChange={(
                                                                checked,
                                                            ) =>
                                                                toggleRowSelection(
                                                                    entry.key,
                                                                    checked,
                                                                )
                                                            }
                                                        />
                                                    </div>

                                                    <div className="min-w-0">
                                                        <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
                                                            <span className="text-body-compact font-medium text-foreground">
                                                                {
                                                                    entry
                                                                        .baseRow
                                                                        .recipient
                                                                }
                                                            </span>
                                                            <span className="num text-xs text-muted-foreground">
                                                                {
                                                                    entry
                                                                        .baseRow
                                                                        .phone
                                                                }
                                                            </span>
                                                        </div>
                                                        <p
                                                            className="mt-0.5 truncate text-xs text-muted-foreground"
                                                            title={
                                                                entry.fullAddress
                                                            }
                                                        >
                                                            {entry.fullAddress}
                                                        </p>
                                                    </div>

                                                    <div>
                                                        {term ? (
                                                            <Badge
                                                                variant={
                                                                    term ===
                                                                    "NOT_DELIVERABLE"
                                                                        ? "destructive"
                                                                        : term ===
                                                                            "FREIGHT_EXTRA"
                                                                          ? "warning"
                                                                          : "neutral"
                                                                }
                                                                className="h-5 px-1.5 text-tiny"
                                                            >
                                                                {deliveryTermLabel(
                                                                    activeDirectSource.option,
                                                                    term,
                                                                )}
                                                            </Badge>
                                                        ) : (
                                                            <span className="text-xs text-muted-foreground">
                                                                —
                                                            </span>
                                                        )}
                                                    </div>

                                                    <div className="num text-right text-xs">
                                                        <span className="font-semibold text-foreground">
                                                            {
                                                                entry.remainingUnassigned
                                                            }{" "}
                                                            {product.unit}
                                                        </span>
                                                        {entry.remainingUnassigned !==
                                                        entry.demandQty ? (
                                                            <span className="block text-tiny text-muted-foreground">
                                                                总需{" "}
                                                                {
                                                                    entry.demandQty
                                                                }
                                                            </span>
                                                        ) : null}
                                                    </div>

                                                    <div className="flex justify-end">
                                                        {canDeliver ? (
                                                            <Button
                                                                id={`${rowId}-fill`}
                                                                type="button"
                                                                size="xs"
                                                                variant="outline"
                                                                className="bg-card"
                                                                onClick={() =>
                                                                    handleFillUncoveredToDirect(
                                                                        activeDirectSource.rowKey,
                                                                        entry,
                                                                    )
                                                                }
                                                            >
                                                                <PlusIcon aria-hidden="true" />
                                                                填入源
                                                                {
                                                                    activeDirectSource.sourceNumber
                                                                }
                                                            </Button>
                                                        ) : (
                                                            <span className="text-tiny text-destructive">
                                                                源
                                                                {
                                                                    activeDirectSource.sourceNumber
                                                                }
                                                                不配送
                                                            </span>
                                                        )}
                                                    </div>
                                                </div>
                                            )
                                        })}
                                    </div>
                                )}
                            </div>
                        </div>
                    ) : activeNonDirectSource ? (
                        /* 当前左侧选中了仓库/库存源：展示未覆盖（留给仓库）地址 + 可从直发拉取转入的地址 */
                        <div className="divide-y divide-border/70">
                            <div>
                                <div className="sticky top-0 z-10 flex flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-table-header px-5 py-2 text-xs">
                                    <div className="flex items-center gap-2">
                                        <span className="font-semibold text-foreground">
                                            未走直发 / 留待仓库统筹地址
                                        </span>
                                        <span className="num text-muted-foreground">
                                            {followUncoveredEntries.length} 处 ·{" "}
                                            {safeSumQuantities(
                                                followUncoveredEntries.map(
                                                    (e) =>
                                                        e.remainingUnassigned,
                                                ),
                                            )}{" "}
                                            {product.unit}
                                        </span>
                                    </div>
                                    {isPositiveQuantity(
                                        nonDirectFillableQty,
                                    ) ? (
                                        <Button
                                            id={`${idPrefix}-warehouse-fill-uncovered`}
                                            type="button"
                                            size="xs"
                                            variant="secondary"
                                            onClick={
                                                handleFillUncoveredToActiveNonDirect
                                            }
                                        >
                                            <PlusIcon aria-hidden="true" />
                                            未覆盖余量 +{
                                                nonDirectFillableQty
                                            }{" "}
                                            {product.unit} 填入 源
                                            {activeNonDirectSource.sourceNumber}
                                        </Button>
                                    ) : null}
                                </div>

                                {followUncoveredEntries.length === 0 ? (
                                    <p className="px-5 py-5 text-center text-xs text-muted-foreground">
                                        目前全部客户地址均已分配给直发来源；如需由{" "}
                                        源{activeNonDirectSource.sourceNumber}·
                                        {activeNonDirectSource.routeLabel}{" "}
                                        发货，可在下方将指定地址移出直发或直接转入。
                                    </p>
                                ) : (
                                    <div className="divide-y divide-grid">
                                        {followUncoveredEntries.map((entry) => (
                                            <div
                                                key={entry.key}
                                                className="grid grid-cols-[minmax(0,1fr)_7.5rem] items-center gap-3 px-5 py-2 hover:bg-row-hover"
                                            >
                                                <div className="min-w-0">
                                                    <div className="flex flex-wrap items-center gap-x-2">
                                                        <span className="text-body-compact font-medium text-foreground">
                                                            {
                                                                entry.baseRow
                                                                    .recipient
                                                            }
                                                        </span>
                                                        <span className="num text-xs text-muted-foreground">
                                                            {
                                                                entry.baseRow
                                                                    .phone
                                                            }
                                                        </span>
                                                    </div>
                                                    <p
                                                        className="mt-0.5 truncate text-xs text-muted-foreground"
                                                        title={
                                                            entry.fullAddress
                                                        }
                                                    >
                                                        {entry.fullAddress}
                                                    </p>
                                                </div>
                                                <span className="num text-right text-xs font-medium text-foreground">
                                                    {entry.remainingUnassigned}{" "}
                                                    {product.unit}
                                                </span>
                                            </div>
                                        ))}
                                    </div>
                                )}
                            </div>

                            {followDirectAssignedAll.length > 0 ? (
                                <div>
                                    <div className="sticky top-0 z-10 flex items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken px-5 py-2 text-xs">
                                        <span className="font-semibold text-foreground">
                                            当前由直发承接的地址（
                                            {
                                                followDirectAssignedAll.length
                                            }{" "}
                                            处）
                                        </span>
                                        <span className="text-tiny text-muted-foreground">
                                            点「转入源
                                            {activeNonDirectSource.sourceNumber}
                                            」可释放直发并同步增加仓库数量
                                        </span>
                                    </div>
                                    <div className="divide-y divide-grid">
                                        {followDirectAssignedAll.map(
                                            (entry) => {
                                                const rowId = `${idPrefix}-from-direct-${addressIdSegment(entry.baseRow)}`
                                                const owners =
                                                    entry.activeSlices
                                                        .map(
                                                            (s) =>
                                                                `源${s.sourceNumber}`,
                                                        )
                                                        .join(" + ")
                                                return (
                                                    <div
                                                        key={entry.key}
                                                        className="grid grid-cols-[minmax(0,1fr)_5.5rem_5rem_11.5rem] items-center gap-3 px-5 py-2 hover:bg-row-hover"
                                                    >
                                                        <div className="min-w-0">
                                                            <div className="flex flex-wrap items-center gap-x-2">
                                                                <span className="text-body-compact font-medium text-foreground">
                                                                    {
                                                                        entry
                                                                            .baseRow
                                                                            .recipient
                                                                    }
                                                                </span>
                                                                <span className="num text-xs text-muted-foreground">
                                                                    {
                                                                        entry
                                                                            .baseRow
                                                                            .phone
                                                                    }
                                                                </span>
                                                            </div>
                                                            <p
                                                                className="mt-0.5 truncate text-xs text-muted-foreground"
                                                                title={
                                                                    entry.fullAddress
                                                                }
                                                            >
                                                                {
                                                                    entry.fullAddress
                                                                }
                                                            </p>
                                                        </div>
                                                        <Badge
                                                            variant="info"
                                                            className="h-5 px-1.5 text-tiny"
                                                        >
                                                            {owners} 直发
                                                        </Badge>
                                                        <span className="num text-right text-xs font-semibold text-foreground">
                                                            {
                                                                entry.assignedTotal
                                                            }{" "}
                                                            {product.unit}
                                                        </span>
                                                        <div className="flex items-center justify-end gap-1">
                                                            <Button
                                                                id={`${rowId}-transfer`}
                                                                type="button"
                                                                size="xs"
                                                                variant="outline"
                                                                className="bg-card"
                                                                onClick={() =>
                                                                    handleBatchTransferToNonDirect(
                                                                        activeNonDirectSource.rowKey,
                                                                        new Set(
                                                                            [
                                                                                entry.key,
                                                                            ],
                                                                        ),
                                                                    )
                                                                }
                                                            >
                                                                <ArrowRightLeftIcon aria-hidden="true" />
                                                                转入源
                                                                {
                                                                    activeNonDirectSource.sourceNumber
                                                                }
                                                            </Button>
                                                            <Button
                                                                id={`${rowId}-clear`}
                                                                type="button"
                                                                size="icon-xs"
                                                                variant="ghost"
                                                                title="仅移出直发"
                                                                aria-label={`仅移出 ${entry.baseRow.recipient} 的直发分配`}
                                                                className="text-muted-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                                                onClick={() =>
                                                                    handleBatchClear(
                                                                        new Set(
                                                                            [
                                                                                entry.key,
                                                                            ],
                                                                        ),
                                                                    )
                                                                }
                                                            >
                                                                <XIcon />
                                                            </Button>
                                                        </div>
                                                    </div>
                                                )
                                            },
                                        )}
                                    </div>
                                </div>
                            ) : null}
                        </div>
                    ) : null}
                </div>
            ) : (
                /* 全部地址总览模式（适合 100+ 地址全局筛选与分页） */
                <>
                    <div className="grid shrink-0 grid-cols-[1.75rem_minmax(0,1fr)_5rem_11.5rem_10rem] items-center gap-3 border-b border-border/70 bg-table-header px-5 py-2 text-xs font-medium text-muted-foreground">
                        <div className="flex items-center justify-center">
                            <NativeCheckbox
                                id={`${idPrefix}-select-page`}
                                aria-label="全选当前页地址用于批量操作"
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
                        <span>配送状态</span>
                        <span>承接去向</span>
                        <span className="text-right">直发量 / 操作</span>
                    </div>

                    <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
                        {pageEntries.length === 0 ? (
                            <div className="flex flex-col items-center justify-center gap-2 px-6 py-16 text-center text-xs text-muted-foreground">
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
                                        activeDirectSource={activeDirectSource}
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
                                        onClearAllDirect={() =>
                                            handleBatchClear(
                                                new Set([entry.key]),
                                            )
                                        }
                                        onFillUncoveredTo={(targetRowKey) =>
                                            handleFillUncoveredToDirect(
                                                targetRowKey,
                                                entry,
                                            )
                                        }
                                    />
                                ))}
                            </div>
                        )}
                    </div>
                </>
            )}

            {/* 固定分页与汇总底栏 */}
            <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-border/60 bg-surface-sunken/70 px-5 py-2 text-xs text-muted-foreground">
                <div className="flex flex-wrap items-center gap-3">
                    <span className="num">
                        {viewMode === "follow"
                            ? `共 ${baseFilteredEntries.length} 处地址`
                            : filteredEntries.length === 0
                              ? "共 0 处"
                              : filteredEntries.length > pageSize
                                ? `第 ${pageStart + 1}–${Math.min(pageStart + pageSize, filteredEntries.length)} 处，共 ${filteredEntries.length} 处`
                                : `共 ${filteredEntries.length} 处地址`}
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
                                / 额度 {state.directTargetQty} {product.unit}
                            </span>
                        ) : null}
                    </span>
                </div>

                {viewMode === "all" && filteredEntries.length > 25 ? (
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

function GlobalAddressRow({
    idPrefix,
    entry,
    unit,
    quantityScale,
    directSources,
    nonDirectSources,
    activeDirectSource,
    hasNonDirectSources,
    hasDirectShortfall,
    directBalanced,
    primarySource,
    checked,
    onToggleChecked,
    onAssignExclusive,
    onSetSliceQty,
    onClearAllDirect,
    onFillUncoveredTo,
}: {
    idPrefix: string
    entry: GlobalAddressEntry
    unit: string
    quantityScale?: number | null
    directSources: readonly DirectSourceInfo[]
    nonDirectSources: readonly NonDirectSourceInfo[]
    activeDirectSource?: DirectSourceInfo
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
    onClearAllDirect: () => void
    onFillUncoveredTo: (targetRowKey: string) => void
}) {
    const rowId = `${idPrefix}-${addressIdSegment(entry.baseRow)}`
    const isSplitMulti = entry.slices.length > 1
    const singleSlice = entry.slices.length === 1 ? entry.slices[0] : undefined
    const editableSource = singleSlice
        ? directSources.find((src) => src.rowKey === singleSlice.rowKey)
        : (activeDirectSource ?? primarySource)
    const hasUncovered = isPositiveQuantity(entry.remainingUnassigned)
    const quickFillTarget = hasUncovered
        ? activeDirectSource &&
          addressDeliverable(
              activeDirectSource.option,
              entry.baseRow.region_code,
          )
            ? activeDirectSource
            : directSources.find(
                  (src) =>
                      src.selected &&
                      src.rowKey !== singleSlice?.rowKey &&
                      addressDeliverable(src.option, entry.baseRow.region_code),
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
            : "未覆盖（留给仓库）"
        : "未覆盖"

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
            <div className="grid grid-cols-[1.75rem_minmax(0,1fr)_5rem_11.5rem_10rem] items-center gap-3 px-5 py-2">
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

                {/* 承接去向 */}
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
                                        : "未覆盖（点击填入）"}
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
                                        ? "移出直发（留给仓库统筹）"
                                        : "未覆盖"}
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

                {/* 数量 + 直达「移出 / 填入」动作 */}
                <div className="flex flex-col items-end gap-0.5">
                    <div className="flex items-center justify-end gap-1">
                        {isSplitMulti ? (
                            <div className="flex items-baseline gap-1 text-xs">
                                <span
                                    className={cn(
                                        "num text-body-compact font-semibold",
                                        entry.overAllocated
                                            ? "text-destructive"
                                            : hasUncovered
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
                          (entry.anySourceDeliverable ||
                              singleSlice?.positive) ? (
                            <>
                                <Input
                                    id={`${rowId}-quantity`}
                                    type="number"
                                    inputMode="decimal"
                                    min="0"
                                    max={entry.demandQty}
                                    step={
                                        sourcingQuantityStep(quantityScale) ??
                                        "1"
                                    }
                                    placeholder="0"
                                    aria-label={`${entry.baseRow.recipient}，${entry.fullAddress}，本次交付数量（${unit}）`}
                                    aria-invalid={entry.hasError || undefined}
                                    className="num h-control-sm w-14 bg-card text-right"
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
                                    /{entry.demandQty}
                                </span>
                            </>
                        ) : (
                            <span className="num text-xs text-muted-foreground">
                                0 / {entry.demandQty} {unit}
                            </span>
                        )}

                        {isPositiveQuantity(entry.assignedTotal) ? (
                            <Button
                                id={`${rowId}-remove-direct`}
                                type="button"
                                size="icon-xs"
                                variant="ghost"
                                title="移出直发分配"
                                aria-label={`移出 ${entry.baseRow.recipient} 的直发分配`}
                                className="text-muted-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                onClick={onClearAllDirect}
                            >
                                <XIcon />
                            </Button>
                        ) : quickFillTarget ? (
                            <Button
                                id={`${rowId}-quick-fill`}
                                type="button"
                                size="icon-xs"
                                variant="ghost"
                                title={`填入 源${quickFillTarget.sourceNumber}`}
                                aria-label={`将 ${entry.baseRow.recipient} 填入 源${quickFillTarget.sourceNumber}`}
                                className="text-primary hover:bg-primary/10"
                                onClick={() =>
                                    onFillUncoveredTo(quickFillTarget.rowKey)
                                }
                            >
                                <PlusIcon />
                            </Button>
                        ) : null}
                    </div>

                    {hasUncovered &&
                    isPositiveQuantity(entry.assignedTotal) &&
                    quickFillTarget ? (
                        <button
                            id={`${rowId}-split-remainder`}
                            type="button"
                            onClick={() =>
                                onFillUncoveredTo(quickFillTarget.rowKey)
                            }
                            className="inline-flex cursor-pointer items-center gap-0.5 text-tiny font-medium text-primary hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                        >
                            <PlusIcon className="size-2.5" />余{" "}
                            {entry.remainingUnassigned} 填源
                            {quickFillTarget.sourceNumber}
                        </button>
                    ) : null}
                </div>
            </div>

            {/* 单地址拆给多来源时的清晰子行 */}
            {isSplitMulti ? (
                <div className="space-y-1 border-t border-border/40 bg-surface-sunken/50 py-1.5 pr-5 pl-12">
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
                    className="flex items-center gap-1.5 px-5 pb-2 pl-12 text-xs text-destructive"
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
