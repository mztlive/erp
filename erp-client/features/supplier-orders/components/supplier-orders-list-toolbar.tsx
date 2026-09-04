"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import {
    FilterChip,
    ListToolbar,
    MultiOptionCombobox,
    OptionCombobox,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    SupplierOrdersAppliedChip,
    SupplierOrdersFilterKey,
} from "@/features/supplier-orders/hooks/use-supplier-orders-filters"
import type {
    CancelStatus,
    ListView,
    RefundStatus,
    SupplierFulfillmentStatus,
} from "@/features/supplier-orders/types"
import {
    CANCEL_STATUS_LABEL,
    CANCEL_STATUSES,
    FULFILLMENT_STATUS_LABEL,
    FULFILLMENT_STATUSES,
    REFUND_STATUS_LABEL,
    REFUND_STATUSES,
    VIEW_LABEL,
} from "@/features/supplier-orders/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

export type SupplierOrdersListToolbarProps = {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    view: ListView
    onViewChange: (view: ListView) => void
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    hasStructuredFilters: boolean
    appliedChips: readonly SupplierOrdersAppliedChip[]
    onRemoveFilter: (key: SupplierOrdersFilterKey) => void
    onApplyFilters: () => void
    onClearAllFilters: () => void
    onResetMoreFilters: () => void
    filterError: string | null
    setFilterError: SetState<string | null>
    supplierIdDraft: string | null
    setSupplierIdDraft: SetState<string | null>
    fulfillmentStatusesDraft: readonly SupplierFulfillmentStatus[]
    setFulfillmentStatusesDraft: SetState<SupplierFulfillmentStatus[]>
    cancelStatusesDraft: readonly CancelStatus[]
    setCancelStatusesDraft: SetState<CancelStatus[]>
    refundStatusesDraft: readonly RefundStatus[]
    setRefundStatusesDraft: SetState<RefundStatus[]>
    paidFromDraft: string
    setPaidFromDraft: SetState<string>
    paidToDraft: string
    setPaidToDraft: SetState<string>
}

/**
 * 供应商订单筛选工具栏（docs/ui-filter-design.md §8 公司商品池结构）：
 * 整个筛选区是唯一语义 <form>；收起态搜索框尾部提交箭头与展开态面板
 * 「应用全部筛选」走同一个 onApplyFilters。
 */
