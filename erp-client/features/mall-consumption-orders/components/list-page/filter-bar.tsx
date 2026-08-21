"use client"

import * as React from "react"
import {
    ChevronDownIcon,
    FilterIcon,
    SearchIcon,
} from "lucide-react"

import {
    FilterChip,
    FixedOptionCheckboxFilter,
    FixedOptionRadioFilter,
    ListToolbar,
    MultiOptionCombobox,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { DateRangePicker } from "@/components/ui/date-picker"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { MallSearchCombobox } from "@/features/entity-selectors"
import {
    ATTRIBUTION_STATUS_OPTIONS,
    COST_BASIS_OPTIONS,
    DATA_SOURCE_OPTIONS,
    FACT_TYPE_OPTIONS,
    FULFILLMENT_CHAIN_OPTIONS,
    PAYMENT_SOURCE_OPTIONS,
    SUPPLIER_STATUS_OPTIONS,
    type MallConsumptionAppliedChip,
    type MallConsumptionOrderFilterDraft,
    type MallConsumptionOrderFilterKey,
} from "@/features/mall-consumption-orders/lib/filters"
import type { SupplierFulfillmentStatus } from "@/features/mall-consumption-orders/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

type Props = {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    hasStructuredFilters: boolean
    appliedChips: readonly MallConsumptionAppliedChip[]
    onRemoveFilter: (key: MallConsumptionOrderFilterKey) => void
    onApplyFilters: () => void
    onClearAllFilters: () => void
    onResetMoreFilters: () => void
    filterDraft: MallConsumptionOrderFilterDraft
    setFilterDraft: SetState<MallConsumptionOrderFilterDraft>
}

/**
 * 商城消费订单筛选工具栏（docs/ui-filter-design.md §8 公司商品池结构）：
 * 整个筛选区是唯一语义 <form>；收起态搜索框尾部提交箭头与展开态面板
 * 「应用全部筛选」走同一个 onApplyFilters。
 * 记录发生期间作为分析期间维度放在「更多筛选」面板内，
 * 「清空全部」与「重置更多条件」都保留期间。
 */
export function ConsumptionOrderFilterBar({
    searchInputRef,
    searchDraft,
    setSearchDraft,
    panelOpen,
    setPanelOpen,
    hasStructuredFilters,
    appliedChips,
    onRemoveFilter,
    onApplyFilters,
    onClearAllFilters,
    onResetMoreFilters,
    filterDraft,
    setFilterDraft,
}: Props) {
    const panelId = React.useId()
    const hasChips = appliedChips.length > 0

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                onApplyFilters()
            }}
        >
            <ListToolbar
                search={
                    <InputGroup className="w-full">
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            ref={searchInputRef}
                            value={searchDraft}
                            onChange={(event) =>
                                setSearchDraft(event.target.value)
                            }
                            placeholder="商城单号、客户、ERP 编号"
                            aria-label="搜索消费订单"
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
                                            label={chip.label}
                                            clearLabel={`移除${chip.label}`}
                                            onClear={() =>
                                                onRemoveFilter(chip.key)
                                            }
                                        />
                                    ))}
                                    <Button
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
                                    aria-label="商城消费订单更多筛选条件"
                                >
                                    <FixedOptionRadioFilter
                                        label="归集状态"
                                        value={filterDraft.attributionStatus}
                                        onValueChange={(attributionStatus) =>
                                            setFilterDraft((current) => ({
                                                ...current,
                                                attributionStatus,
                                            }))
                                        }
                                        options={ATTRIBUTION_STATUS_OPTIONS}
                                    />
                                    <FixedOptionRadioFilter
                                        label="履约链"
                                        value={filterDraft.fulfillmentChain}
                                        onValueChange={(fulfillmentChain) =>
                                            setFilterDraft((current) => ({
                                                ...current,
                                                fulfillmentChain,
                                            }))
                                        }
                                        options={FULFILLMENT_CHAIN_OPTIONS}
                                    />
                                    <FixedOptionRadioFilter
                                        label="支付方式"
                                        value={filterDraft.paymentSource}
                                        onValueChange={(paymentSource) =>
                                            setFilterDraft((current) => ({
                                                ...current,
                                                paymentSource,
                                            }))
                                        }
                                        options={PAYMENT_SOURCE_OPTIONS}
                                    />
                                    <FixedOptionRadioFilter
                                        label="成本口径"
                                        value={filterDraft.costBasis}
                                        onValueChange={(costBasis) =>
                                            setFilterDraft((current) => ({
                                                ...current,
                                                costBasis,
                                            }))
                                        }
                                        options={COST_BASIS_OPTIONS}
                                    />
                                    <FixedOptionCheckboxFilter
                                        label="事实类型"
                                        value={filterDraft.factTypes}
                                        onValueChange={(factTypes) =>
                                            setFilterDraft((current) => ({
                                                ...current,
                                                factTypes,
                                            }))
                                        }
                                        options={FACT_TYPE_OPTIONS}
                                    />
                                    <FixedOptionCheckboxFilter
                                        label="数据来源"
                                        value={filterDraft.dataSources}
                                        onValueChange={(dataSources) =>
                                            setFilterDraft((current) => ({
                                                ...current,
                                                dataSources,
                                            }))
                                        }
                                        options={DATA_SOURCE_OPTIONS}
                                    />
                                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                来源商城
                                            </span>
                                            <MallSearchCombobox
                                                purpose="filter"
                                                className="w-full"
                                                value={
                                                    filterDraft.mallId || null
                                                }
                                                onValueChange={(mallId) =>
                                                    setFilterDraft(
                                                        (current) => ({
                                                            ...current,
                                                            mallId: mallId ?? "",
                                                        }),
                                                    )
                                                }
                                                aria-label="来源商城"
                                                placeholder="全部商城"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                供应商状态
                                            </span>
                                            <MultiOptionCombobox
                                                className="w-full"
                                                value={
                                                    filterDraft.supplierStatuses
                                                }
                                                onValueChange={(
                                                    supplierStatuses,
                                                ) =>
                                                    setFilterDraft(
                                                        (current) => ({
                                                            ...current,
                                                            supplierStatuses:
                                                                supplierStatuses as SupplierFulfillmentStatus[],
                                                        }),
                                                    )
                                                }
                                                options={
                                                    SUPPLIER_STATUS_OPTIONS
                                                }
                                                aria-label="供应商状态"
                                                placeholder="全部状态"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm sm:col-span-2">
                                            <span className="text-muted-foreground">
                                                记录发生时间
                                            </span>
                                            <DateRangePicker
                                                className="w-full"
                                                value={
                                                    filterDraft.occurredFrom ||
                                                    filterDraft.occurredTo
                                                        ? {
                                                              from:
                                                                  filterDraft.occurredFrom ||
                                                                  undefined,
                                                              to:
                                                                  filterDraft.occurredTo ||
                                                                  undefined,
                                                          }
                                                        : undefined
                                                }
                                                onValueChange={(range) =>
                                                    setFilterDraft(
                                                        (current) => ({
                                                            ...current,
                                                            occurredFrom:
                                                                range?.from ??
                                                                "",
                                                            occurredTo:
                                                                range?.to ?? "",
                                                        }),
                                                    )
                                                }
                                                placeholder="全部时间"
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
                                                onClick={onResetMoreFilters}
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
