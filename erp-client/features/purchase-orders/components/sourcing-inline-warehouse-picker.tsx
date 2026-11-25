"use client"

import { useState } from "react"
import {
    CheckIcon,
    ChevronDownIcon,
    SearchIcon,
    TriangleAlertIcon,
    WarehouseIcon,
    XIcon,
} from "lucide-react"
import { ComboboxRefresh } from "@/components/business/combobox-refresh"
import type { WarehouseComboboxItem } from "@/components/business/entity-comboboxes"
import { SelectorQueryFeedback } from "@/components/business/selector-query-feedback"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Popover,
    PopoverContent,
    PopoverTrigger,
} from "@/components/ui/popover"
import {
    useDebouncedSearch,
    useWarehouseSelectorQuery,
} from "@/features/entity-selectors"
import { useRemoteSearchCombobox } from "@/features/entity-selectors/hooks/use-remote-search-combobox"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import {
    deliveryTermLabel,
    warehouseDeliveryTerm,
} from "../lib/sourcing/delivery-terms"
import type { SourcingSupplierOption } from "../lib/sourcing/types"

/**
 * 供给表格行内轻量化目标仓选择器：
 * 与 `SourcingInlineOptionPicker` 保持同一套平面细线触发器与浮层交互，
 * 避免在紧凑表格单元格内塞入带清除按钮的厚重输入框。
 */