export function SupplierOrdersListToolbar({
    searchInputRef,
    view,
    onViewChange,
    searchDraft,
    onSearchDraftChange,
    panelOpen,
    setPanelOpen,
    hasStructuredFilters,
    appliedChips,
    onRemoveFilter,
    onApplyFilters,
    onClearAllFilters,
    onResetMoreFilters,
    filterError,
    setFilterError,
    supplierIdDraft,
    setSupplierIdDraft,
    fulfillmentStatusesDraft,
    setFulfillmentStatusesDraft,
    cancelStatusesDraft,
    setCancelStatusesDraft,
    refundStatusesDraft,
    setRefundStatusesDraft,
    paidFromDraft,
    setPaidFromDraft,
    paidToDraft,
    setPaidToDraft,
}: SupplierOrdersListToolbarProps) {
    const panelId = React.useId()
    const paidDateErrorId = React.useId()
    const hasChips = appliedChips.length > 0

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                onApplyFilters()
            }}
        >
            <ListToolbar
                savedView={
                    <ToggleGroup
                        value={[view]}
                        onValueChange={(values) => {
                            const next = values[0] as ListView | undefined
                            if (next) onViewChange(next)
                        }}
                        variant="outline"
                        spacing={0}
                        aria-label="列表视图"
                    >
                        {(Object.keys(VIEW_LABEL) as ListView[]).map((v) => (
                            <ToggleGroupItem
                                key={v}
                                value={v}
                                id={`supplier-orders-list-view-${toAutomationIdSegment(v)}`}
                            >
                                {VIEW_LABEL[v]}
                            </ToggleGroupItem>
                        ))}
                    </ToggleGroup>
                }
                search={
                    <InputGroup className="w-full">
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            id="supplier-orders-list-search-input"
                            ref={searchInputRef}
                            data-slot="sfo-list-search"
                            value={searchDraft}
                            onChange={(event) =>
                                onSearchDraftChange(event.target.value)
                            }
                            placeholder="供应商订单号、外部单号"
                            aria-label="搜索供应商订单"
                        />
                    </InputGroup>
                }
                filters={
                    <Button
                        id="supplier-orders-list-filter-toggle"
                        type="button"
                        variant="outline"
                        aria-expanded={panelOpen}
                        aria-controls={panelId}
                        onClick={() => setPanelOpen((open) => !open)}
                    >
                        <FilterIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        更多筛选
                        {hasStructuredFilters ? (
                            <Badge variant="info">已启用</Badge>
                        ) : null}
                        <ChevronDownIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                            className={
                                panelOpen
                                    ? "rotate-180 transition-transform"
                                    : "transition-transform"
                            }
                        />
                    </Button>
                }
                secondary={
                    hasChips || panelOpen ? (
                        <div className="w-full space-y-3">
                            {hasChips ? (
                                <div className="flex flex-wrap items-center gap-2 border-t pt-3">
                                    <span className="text-xs text-muted-foreground">
                                        已筛选
                                    </span>
                                    {appliedChips.map((chip) => (
                                        <FilterChip
                                            key={chip.key}
                                            id={`supplier-orders-list-filter-chip-${toAutomationIdSegment(chip.key)}`}
                                            label={chip.label}
                                            clearLabel={`移除${chip.label}`}
                                            onClear={() =>
                                                onRemoveFilter(chip.key)
                                            }
                                        />
                                    ))}
                                    <Button
                                        id="supplier-orders-list-filter-clear-all"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        onClick={onClearAllFilters}
                                    >
                                        清空全部
                                    </Button>
                                </div>
                            ) : null}
                            {panelOpen ? (
                                <div
                                    id={panelId}
                                    className="flex w-full flex-col gap-3 border-t pt-3"
                                    aria-label="供应商订单更多筛选条件"
                                >
                                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                供应商
                                            </span>
                                            <SupplierSearchCombobox
                                                id="supplier-orders-list-filter-supplier"
                                                className="w-full"
                                                purpose="filter"
                                                value={
                                                    supplierIdDraft ?? undefined
                                                }
                                                onValueChange={(id) =>
                                                    setSupplierIdDraft(
                                                        id ?? null,
                                                    )
                                                }
                                                placeholder="全部供应商"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                履约状态
                                            </span>
                                            <MultiOptionCombobox
                                                id="supplier-orders-list-filter-fulfillment"
                                                className="w-full"
                                                value={fulfillmentStatusesDraft}
                                                onValueChange={(values) =>
                                                    setFulfillmentStatusesDraft(
                                                        values as SupplierFulfillmentStatus[],
                                                    )
                                                }
                                                options={FULFILLMENT_STATUSES.map(
                                                    (s) => ({
                                                        value: s,
                                                        label: FULFILLMENT_STATUS_LABEL[
                                                            s
                                                        ],
                                                    }),
                                                )}
                                                aria-label="履约状态"
                                                placeholder="全部履约状态"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                取消状态
                                            </span>
                                            <OptionCombobox
                                                id="supplier-orders-list-filter-cancel"
                                                className="w-full"
                                                value={
                                                    cancelStatusesDraft[0] ?? ""
                                                }
                                                onValueChange={(value) =>
                                                    setCancelStatusesDraft(
                                                        value
                                                            ? [
                                                                  value as CancelStatus,
                                                              ]
                                                            : [],
                                                    )
                                                }
                                                options={CANCEL_STATUSES.map(
                                                    (s) => ({
                                                        value: s,
                                                        label: CANCEL_STATUS_LABEL[
                                                            s
                                                        ],
                                                    }),
                                                )}
                                                aria-label="取消状态"
                                                placeholder="全部取消状态"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                退款状态
                                            </span>
                                            <OptionCombobox
                                                id="supplier-orders-list-filter-refund"
                                                className="w-full"
                                                value={
                                                    refundStatusesDraft[0] ?? ""
                                                }
                                                onValueChange={(value) =>
                                                    setRefundStatusesDraft(
                                                        value
                                                            ? [
                                                                  value as RefundStatus,
                                                              ]
                                                            : [],
                                                    )
                                                }
                                                options={REFUND_STATUSES.map(
                                                    (s) => ({
                                                        value: s,
                                                        label: REFUND_STATUS_LABEL[
                                                            s
                                                        ],
                                                    }),
                                                )}
                                                aria-label="退款状态"
                                                placeholder="全部退款状态"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm sm:col-span-2">
                                            <span className="text-muted-foreground">
                                                支付时间
                                            </span>
                                            <div className="flex items-center gap-1.5">
                                                <DatePicker
                                                    id="supplier-orders-list-filter-paid-from"
                                                    className="w-0 min-w-0 flex-1"
                                                    value={
                                                        paidFromDraft ||
                                                        undefined
                                                    }
                                                    onValueChange={(next) => {
                                                        setPaidFromDraft(
                                                            next ?? "",
                                                        )
                                                        setFilterError(null)
                                                    }}
                                                    placeholder="开始日期"
                                                    aria-invalid={Boolean(
                                                        filterError,
                                                    )}
                                                    aria-describedby={
                                                        filterError
                                                            ? paidDateErrorId
                                                            : undefined
                                                    }
                                                />
                                                <span className="text-muted-foreground">
                                                    至
                                                </span>
                                                <DatePicker
                                                    id="supplier-orders-list-filter-paid-to"
                                                    className="w-0 min-w-0 flex-1"
                                                    value={
                                                        paidToDraft || undefined
                                                    }
                                                    onValueChange={(next) => {
                                                        setPaidToDraft(
                                                            next ?? "",
                                                        )
                                                        setFilterError(null)
                                                    }}
                                                    placeholder="结束日期"
                                                    aria-invalid={Boolean(
                                                        filterError,
                                                    )}
                                                    aria-describedby={
                                                        filterError
                                                            ? paidDateErrorId
                                                            : undefined
                                                    }
                                                />
                                            </div>
                                            {filterError ? (
                                                <span
                                                    id={paidDateErrorId}
                                                    className="text-xs text-destructive"
                                                    role="alert"
                                                >
                                                    {filterError}
                                                </span>
                                            ) : null}
                                        </div>
                                    </div>
                                    <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                                        <p className="text-xs text-muted-foreground">
                                            将同时应用上方关键词和以下筛选条件；结果也用于导出。
                                        </p>
                                        <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                            <Button
                                                id="supplier-orders-list-filter-reset-more"
                                                type="button"
                                                variant="ghost"
                                                onClick={onResetMoreFilters}
                                            >
                                                重置更多条件
                                            </Button>
                                            <Button
                                                id="supplier-orders-list-filter-apply"
                                                type="submit"
                                            >
                                                <SearchIcon
                                                    data-icon="inline-start"
                                                    aria-hidden="true"
                                                />
                                                应用全部筛选
                                            </Button>
                                        </div>
                                    </div>
                                </div>
                            ) : null}
                        </div>
                    ) : undefined
                }
            />
        </form>
    )
}
