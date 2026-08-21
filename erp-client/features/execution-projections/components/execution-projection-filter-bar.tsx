"use client"

import * as React from "react"
import {
    ChevronDownIcon,
    FilterIcon,
    SearchIcon,
} from "lucide-react"

import {
    FilterChip,
    FixedOptionRadioFilter,
    ListToolbar,
    OptionCombobox,
    type ComboboxOption,
    type FixedOptionRadioFilterOption,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import type {
    ExecutionProjectionAppliedChip,
    ExecutionProjectionFilterKey,
    ExecutionProjectionFilterState,
} from "@/features/execution-projections/hooks/use-execution-projection-filters"
import {
    DELIVERY_STATUS_LABEL,
    LATENCY_LABEL,
    SOURCE_LABEL,
    type LatencyBand,
    type ProjectionSource,
    type ReconciliationStatus,
} from "@/features/execution-projections/types"

const LATENCY_FILTER_OPTIONS: ReadonlyArray<
    FixedOptionRadioFilterOption<LatencyBand | "all">
> = [
    { value: "all", label: "全部" },
    { value: "normal", label: LATENCY_LABEL.normal },
    { value: "near_sla", label: LATENCY_LABEL.near_sla },
    { value: "over_sla", label: LATENCY_LABEL.over_sla },
]

const RECONCILIATION_FILTER_OPTIONS: ReadonlyArray<
    FixedOptionRadioFilterOption<ReconciliationStatus | "all">
> = [
    { value: "all", label: "全部" },
    { value: "VERSION_MISMATCH", label: "仅版本差异" },
    { value: "MATCHED", label: "版本一致" },
    { value: "NONE", label: "无对账" },
]

const SOURCE_FILTER_OPTIONS: ReadonlyArray<
    FixedOptionRadioFilterOption<ProjectionSource | "all">
> = [
    { value: "all", label: "全部" },
    { value: "ERP_SALES_REVISION", label: SOURCE_LABEL.ERP_SALES_REVISION },
    { value: "MIGRATION_BASELINE", label: SOURCE_LABEL.MIGRATION_BASELINE },
]

const DELIVERY_STATUS_OPTIONS: readonly ComboboxOption[] = [
    { value: "all", label: "全部接收状态" },
    { value: "UNKNOWN", label: DELIVERY_STATUS_LABEL.UNKNOWN },
    { value: "FAILED", label: DELIVERY_STATUS_LABEL.FAILED },
    { value: "ESCALATED_MANUAL", label: DELIVERY_STATUS_LABEL.ESCALATED_MANUAL },
    { value: "RETRYING", label: DELIVERY_STATUS_LABEL.RETRYING },
    { value: "SENDING", label: DELIVERY_STATUS_LABEL.SENDING },
    { value: "PENDING", label: DELIVERY_STATUS_LABEL.PENDING },
    { value: "ACKED", label: DELIVERY_STATUS_LABEL.ACKED },
    { value: "UNKNOWN,FAILED,ESCALATED_MANUAL", label: "未知+失败+转人工" },
]

export function ExecutionProjectionFilterBar({
    filters,
    appliedChips,
    removeFilter,
    hasChips,
    malls,
}: {
    filters: ExecutionProjectionFilterState
    appliedChips: readonly ExecutionProjectionAppliedChip[]
    removeFilter: (key: ExecutionProjectionFilterKey) => void
    hasChips: boolean
    malls: Array<{ id: string; name: string }>
}) {
    const panelId = React.useId()
    const { panelOpen, setPanelOpen } = filters

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                filters.applyFilters()
            }}
        >
            <ListToolbar
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            ref={filters.searchInputRef}
                            value={filters.searchDraft}
                            onChange={(event) =>
                                filters.setSearchDraft(event.target.value)
                            }
                            placeholder="销售单号、客户"
                            aria-label="搜索执行信息"
                        />
                        
                    </InputGroup>
                }
                filters={
                    <Button
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
                        {filters.hasStructuredFilters ? (
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
                                            label={chip.label}
                                            clearLabel={`移除${chip.label}`}
                                            onClear={() =>
                                                removeFilter(chip.key)
                                            }
                                        />
                                    ))}
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        onClick={filters.clearAllFilters}
                                    >
                                        清空全部
                                    </Button>
                                </div>
                            ) : null}
                            {panelOpen ? (
                                <div
                                    id={panelId}
                                    className="flex w-full flex-col gap-3 border-t pt-3"
                                    aria-label="执行信息更多筛选条件"
                                >
                                    <FixedOptionRadioFilter
                                        label="等待时长"
                                        value={filters.latencyDraft}
                                        onValueChange={filters.setLatencyDraft}
                                        options={LATENCY_FILTER_OPTIONS}
                                    />
                                    <FixedOptionRadioFilter
                                        label="版本核对"
                                        value={filters.reconciliationDraft}
                                        onValueChange={
                                            filters.setReconciliationDraft
                                        }
                                        options={RECONCILIATION_FILTER_OPTIONS}
                                    />
                                    <FixedOptionRadioFilter
                                        label="数据来源"
                                        value={filters.sourceDraft}
                                        onValueChange={filters.setSourceDraft}
                                        options={SOURCE_FILTER_OPTIONS}
                                    />
                                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                目标商城
                                            </span>
                                            <OptionCombobox
                                                className="w-full"
                                                value={filters.mallIdDraft}
                                                onValueChange={(value) =>
                                                    filters.setMallIdDraft(
                                                        value ?? "all",
                                                    )
                                                }
                                                options={[
                                                    {
                                                        value: "all",
                                                        label: "全部商城",
                                                    },
                                                    ...malls.map((mall) => ({
                                                        value: mall.id,
                                                        label: mall.name,
                                                    })),
                                                ]}
                                                placeholder="全部商城"
                                                searchPlaceholder="搜索商城名称"
                                                aria-label="目标商城"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                接收状态
                                            </span>
                                            <OptionCombobox
                                                className="w-full"
                                                value={filters.deliveryStatusDraft}
                                                onValueChange={(value) =>
                                                    filters.setDeliveryStatusDraft(
                                                        value ?? "all",
                                                    )
                                                }
                                                options={
                                                    DELIVERY_STATUS_OPTIONS
                                                }
                                                placeholder="全部接收状态"
                                                searchPlaceholder="搜索接收状态"
                                                aria-label="接收状态"
                                            />
                                        </div>
                                    </div>
                                    <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                                        <p className="text-xs text-muted-foreground">
                                            将同时应用上方关键词和以下筛选条件；结果也用于导出。
                                        </p>
                                        <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                            <Button
                                                type="button"
                                                variant="ghost"
                                                onClick={
                                                    filters.resetMoreFilters
                                                }
                                            >
                                                重置更多条件
                                            </Button>
                                            <Button type="submit">
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