export function SourcingInlineWarehousePicker({
    id,
    value,
    onSelect,
    option,
    productName,
    missing = false,
    otherWarehouseTargets = 0,
    onSyncToOthers,
}: {
    id: string
    value?: string
    onSelect: (warehouse: WarehouseComboboxItem) => void
    option?: SourcingSupplierOption
    productName: string
    missing?: boolean
    otherWarehouseTargets?: number
    onSyncToOthers?: (resolvedWarehouseName?: string) => void
}) {
    const [open, setOpen] = useState(false)
    const [rawQuery, setRawQuery] = useState("")
    const debouncedQuery = useDebouncedSearch(rawQuery)

    const query = useWarehouseSelectorQuery(
        { query: debouncedQuery, purpose: "purchase-receipt" },
        value,
    )
    const {
        rows,
        denied,
        onRefresh,
        loading,
        emptyLabel: resolvedEmptyLabel,
    } = useRemoteSearchCombobox({
        selectedId: value,
        list: query.list,
        selected: query.selected,
        idOf: (item) => item.warehouseId,
        emptyLabel: "没有可用仓库",
        fallbackError: "仓库加载失败，请重试",
    })

    if (denied) return null

    const selectedWarehouse = value
        ? rows.find((item) => item.warehouseId === value)
        : undefined
    const displayLabel = !value
        ? "选择目标仓"
        : selectedWarehouse
          ? selectedWarehouse.warehouseName
          : loading
            ? "核对中…"
            : "仓库不可用"

    const hasQueryError = query.list.isError || query.selected.isError
    const noScope =
        !query.list.isFetching &&
        !query.list.isError &&
        query.list.emptyReason === "no_scope"

    const judgement = value ? warehouseDeliveryTerm(option, value) : undefined
    const isUndeliverable = judgement?.term === "NOT_DELIVERABLE"
    const isFreightExtra = judgement?.term === "FREIGHT_EXTRA"
    const isUnavailable = Boolean(value) && !loading && !selectedWarehouse

    const triggerTitle = selectedWarehouse
        ? (() => {
              const base =
                  selectedWarehouse.warehouseCode &&
                  selectedWarehouse.warehouseCode !==
                      selectedWarehouse.warehouseName
                      ? `${selectedWarehouse.warehouseName}（${selectedWarehouse.warehouseCode}）`
                      : selectedWarehouse.warehouseName
              return judgement && option
                  ? `${base} · ${judgement.regionName}（${deliveryTermLabel(option, judgement.term)}）`
                  : base
          })()
        : missing
          ? "请选择入库目标仓"
          : isUnavailable
            ? "当前目标仓不可用，请重新选择"
            : undefined

    const triggerToneClass =
        isUndeliverable || isUnavailable
            ? "border-destructive-border bg-destructive-soft/50 text-destructive-soft-foreground hover:bg-destructive-soft/80 data-[popup-open]:border-destructive"
            : missing && !value
              ? "border-dashed border-warning-border bg-warning-soft/55 font-medium text-warning-soft-foreground hover:bg-warning-soft data-[popup-open]:border-warning-border data-[popup-open]:bg-warning-soft"
              : isFreightExtra
                ? "border-warning-border/80 bg-warning-soft/40 text-warning-soft-foreground hover:bg-warning-soft/75 data-[popup-open]:border-warning-border"
                : "border-border/75 bg-surface-sunken/75 text-foreground hover:border-primary/45 hover:bg-card data-[popup-open]:border-primary data-[popup-open]:bg-card data-[popup-open]:ring-2 data-[popup-open]:ring-ring"

    const LeadingIcon =
        isUndeliverable || isFreightExtra ? TriangleAlertIcon : WarehouseIcon

    return (
        <Popover
            open={open}
            onOpenChange={(next) => {
                setOpen(next)
                if (!next) setRawQuery("")
            }}
        >
            <PopoverTrigger
                render={
                    <button
                        id={id}
                        type="button"
                        onClick={(event) => event.stopPropagation()}
                        title={triggerTitle}
                        aria-label={`选择 ${productName} 的入库目标仓${selectedWarehouse ? `，当前 ${selectedWarehouse.warehouseName}` : ""}${missing && !value ? "，待选目标仓" : isUndeliverable ? "，该仓不配送" : isUnavailable ? "，当前目标仓不可用" : ""}`}
                        data-invalid={
                            missing ||
                            isUndeliverable ||
                            isUnavailable ||
                            undefined
                        }
                        className={cn(
                            "group/wh inline-flex h-control-xs w-fit max-w-full min-w-0 cursor-pointer items-center gap-1 rounded-sm border px-1.5 text-xs transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                            triggerToneClass,
                        )}
                    />
                }
            >
                <LeadingIcon
                    aria-hidden="true"
                    className={cn(
                        "size-3 shrink-0",
                        !(isUndeliverable || isFreightExtra) && "opacity-75",
                    )}
                />
                <span className="min-w-0 truncate">{displayLabel}</span>
                <ChevronDownIcon
                    aria-hidden="true"
                    className="size-3 shrink-0 opacity-65 transition-transform group-hover/wh:opacity-100 group-data-[popup-open]/wh:rotate-180"
                />
            </PopoverTrigger>

            <PopoverContent
                align="start"
                className="w-[min(22rem,calc(100vw-2rem))] gap-0 overflow-hidden rounded-lg p-0"
                onClick={(event) => event.stopPropagation()}
            >
                <div className="flex items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken px-3 py-2">
                    <span className="shrink-0 text-xs font-semibold text-foreground">
                        选择入库目标仓
                    </span>
                    <span
                        className="min-w-0 truncate text-tiny text-muted-foreground"
                        title={productName}
                    >
                        {productName}
                    </span>
                </div>

                <div className="border-b border-border/60 px-2.5 py-2">
                    <div className="relative">
                        <SearchIcon
                            aria-hidden="true"
                            className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                        />
                        <Input
                            id={`${id}-search`}
                            size="sm"
                            aria-label="搜索仓库名称或编码"
                            placeholder="搜索仓库名称或编码"
                            className="pr-7 pl-8"
                            value={rawQuery}
                            onChange={(event) =>
                                setRawQuery(event.target.value)
                            }
                        />
                        {rawQuery ? (
                            <Button
                                id={`${id}-search-clear`}
                                type="button"
                                variant="ghost"
                                size="icon-xs"
                                aria-label="清除搜索"
                                className="absolute top-1/2 right-0.5 -translate-y-1/2"
                                onClick={() => setRawQuery("")}
                            >
                                <XIcon />
                            </Button>
                        ) : null}
                    </div>
                </div>

                {hasQueryError || noScope ? (
                    <div className="border-b border-border/60 px-3 py-2">
                        <SelectorQueryFeedback
                            id={id}
                            failed={hasQueryError}
                            error={query.list.error ?? query.selected.error}
                            noScope={noScope}
                            onRetry={() => {
                                void query.list.refetch()
                                if (value) void query.selected.refetch()
                            }}
                        />
                    </div>
                ) : null}

                <div className="max-h-64 overflow-y-auto overscroll-contain">
                    {rows.length === 0 ? (
                        <p className="px-4 py-6 text-center text-xs text-muted-foreground">
                            {loading
                                ? "正在加载仓库…"
                                : resolvedEmptyLabel || "没有可用仓库"}
                        </p>
                    ) : (
                        <ul className="divide-y divide-grid">
                            {rows.map((warehouse) => {
                                const selected = warehouse.warehouseId === value
                                const termInfo = warehouseDeliveryTerm(
                                    option,
                                    warehouse.warehouseId,
                                )
                                const showCode =
                                    Boolean(warehouse.warehouseCode) &&
                                    warehouse.warehouseCode !==
                                        warehouse.warehouseName
                                return (
                                    <li key={warehouse.warehouseId}>
                                        <button
                                            id={`${id}-option-${toAutomationIdSegment(warehouse.warehouseId)}`}
                                            type="button"
                                            onClick={() => {
                                                onSelect(warehouse)
                                                setRawQuery("")
                                                setOpen(false)
                                            }}
                                            className={cn(
                                                "flex w-full cursor-pointer items-center justify-between gap-2.5 px-3 py-2 text-left transition-colors outline-none hover:bg-row-hover focus-visible:bg-row-hover",
                                                selected &&
                                                    "bg-sidebar-accent/70",
                                            )}
                                        >
                                            <div className="min-w-0 flex-1 space-y-0.5">
                                                <div className="flex min-w-0 items-center gap-1.5">
                                                    {selected ? (
                                                        <CheckIcon
                                                            aria-hidden="true"
                                                            className="size-3.5 shrink-0 text-primary"
                                                        />
                                                    ) : null}
                                                    <span className="truncate text-body-compact font-medium text-foreground">
                                                        {
                                                            warehouse.warehouseName
                                                        }
                                                    </span>
                                                </div>
                                                {showCode || termInfo ? (
                                                    <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-tiny text-muted-foreground">
                                                        {showCode ? (
                                                            <span className="num">
                                                                {
                                                                    warehouse.warehouseCode
                                                                }
                                                            </span>
                                                        ) : null}
                                                        {termInfo && option ? (
                                                            <span
                                                                className={cn(
                                                                    termInfo.term ===
                                                                        "FREIGHT_INCLUDED"
                                                                        ? "text-success-soft-foreground"
                                                                        : termInfo.term ===
                                                                            "FREIGHT_EXTRA"
                                                                          ? "text-warning-soft-foreground"
                                                                          : "text-destructive",
                                                                )}
                                                            >
                                                                {
                                                                    termInfo.regionName
                                                                }{" "}
                                                                ·{" "}
                                                                {deliveryTermLabel(
                                                                    option,
                                                                    termInfo.term,
                                                                )}
                                                            </span>
                                                        ) : null}
                                                    </div>
                                                ) : null}
                                            </div>
                                        </button>
                                    </li>
                                )
                            })}
                        </ul>
                    )}
                </div>

                {otherWarehouseTargets > 0 && value && onSyncToOthers ? (
                    <div className="border-t border-border/60 bg-surface-sunken/50 px-3 py-1.5">
                        <button
                            id={`${id}-sync-all`}
                            type="button"
                            onClick={() => {
                                const resolvedName = selectedWarehouse
                                    ? selectedWarehouse.warehouseCode
                                        ? `${selectedWarehouse.warehouseCode} · ${selectedWarehouse.warehouseName}`
                                        : selectedWarehouse.warehouseName
                                    : undefined
                                onSyncToOthers(resolvedName)
                                setOpen(false)
                            }}
                            className="cursor-pointer text-xs font-medium text-primary hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                        >
                            同步当前目标仓到其余 {otherWarehouseTargets}{" "}
                            个入仓项
                        </button>
                    </div>
                ) : null}

                <ComboboxRefresh
                    id={id}
                    onRefresh={onRefresh}
                    loading={loading}
                />
            </PopoverContent>
        </Popover>
    )
}
