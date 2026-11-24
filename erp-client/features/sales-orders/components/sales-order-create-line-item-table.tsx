"use client"

import * as React from "react"
import { useSelector } from "@tanstack/react-form"
import {
    CalendarDaysIcon,
    ChevronDownIcon,
    ChevronLeftIcon,
    ChevronRightIcon,
    CircleHelpIcon,
    MapPinIcon,
    PlusIcon,
    SearchIcon,
    Trash2Icon,
    XIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    EditableLineItemTable,
    ValidationSummary,
    detailWorkspaceStyles,
} from "@/components/business"
import { DetailHint } from "@/components/business/detail-presentation"
import { toFieldErrors } from "@/components/form"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { DatePicker } from "@/components/ui/date-picker"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuGroup,
    DropdownMenuItem,
    DropdownMenuLabel,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import {
    Popover,
    PopoverContent,
    PopoverDescription,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { RequiredMark } from "@/components/ui/required-mark"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toast } from "@/components/ui/toast"
import { useRegionIndex } from "@/features/regions"
import { buildSalesOrderCreateLineItemColumns } from "@/features/sales-orders/components/sales-order-create-line-item-columns"
import type { SalesLineSkuDetails } from "@/features/sales-orders/hooks/use-sales-line-images-query"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import type {
    SalesLineProcurementResponsibility,
    SalesOrderDraftLineInput,
    SalesProcurementPriceBasis,
} from "@/features/sales-orders/types"
import { cn } from "@/lib/utils"
import type { DeliveryCoverageHint as CoverageCode } from "../api/delivery-coverage"
import { addressKey } from "../lib/sales-delivery-import"
import {
    applyDeliveryAddressToSelectedLines,
    collectOrderAddressPool,
    detectUnifiedAddress,
} from "../lib/sales-delivery-pool"
import {
    buildFilteredSalesLines,
    type SalesLineFilterKey,
} from "../lib/sales-order-create-line-health"
import {
    createEmptyLine,
    type UnifiedDeliveryAddress,
} from "../lib/sales-order-create-model"
import { supplyReferencePrice } from "../lib/sales-procurement-price"
import { SalesOrderCreateLineRow } from "./sales-order-create-line-row"
import { SalesOrderCreateUnifiedAddressDialog } from "./sales-order-create-unified-address-dialog"

const PAGE_SIZE_OPTIONS = [20, 50, 100] as const

type RemovedEntry = {
    line: SalesOrderDraftLineInput
    index: number
}

export type SalesOrderCreateLineItemTableProps = {
    inquiryLocked?: boolean
    inquiryDeliveryLineKeys?: ReadonlySet<string>
    form: SalesOrderCreateFormApi
    skuDetails: SalesLineSkuDetails
    procurementOwners?: ReadonlyMap<string, SalesLineProcurementResponsibility>
    onPickSku?: (rowIndex: number) => void
    onAddProducts?: () => void
    onOpenDelivery?: (rowKey: string) => void
    coverageHint?: (sku: string, regionCode: string) => CoverageCode
    procurementFetching?: boolean
    procurementError?: boolean
    activeFilter?: SalesLineFilterKey
    onActiveFilterChange?: (filter: SalesLineFilterKey) => void
}

const NOOP_COVERAGE = (): CoverageCode => "NONE"

