"use client"

import type * as React from "react"
import { SearchIcon } from "lucide-react"

import { ListToolbar, MultiOptionCombobox, OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import { DateRangePicker } from "@/components/ui/date-picker"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { MallSearchCombobox } from "@/features/entity-selectors"
import {
    DATA_SOURCES,
    FACT_TYPES,
    SUPPLIER_STATUSES,
} from "@/features/mall-consumption-orders/lib/url-state"
import type { FactType, SupplierFulfillmentStatus } from "@/features/mall-consumption-orders/types"
import {
    COST_BASIS_LABEL,
    DATA_SOURCE_LABEL,
    FACT_TYPE_LABEL,
    SUPPLIER_STATUS_LABEL,
} from "@/features/mall-consumption-orders/types"
import type { ReplaceParamsPatch } from "@/features/mall-consumption-orders/hooks/use-consumption-orders-url-state"

type Props = {
    searchInput: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    onSearchInputChange: (value: string) => void
    onCommitSearch: () => void
    searchPending: boolean
    filterSummary?: string
    mallId: string
    attributionStatus: string
    fulfillmentChain: string
    paymentSource: string
    costBasis: string
    occurredFrom: string
    occurredTo: string
    factTypes: FactType[]
    supplierStatuses: SupplierFulfillmentStatus[]
    dataSources: Array<"REALTIME" | "BACKFILL">
    hasActiveFilters: boolean
    onClearFilters: () => void
    onReplaceParams: (patch: ReplaceParamsPatch) => void
}

export function ConsumptionOrderFilterBar({
    searchInput,
    searchInputRef,
    onSearchInputChange,
    onCommitSearch,
    searchPending,
    filterSummary,
    mallId,
    attributionStatus,
    fulfillmentChain,
    paymentSource,
    costBasis,
    occurredFrom,
    occurredTo,
    factTypes,
    supplierStatuses,
    dataSources,
    hasActiveFilters,
    onClearFilters,
    onReplaceParams,
}: Props) {
    return (
        <>
            <ListToolbar
                search={
                    <InputGroup className="w-full">
                        <InputGroupAddon>
                            <SearchIcon className="size-4" />
                        </InputGroupAddon>
                        <InputGroupInput
                            ref={searchInputRef}
                            value={searchInput}
                            onChange={(e) => onSearchInputChange(e.target.value)}
                            onKeyDown={(e) => {
                                if (e.key === "Enter") onCommitSearch()
                            }}
                            placeholder="商城单号、客户、ERP 编号"
                            aria-label="搜索消费订单"
                        />
                    </InputGroup>
                }
                filters={
                    <>
                        <MallSearchCombobox
                            value={mallId === "all" ? null : mallId}
                            onValueChange={(v) =>
                                onReplaceParams({ mall: v || "all" })
                            }
                            className="w-44"
                            size="sm"
                            allowClear={false}
                            aria-label="来源商城"
                            placeholder="全部商城"
                        />
                        <DateRangePicker
                            value={
                                occurredFrom || occurredTo
                                    ? {
                                          from: occurredFrom || undefined,
                                          to: occurredTo || undefined,
                                      }
                                    : undefined
                            }
                            onValueChange={(range) =>
                                onReplaceParams({
                                    occurredFrom: range?.from || undefined,
                                    occurredTo: range?.to || undefined,
                                })
                            }
                            placeholder="记录发生时间"
                            className="w-56"
                        />
                        <OptionCombobox
                            value={attributionStatus}
                            onValueChange={(v) =>
                                onReplaceParams({
                                    attributionStatus: v || undefined,
                                })
                            }
                            options={[
                                {
                                    value: "all",
                                    label: "归集",
                                },
                                {
                                    value: "ATTRIBUTED",
                                    label: "已归集",
                                },
                                {
                                    value: "PENDING",
                                    label: "待归集",
                                },
                                {
                                    value: "DIFFERENCE",
                                    label: "差异",
                                },
                            ]}
                            className="w-32"
                            size="sm"
                            allowClear={false}
                            aria-label="归集状态"
                            placeholder="归集"
                        />
                    </>
                }
                secondary={
                    <>
                        <OptionCombobox
                            value={fulfillmentChain}
                            onValueChange={(v) =>
                                onReplaceParams({
                                    fulfillmentChain: v || undefined,
                                })
                            }
                            options={[
                                {
                                    value: "all",
                                    label: "履约链",
                                },
                                {
                                    value: "LEGACY_MANUAL",
                                    label: "原人工",
                                },
                                {
                                    value: "ERP_AUTOMATED",
                                    label: "ERP 自动",
                                },
                            ]}
                            className="w-36"
                            size="sm"
                            allowClear={false}
                            aria-label="履约链"
                            placeholder="履约链"
                        />
                        <MultiOptionCombobox
                            value={factTypes}
                            onValueChange={(v) =>
                                onReplaceParams({
                                    factType: v.length ? v.join(",") : undefined,
                                })
                            }
                            options={FACT_TYPES.map((t) => ({
                                value: t,
                                label: FACT_TYPE_LABEL[t],
                            }))}
                            className="w-40"
                            size="sm"
                            aria-label="事实类型"
                            placeholder="事实类型"
                        />
                        <MultiOptionCombobox
                            value={supplierStatuses}
                            onValueChange={(v) =>
                                onReplaceParams({
                                    supplierStatus: v.length
                                        ? v.join(",")
                                        : undefined,
                                })
                            }
                            options={SUPPLIER_STATUSES.map((s) => ({
                                value: s,
                                label: SUPPLIER_STATUS_LABEL[s],
                            }))}
                            className="w-40"
                            size="sm"
                            aria-label="供应商状态"
                            placeholder="供应商状态"
                        />
                        <MultiOptionCombobox
                            value={dataSources}
                            onValueChange={(v) =>
                                onReplaceParams({
                                    dataSource: v.length
                                        ? v.join(",")
                                        : undefined,
                                })
                            }
                            options={DATA_SOURCES.map((d) => ({
                                value: d,
                                label: DATA_SOURCE_LABEL[d],
                            }))}
                            className="w-32"
                            size="sm"
                            aria-label="数据来源"
                            placeholder="数据来源"
                        />
                        <OptionCombobox
                            value={paymentSource}
                            onValueChange={(v) =>
                                onReplaceParams({
                                    paymentSource: v || undefined,
                                })
                            }
                            options={[
                                {
                                    value: "all",
                                    label: "支付方式",
                                },
                                {
                                    value: "CARD",
                                    label: "卡券",
                                },
                                {
                                    value: "WECHAT",
                                    label: "微信",
                                },
                                {
                                    value: "MIXED",
                                    label: "组合",
                                },
                            ]}
                            className="w-32"
                            size="sm"
                            allowClear={false}
                            aria-label="支付方式"
                            placeholder="支付方式"
                        />
                        <OptionCombobox
                            value={costBasis}
                            onValueChange={(v) =>
                                onReplaceParams({
                                    costBasis: v || undefined,
                                })
                            }
                            options={[
                                {
                                    value: "all",
                                    label: "成本口径",
                                },
                                {
                                    value: "ACTUAL",
                                    label: COST_BASIS_LABEL.ACTUAL,
                                },
                                {
                                    value: "STANDARD",
                                    label: COST_BASIS_LABEL.STANDARD,
                                },
                                {
                                    value: "NONE",
                                    label: COST_BASIS_LABEL.NONE,
                                },
                            ]}
                            className="w-32"
                            size="sm"
                            allowClear={false}
                            aria-label="成本口径"
                            placeholder="成本口径"
                        />
                    </>
                }
                actions={
                    hasActiveFilters ? (
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : null
                }
            />

            {searchPending ? (
                <p className="text-xs text-muted-foreground" aria-live="polite">
                    搜索框内容尚未应用，稍候将自动生效；回车可立即搜索。
                </p>
            ) : null}

            {filterSummary ? (
                <p className="text-sm text-muted-foreground" aria-live="polite">
                    筛选摘要：{filterSummary}
                </p>
            ) : null}
        </>
    )
}