export function SalesOrderCreateLineItemTable({
    form,
    skuDetails,
    inquiryLocked = false,
    inquiryDeliveryLineKeys,
    procurementOwners,
    onPickSku,
    onAddProducts,
    onOpenDelivery,
    coverageHint = NOOP_COVERAGE,
    procurementFetching = false,
    procurementError = false,
    activeFilter: controlledFilter,
    onActiveFilterChange,
}: SalesOrderCreateLineItemTableProps) {
    const regions = useRegionIndex()
    const nature = useSelector(form.store, (state) => state.values.nature)
    const lines = useSelector(form.store, (state) => state.values.lineItems)
    const taxRatePercent = useSelector(
        form.store,
        (state) => state.values.taxRatePercent,
    )

    const [uncontrolledFilter, setUncontrolledFilter] =
        React.useState<SalesLineFilterKey>("all")
    const filter = controlledFilter ?? uncontrolledFilter
    const setFilter = onActiveFilterChange ?? setUncontrolledFilter

    const [searchQuery, setSearchQuery] = React.useState("")
    const [page, setPage] = React.useState(1)
    const [pageSize, setPageSize] = React.useState<number>(20)
    const [selectedKeys, setSelectedKeys] = React.useState<Set<string>>(
        () => new Set(),
    )
    const [removedEntries, setRemovedEntries] = React.useState<RemovedEntry[]>(
        [],
    )
    const [batchAddressOpen, setBatchAddressOpen] = React.useState(false)
    const [batchDueDateOpen, setBatchDueDateOpen] = React.useState(false)
    const [batchDueDate, setBatchDueDate] = React.useState("")

    // 方案带入的行尚无商品池参考价；仅补齐同一修订，后台刷新不覆盖已编辑单价。
    React.useEffect(() => {
        if (inquiryLocked || nature !== "physical_service") return
        form.getFieldValue("lineItems").forEach((line, index) => {
            if (line.referencePrices) return
            const reference = skuDetails.references.get(line.skuRevisionId)
            if (!reference || reference.skuId !== line.sku) return
            form.setFieldValue(`lineItems[${index}].referencePrices`, reference)
            if (
                !line.procurementPriceBasis &&
                !line.procurementSupplyPrice?.trim()
            ) {
                form.setFieldValue(
                    `lineItems[${index}].procurementPriceBasis`,
                    "DROPSHIP",
                )
                form.setFieldValue(
                    `lineItems[${index}].procurementSupplyPrice`,
                    reference.salesVisiblePriceGross,
                )
            }
        })
    }, [form, inquiryLocked, nature, skuDetails.references])

    // 清理已删除行的选中态
    React.useEffect(() => {
        setSelectedKeys((prev) => {
            if (prev.size === 0) return prev
            const validKeys = new Set(lines.map((line) => line.rowKey))
            let changed = false
            const next = new Set<string>()
            for (const key of prev) {
                if (validKeys.has(key)) next.add(key)
                else changed = true
            }
            return changed ? next : prev
        })
    }, [lines])

    // 切换搜索或筛选条件时回到第一页
    React.useEffect(() => {
        setPage(1)
    }, [filter, searchQuery, pageSize])

    /** 明细表根路径校验错误（如卡券仅一条）在明细区汇总展示。 */
    const lineItemIssues = useSelector(form.store, (state) => {
        return toFieldErrors(state.fieldMeta.lineItems?.errors ?? [])
            .filter((error) => Boolean(error?.message))
            .map((error, index) => ({
                id: `line-items-${index}`,
                label: "销售明细",
                message: error!.message!,
                targetId: "sales-line-items-section",
            }))
    })

    const { entries: filteredEntries, counts } = React.useMemo(
        () =>
            nature === "physical_service"
                ? buildFilteredSalesLines(lines, {
                      query: searchQuery,
                      filter,
                      procurementOwners,
                      procurementFetching,
                      regions: regions.data,
                  })
                : {
                      entries: [],
                      counts: {
                          all: lines.length,
                          incomplete: 0,
                          noAddress: 0,
                          addressIssue: 0,
                          negativeProfit: 0,
                      },
                  },
        [
            filter,
            lines,
            nature,
            procurementFetching,
            procurementOwners,
            regions.data,
            searchQuery,
        ],
    )

    // 如果当前激活的问题筛选结果被全部修完，平滑回到全部视图
    React.useEffect(() => {
        if (filter !== "all" && filteredEntries.length === 0 && !searchQuery) {
            setFilter("all")
        }
    }, [filter, filteredEntries.length, searchQuery, setFilter])

    const totalPages = Math.max(1, Math.ceil(filteredEntries.length / pageSize))
    const safePage = Math.min(page, totalPages)
    const startOffset = (safePage - 1) * pageSize
    const pageEntries = filteredEntries.slice(
        startOffset,
        startOffset + pageSize,
    )

    const unified = React.useMemo(
        () =>
            nature === "physical_service"
                ? detectUnifiedAddress(lines)
                : undefined,
        [lines, nature],
    )
    const unifiedAddressKey = unified ? addressKey(unified) : undefined
    const addressPool = React.useMemo(
        () =>
            nature === "physical_service" ? collectOrderAddressPool(lines) : [],
        [lines, nature],
    )

    const procurementStatus = React.useMemo(
        () => ({ fetching: procurementFetching, error: procurementError }),
        [procurementError, procurementFetching],
    )

    const handleToggleSelect = React.useCallback(
        (rowKey: string, checked: boolean) => {
            setSelectedKeys((prev) => {
                const next = new Set(prev)
                if (checked) next.add(rowKey)
                else next.delete(rowKey)
                return next
            })
        },
        [],
    )

    const handleDuplicateLine = React.useCallback(
        (sourceLine: SalesOrderDraftLineInput, rowIndex: number) => {
            const empty = createEmptyLine("physical_service")
            const cloned: SalesOrderDraftLineInput = {
                ...sourceLine,
                rowKey: empty.rowKey,
                deliveryAllocations: sourceLine.deliveryAllocations
                    ? sourceLine.deliveryAllocations.map((row) => ({ ...row }))
                    : undefined,
            }
            const current = [...form.getFieldValue("lineItems")]
            current.splice(rowIndex + 1, 0, cloned)
            form.setFieldValue("lineItems", current)
            toast.add({
                title: "已复制商品明细",
                description: `已复制「${sourceLine.name || "商品"}」，可单独调整数量、单价或收货地址。`,
                type: "success",
                timeout: 2500,
            })
        },
        [form],
    )

    const handleRemoveLine = React.useCallback(
        (targetLine: SalesOrderDraftLineInput, rowIndex: number) => {
            setRemovedEntries([{ line: targetLine, index: rowIndex }])
            void form.removeFieldValue("lineItems", rowIndex)
        },
        [form],
    )

    const handleOpenDelivery = React.useCallback(
        (rowKey: string) => {
            onOpenDelivery?.(rowKey)
        },
        [onOpenDelivery],
    )

    const hasUnresolvedOwner =
        nature === "physical_service" &&
        lines.some((line) => line.sku.trim()) &&
        !procurementFetching &&
        lines.some(
            (line) =>
                line.sku.trim() &&
                !procurementOwners?.get(line.rowKey)?.resolved,
        )

    // 卡券固定单行明细，继续沿用轻量专用列视图
    if (nature === "card_voucher") {
        return (
            <>
                <form.AppField name="lineItems" mode="array">
                    {() => null}
                </form.AppField>
                <form.Subscribe selector={(state) => state.values}>
                    {(values) => {
                        const columns = buildSalesOrderCreateLineItemColumns(
                            values,
                            form,
                            procurementOwners,
                            onPickSku,
                            procurementStatus,
                            inquiryLocked,
                            skuDetails.images,
                        )
                        return (
                            <>
                                {values.lineItems.length > 0 ? (
                                    <div
                                        data-slot="detail-inline-table"
                                        className={
                                            detailWorkspaceStyles.inlineTableWrapper
                                        }
                                    >
                                        <EditableLineItemTable
                                            id="sales-orders-create-line-items"
                                            className="[&_table]:[--table-cell-inline:0.625rem] [&_tbody_td]:py-2.5 [&_tbody_td]:align-top [&_tbody_td:last-child>div]:min-h-control [&_tbody_td:last-child>div]:items-center"
                                            items={values.lineItems}
                                            columns={columns}
                                            getRowId={(item) => item.rowKey}
                                            caption="销售单创建明细"
                                            emptyContent="至少需要一条销售明细。"
                                            onRemoveItem={(
                                                _item,
                                                _rowId,
                                                rowIndex,
                                            ) => {
                                                void form.removeFieldValue(
                                                    "lineItems",
                                                    rowIndex,
                                                )
                                            }}
                                            getRemoveDisabledReason={() =>
                                                "卡券销售单必须保留唯一明细"
                                            }
                                        />
                                    </div>
                                ) : null}
                                {lineItemIssues.length > 0 ? (
                                    <ValidationSummary
                                        issues={lineItemIssues}
                                        title={`明细共 ${lineItemIssues.length} 项待处理`}
                                    />
                                ) : null}
                            </>
                        )
                    }}
                </form.Subscribe>
            </>
        )
    }

    if (lines.length === 0) {
        return (
            <>
                <form.AppField name="lineItems" mode="array">
                    {() => null}
                </form.AppField>
                {lineItemIssues.length > 0 ? (
                    <ValidationSummary
                        issues={lineItemIssues}
                        title={`明细共 ${lineItemIssues.length} 项待处理`}
                    />
                ) : null}
            </>
        )
    }

    const visibleKeys = pageEntries.map((entry) => entry.line.rowKey)
    const selectedVisibleCount = visibleKeys.filter((key) =>
        selectedKeys.has(key),
    ).length
    const allVisibleSelected =
        visibleKeys.length > 0 && selectedVisibleCount === visibleKeys.length
    const someVisibleSelected =
        selectedVisibleCount > 0 && selectedVisibleCount < visibleKeys.length
    const selectedCount = selectedKeys.size

    const selectedAllocatedCount = lines.filter(
        (line) =>
            selectedKeys.has(line.rowKey) &&
            (line.deliveryAllocations?.length ?? 0) > 0,
    ).length

    const handleBatchAddressApply = (address: UnifiedDeliveryAddress) => {
        const current = form.getFieldValue("lineItems")
        const next = applyDeliveryAddressToSelectedLines(
            current,
            selectedKeys,
            address,
        )
        form.setFieldValue("lineItems", next)
        toast.add({
            title: `已为 ${selectedCount} 项商品设置收货地址`,
            description: "所选商品均按各自销售数量分配到该地址。",
            type: "success",
            timeout: 3000,
        })
        setSelectedKeys(new Set())
    }

    const handleBatchDueDateApply = () => {
        if (!batchDueDate) return
        const current = form.getFieldValue("lineItems")
        form.setFieldValue(
            "lineItems",
            current.map((line) =>
                selectedKeys.has(line.rowKey)
                    ? { ...line, dueDate: batchDueDate }
                    : line,
            ),
        )
        toast.add({
            title: `已为 ${selectedCount} 项商品设置交期`,
            description: `已将所选商品的期望交付日设为 ${batchDueDate}。`,
            type: "success",
            timeout: 3000,
        })
        setBatchDueDateOpen(false)
        setSelectedKeys(new Set())
    }

    const handleBatchPriceBasisApply = (basis: SalesProcurementPriceBasis) => {
        const current = form.getFieldValue("lineItems")
        let missing = 0
        const next = current.map((line) => {
            if (!selectedKeys.has(line.rowKey)) return line
            const supply = supplyReferencePrice(line.referencePrices, basis)
            if (!supply.trim()) missing += 1
            return {
                ...line,
                procurementPriceBasis: basis,
                procurementSupplyPrice: supply,
            }
        })
        form.setFieldValue("lineItems", next)
        const label = basis === "BULK" ? "集采" : "一件代发"
        toast.add({
            title: `已将 ${selectedCount} 项商品设为${label}`,
            description:
                missing > 0
                    ? `其中 ${missing} 项缺少对应参考价，请补填供给单价。`
                    : "已按对应参考价更新供给单价。",
            type: "success",
            timeout: missing > 0 ? 5000 : 3000,
        })
        setSelectedKeys(new Set())
    }

    const handleBatchRemove = () => {
        const current = form.getFieldValue("lineItems")
        const removed: RemovedEntry[] = []
        const kept: SalesOrderDraftLineInput[] = []
        current.forEach((line, index) => {
            if (selectedKeys.has(line.rowKey)) {
                removed.push({ line, index })
            } else {
                kept.push(line)
            }
        })
        if (removed.length === 0) return
        setRemovedEntries(removed)
        form.setFieldValue("lineItems", kept)
        setSelectedKeys(new Set())
    }

    const handleUndoRemove = () => {
        if (removedEntries.length === 0) return
        const current = [...form.getFieldValue("lineItems")]
        for (const entry of removedEntries) {
            current.splice(Math.min(entry.index, current.length), 0, entry.line)
        }
        form.setFieldValue("lineItems", current)
        setRemovedEntries([])
    }

    const filterPills: {
        key: SalesLineFilterKey
        label: string
        count: number
        tone?: "warning" | "destructive" | "neutral"
        alwaysShow?: boolean
    }[] = [
        { key: "all", label: "全部", count: counts.all, alwaysShow: true },
        {
            key: "incomplete",
            label: "待完善",
            count: counts.incomplete,
            tone: "warning",
        },
        {
            key: "no_address",
            label: "未设地址",
            count: counts.noAddress,
            tone: "neutral",
        },
        {
            key: "address_issue",
            label: "地址待核对",
            count: counts.addressIssue,
            tone: "warning",
        },
        {
            key: "negative_profit",
            label: "负毛利",
            count: counts.negativeProfit,
            tone: "destructive",
        },
    ]

    const showFilterToolbar =
        lines.length > 5 ||
        counts.addressIssue > 0 ||
        counts.negativeProfit > 0 ||
        Boolean(searchQuery) ||
        filter !== "all"

    const showInlineAdd = Boolean(
        !inquiryLocked && onAddProducts && lines.length > 5,
    )
    const showPagination =
        lines.length > 20 || filteredEntries.length > pageSize

    const useBoundedScroll = pageEntries.length > 8

    return (
        <div className="space-y-2.5">
            {skuDetails.error ? (
                <BusinessFailureState
                    id="sales-orders-create-line-images"
                    title={
                        inquiryLocked
                            ? "商品图片和单位资料读取失败"
                            : "商品图片资料读取失败"
                    }
                    error={skuDetails.error}
                    onRetry={() => void skuDetails.refetch()}
                />
            ) : null}

            <form.AppField name="lineItems" mode="array">
                {() => null}
            </form.AppField>

            {/* 搜索与状态快捷筛选栏（100 项商品时秒定位待完善/未设地址/负毛利行） */}
            {showFilterToolbar ? (
                <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-2">
                    <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                        <div className="relative w-56 max-w-full">
                            <SearchIcon
                                className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                                aria-hidden="true"
                            />
                            <Input
                                id="sales-orders-create-lines-search"
                                value={searchQuery}
                                onChange={(event) =>
                                    setSearchQuery(event.target.value)
                                }
                                placeholder="搜索商品、规格、收货人…"
                                className="h-control-sm pr-7 pl-8 text-body-compact"
                            />
                            {searchQuery ? (
                                <button
                                    id="sales-orders-create-lines-search-clear"
                                    type="button"
                                    aria-label="清空搜索"
                                    className="absolute top-1/2 right-2 -translate-y-1/2 cursor-pointer text-muted-foreground hover:text-foreground"
                                    onClick={() => setSearchQuery("")}
                                >
                                    <XIcon
                                        className="size-3.5"
                                        aria-hidden="true"
                                    />
                                </button>
                            ) : null}
                        </div>

                        <div
                            role="group"
                            aria-label="商品状态筛选"
                            className="flex flex-wrap items-center gap-1"
                        >
                            {filterPills
                                .filter(
                                    (pill) =>
                                        pill.alwaysShow ||
                                        pill.count > 0 ||
                                        filter === pill.key,
                                )
                                .map((pill) => {
                                    const active = filter === pill.key
                                    return (
                                        <button
                                            key={pill.key}
                                            id={`sales-orders-create-lines-filter-${pill.key}`}
                                            type="button"
                                            aria-pressed={active}
                                            onClick={() => setFilter(pill.key)}
                                            className={cn(
                                                "inline-flex h-control-sm cursor-pointer items-center gap-1 rounded-md px-2.5 text-xs transition-colors",
                                                active
                                                    ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
                                                    : pill.tone ===
                                                        "destructive"
                                                      ? "bg-destructive-soft/60 text-destructive-soft-foreground hover:bg-destructive-soft"
                                                      : pill.tone === "warning"
                                                        ? "bg-warning-soft/70 text-warning-soft-foreground hover:bg-warning-soft"
                                                        : "bg-muted/65 text-muted-foreground hover:bg-muted hover:text-foreground",
                                            )}
                                        >
                                            <span>{pill.label}</span>
                                            <span className="num">
                                                {pill.count}
                                            </span>
                                        </button>
                                    )
                                })}
                        </div>
                    </div>

                    {filteredEntries.length > pageSize || lines.length > 20 ? (
                        <span className="num text-xs text-muted-foreground">
                            显示第 {startOffset + 1}–
                            {Math.min(
                                startOffset + pageSize,
                                filteredEntries.length,
                            )}{" "}
                            项，共 {filteredEntries.length} 项
                        </span>
                    ) : null}
                </div>
            ) : null}

            {/* 批量勾选操作栏：勾选多件商品后可分批设收货地址、交期、取价方式或批量移除 */}
            {!inquiryLocked && selectedCount > 0 ? (
                <div
                    role="region"
                    aria-label="已选商品批量操作"
                    className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-primary/20 bg-sidebar-accent/55 px-3 py-1.5 text-body-compact"
                >
                    <div className="flex flex-wrap items-center gap-1.5">
                        <span className="font-medium text-sidebar-accent-foreground">
                            已选 <span className="num">{selectedCount}</span> /{" "}
                            <span className="num">{lines.length}</span> 项
                        </span>
                        {selectedCount < filteredEntries.length ? (
                            <Button
                                id="sales-orders-create-lines-select-all-filtered"
                                type="button"
                                variant="ghost"
                                size="xs"
                                onClick={() =>
                                    setSelectedKeys(
                                        new Set(
                                            filteredEntries.map(
                                                (entry) => entry.line.rowKey,
                                            ),
                                        ),
                                    )
                                }
                            >
                                全选当前{" "}
                                {filteredEntries.length === lines.length
                                    ? "全部"
                                    : "筛选结果"}{" "}
                                {filteredEntries.length} 项
                            </Button>
                        ) : null}
                        <span
                            aria-hidden="true"
                            className="mx-0.5 h-3.5 w-px bg-primary/20"
                        />
                        <Button
                            id="sales-orders-create-batch-address-trigger"
                            type="button"
                            variant="ghost"
                            size="xs"
                            className="text-primary hover:bg-card/80 hover:text-primary"
                            onClick={() => setBatchAddressOpen(true)}
                        >
                            <MapPinIcon
                                data-icon="inline-start"
                                className="size-3.5"
                                aria-hidden="true"
                            />
                            设地址
                        </Button>

                        <Popover
                            open={batchDueDateOpen}
                            onOpenChange={setBatchDueDateOpen}
                        >
                            <PopoverTrigger
                                id="sales-orders-create-selected-due-date-open"
                                render={
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        className="text-foreground hover:bg-card/80"
                                    />
                                }
                            >
                                <CalendarDaysIcon
                                    data-icon="inline-start"
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                设交期
                            </PopoverTrigger>
                            <PopoverContent align="start" className="w-72">
                                <PopoverTitle>
                                    为已选 {selectedCount} 项设置交期
                                </PopoverTitle>
                                <PopoverDescription>
                                    仅覆盖当前勾选商品的期望交付日。
                                </PopoverDescription>
                                <DatePicker
                                    id="sales-orders-create-selected-due-date"
                                    size="default"
                                    value={batchDueDate || undefined}
                                    onValueChange={(next) =>
                                        setBatchDueDate(next ?? "")
                                    }
                                    placeholder="选择日期"
                                    clearable={false}
                                    className="w-full"
                                />
                                <Button
                                    id="sales-orders-create-selected-due-date-apply"
                                    type="button"
                                    size="sm"
                                    disabled={!batchDueDate}
                                    onClick={handleBatchDueDateApply}
                                >
                                    应用到已选 {selectedCount} 项
                                </Button>
                            </PopoverContent>
                        </Popover>

                        <DropdownMenu>
                            <DropdownMenuTrigger
                                id="sales-orders-create-selected-price-basis-open"
                                render={
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        className="text-foreground hover:bg-card/80"
                                    />
                                }
                            >
                                设取价
                                <ChevronDownIcon
                                    data-icon="inline-end"
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                            </DropdownMenuTrigger>
                            <DropdownMenuContent align="start" className="w-56">
                                <DropdownMenuGroup>
                                    <DropdownMenuLabel>
                                        应用到已选 {selectedCount} 项商品
                                    </DropdownMenuLabel>
                                    <DropdownMenuItem
                                        id="sales-orders-create-selected-price-basis-bulk"
                                        onClick={() =>
                                            handleBatchPriceBasisApply("BULK")
                                        }
                                    >
                                        设为集采
                                    </DropdownMenuItem>
                                    <DropdownMenuItem
                                        id="sales-orders-create-selected-price-basis-dropship"
                                        onClick={() =>
                                            handleBatchPriceBasisApply(
                                                "DROPSHIP",
                                            )
                                        }
                                    >
                                        设为一件代发
                                    </DropdownMenuItem>
                                </DropdownMenuGroup>
                            </DropdownMenuContent>
                        </DropdownMenu>

                        <Button
                            id="sales-orders-create-batch-remove-selected"
                            type="button"
                            variant="ghost"
                            size="xs"
                            className={detailWorkspaceStyles.quietDangerButton}
                            onClick={handleBatchRemove}
                        >
                            <Trash2Icon
                                data-icon="inline-start"
                                className="size-3.5"
                                aria-hidden="true"
                            />
                            移除所选
                        </Button>
                    </div>
                    <Button
                        id="sales-orders-create-lines-clear-selection"
                        type="button"
                        variant="ghost"
                        size="xs"
                        onClick={() => setSelectedKeys(new Set())}
                    >
                        清空选择
                    </Button>
                </div>
            ) : null}

            {/* 撤销删除提示条 */}
            {removedEntries.length > 0 ? (
                <div
                    role="status"
                    className="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-muted/55 px-3.5 py-2 text-body-compact"
                >
                    <span>
                        {removedEntries.length === 1
                            ? `已移除「${removedEntries[0]!.line.name || "商品"}」`
                            : `已移除 ${removedEntries.length} 项商品`}
                    </span>
                    <div className="flex items-center gap-1">
                        <Button
                            id="sales-orders-create-lines-undo-remove"
                            type="button"
                            variant="ghost"
                            size="xs"
                            className={detailWorkspaceStyles.inlineLinkButton}
                            onClick={handleUndoRemove}
                        >
                            撤销
                        </Button>
                        <Button
                            id="sales-orders-create-lines-dismiss-undo"
                            type="button"
                            variant="ghost"
                            size="icon-sm"
                            aria-label="关闭移除提示"
                            onClick={() => setRemovedEntries([])}
                        >
                            <XIcon className="size-3.5" aria-hidden="true" />
                        </Button>
                    </div>
                </div>
            ) : null}

            {/* 高密度内联编辑主表（多行时表头吸顶在 table-container 滚动区内，不拖垮整页滚动） */}
            <div
                data-slot="detail-inline-table"
                className={cn(
                    detailWorkspaceStyles.inlineTableWrapper,
                    useBoundedScroll &&
                        "[&>[data-slot=table-container]]:max-h-[calc(100dvh-20rem)] [&>[data-slot=table-container]]:min-h-64 [&>[data-slot=table-container]]:overflow-y-auto",
                )}
            >
                <Table
                    id="sales-orders-create-line-items"
                    className="min-w-[68rem] [--table-cell-inline:0.5rem]"
                >
                    <caption className="sr-only">销售单创建明细</caption>
                    <TableHeader>
                        <TableRow className="hover:bg-transparent">
                            <TableHead className="w-14">
                                <div className="flex items-center gap-1.5">
                                    {!inquiryLocked ? (
                                        <NativeCheckbox
                                            id="sales-orders-create-lines-select-page"
                                            aria-label="全选当前页商品"
                                            checked={allVisibleSelected}
                                            indeterminate={someVisibleSelected}
                                            disabled={visibleKeys.length === 0}
                                            onCheckedChange={(checked) => {
                                                setSelectedKeys((prev) => {
                                                    const next = new Set(prev)
                                                    for (const key of visibleKeys) {
                                                        if (checked)
                                                            next.add(key)
                                                        else next.delete(key)
                                                    }
                                                    return next
                                                })
                                            }}
                                        />
                                    ) : null}
                                    <span className="num">#</span>
                                </div>
                            </TableHead>
                            <TableHead className="min-w-52">
                                销售项目
                                <RequiredMark />
                            </TableHead>
                            <TableHead data-align="end" className="w-26">
                                数量
                                <RequiredMark />
                            </TableHead>
                            <TableHead data-align="end" className="w-32">
                                <span className="inline-flex items-center justify-end gap-0.5">
                                    含税成交价
                                    <RequiredMark />
                                    <DetailHint
                                        id="sales-orders-create-unit-price-hint"
                                        label="含税成交单价"
                                        icon={
                                            <CircleHelpIcon
                                                className="size-3.5"
                                                aria-hidden="true"
                                            />
                                        }
                                    >
                                        {inquiryLocked
                                            ? "优先带入询价销售价，可修改。未填销售价的商品须补填。"
                                            : "由销售填写大于 0 的含税成交单价。"}
                                    </DetailHint>
                                </span>
                            </TableHead>
                            <TableHead data-align="end" className="w-44">
                                <span className="inline-flex items-center justify-end gap-0.5">
                                    采购供给价
                                    <RequiredMark />
                                    <DetailHint
                                        id="sales-orders-create-supply-price-hint"
                                        label="采购供给单价"
                                        icon={
                                            <CircleHelpIcon
                                                className="size-3.5"
                                                aria-hidden="true"
                                            />
                                        }
                                    >
                                        {inquiryLocked
                                            ? "由询价结果覆盖，不可修改；按单价乘数量估算含税毛利。"
                                            : "左侧可下拉切换一件代发或集采参考价，也可直接手填；用于估算含税毛利。"}
                                    </DetailHint>
                                </span>
                            </TableHead>
                            <TableHead className="w-34">
                                期望交付日
                                <RequiredMark />
                            </TableHead>
                            <TableHead className="min-w-44">收货地址</TableHead>
                            <TableHead data-align="end" className="w-28">
                                含税小计
                            </TableHead>
                            <TableHead className="w-18 text-right">
                                操作
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {pageEntries.length === 0 ? (
                            <TableRow>
                                <TableCell
                                    colSpan={9}
                                    className="py-10 text-center text-xs text-muted-foreground"
                                >
                                    当前筛选条件下没有匹配的商品。
                                    <Button
                                        id="sales-orders-create-lines-reset-filter"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        className={cn(
                                            "ml-1",
                                            detailWorkspaceStyles.inlineLinkButton,
                                        )}
                                        onClick={() => {
                                            setSearchQuery("")
                                            setFilter("all")
                                        }}
                                    >
                                        查看全部商品
                                    </Button>
                                </TableCell>
                            </TableRow>
                        ) : (
                            pageEntries.map(({ line, rowIndex }) => (
                                <SalesOrderCreateLineRow
                                    key={line.rowKey}
                                    form={form}
                                    line={line}
                                    rowIndex={rowIndex}
                                    taxRatePercent={taxRatePercent}
                                    inquiryLocked={inquiryLocked}
                                    displayUnit={
                                        skuDetails.unitNames.get(
                                            line.unit.trim(),
                                        ) ?? ""
                                    }
                                    quantityFromDelivery={
                                        inquiryDeliveryLineKeys?.has(
                                            line.rowKey,
                                        ) ?? false
                                    }
                                    selected={selectedKeys.has(line.rowKey)}
                                    imageAssetId={skuDetails.images.get(
                                        line.sku,
                                    )}
                                    unifiedAddressKey={unifiedAddressKey}
                                    procurementOwner={procurementOwners?.get(
                                        line.rowKey,
                                    )}
                                    procurementStatus={procurementStatus}
                                    coverageHint={coverageHint}
                                    onToggleSelect={handleToggleSelect}
                                    onPickSku={onPickSku}
                                    onOpenDelivery={handleOpenDelivery}
                                    onDuplicate={handleDuplicateLine}
                                    onRemove={handleRemoveLine}
                                />
                            ))
                        )}
                    </TableBody>
                </Table>
            </div>

            {/* 底部快捷添加 + 分页条（多行时自然呈现） */}
            {showInlineAdd || showPagination ? (
                <div className="flex flex-wrap items-center justify-between gap-2 pt-0.5">
                    <div className="flex items-center gap-3">
                        {showInlineAdd ? (
                            <Button
                                id="sales-orders-create-line-items-inline-add"
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={
                                    detailWorkspaceStyles.inlineLinkButton
                                }
                                onClick={onAddProducts}
                            >
                                <PlusIcon
                                    data-icon="inline-start"
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                继续添加商品
                            </Button>
                        ) : null}
                    </div>

                    {showPagination ? (
                        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                            <div className="inline-flex items-center gap-1">
                                <span>每页</span>
                                <div className="inline-flex items-center rounded-md bg-muted/65 p-0.5">
                                    {PAGE_SIZE_OPTIONS.map((size) => (
                                        <button
                                            key={size}
                                            id={`sales-orders-create-lines-pagesize-${size}`}
                                            type="button"
                                            aria-pressed={pageSize === size}
                                            onClick={() => setPageSize(size)}
                                            className={cn(
                                                "num cursor-pointer rounded-sm px-2 py-0.5 text-xs transition-colors",
                                                pageSize === size
                                                    ? "bg-card font-medium text-foreground shadow-2xs"
                                                    : "text-muted-foreground hover:text-foreground",
                                            )}
                                        >
                                            {size}
                                        </button>
                                    ))}
                                </div>
                            </div>
                            <span className="num">
                                第 {safePage} / {totalPages} 页
                            </span>
                            <div className="flex items-center gap-1">
                                <Button
                                    id="sales-orders-create-lines-page-prev"
                                    type="button"
                                    variant="outline"
                                    size="icon-sm"
                                    aria-label="上一页"
                                    disabled={safePage <= 1}
                                    onClick={() =>
                                        setPage((current) =>
                                            Math.max(1, current - 1),
                                        )
                                    }
                                >
                                    <ChevronLeftIcon aria-hidden="true" />
                                </Button>
                                <Button
                                    id="sales-orders-create-lines-page-next"
                                    type="button"
                                    variant="outline"
                                    size="icon-sm"
                                    aria-label="下一页"
                                    disabled={safePage >= totalPages}
                                    onClick={() =>
                                        setPage((current) =>
                                            Math.min(totalPages, current + 1),
                                        )
                                    }
                                >
                                    <ChevronRightIcon aria-hidden="true" />
                                </Button>
                            </div>
                        </div>
                    ) : null}
                </div>
            ) : null}

            {hasUnresolvedOwner ? (
                <p role="status" className="text-xs text-destructive">
                    {procurementError
                        ? "采购负责人暂时无法匹配，请稍后重试。"
                        : "部分商品尚未匹配采购负责人，请联系管理员维护采购责任规则。"}
                    可先保存草稿，完成匹配后再提交审批。
                </p>
            ) : null}

            {lineItemIssues.length > 0 ? (
                <ValidationSummary
                    issues={lineItemIssues}
                    title={`明细共 ${lineItemIssues.length} 项待处理`}
                />
            ) : null}

            {/* 已勾选商品分批设地址弹窗 */}
            <SalesOrderCreateUnifiedAddressDialog
                mode="selected"
                hideTrigger
                open={batchAddressOpen}
                onOpenChange={setBatchAddressOpen}
                lineCount={selectedCount}
                allocatedLineCount={selectedAllocatedCount}
                existingAddresses={addressPool}
                onApply={handleBatchAddressApply}
            />
        </div>
    )
}
