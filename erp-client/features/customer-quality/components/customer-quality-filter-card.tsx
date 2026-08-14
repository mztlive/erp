"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import { OptionCombobox, surfacePanelClassName } from "@/components/business"
import { FilterChip } from "@/components/business/filter-chip"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { DatePicker } from "@/components/ui/date-picker"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import type {
    BusinessTypeFilter,
    CustomerQualityPeriodPolicy,
    FundsReviewFilter,
} from "../types"
import type { CustomerQualityPatch } from "../hooks/use-customer-quality-navigation-state"

export function CustomerQualityFilterCard({
    resolvedFrom,
    resolvedTo,
    periodInvalid,
    presets,
    periodPreset,
    fundsReview,
    businessType,
    sort,
    searchInput,
    searchInputRef,
    onSearchInputChange,
    customerId,
    chipCustomerName,
    showClearFilters,
    onClearFilters,
    onPresetSelect,
    patchUrl,
    resetPage,
    filterSummary,
    filteredTotal,
    total,
}: {
    resolvedFrom?: string
    resolvedTo?: string
    periodInvalid: boolean
    presets?: CustomerQualityPeriodPolicy["presets"]
    periodPreset?: string
    fundsReview: FundsReviewFilter
    businessType?: BusinessTypeFilter
    sort: string
    searchInput: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    onSearchInputChange: (value: string) => void
    customerId?: string
    chipCustomerName?: string
    showClearFilters: boolean
    onClearFilters: () => void
    onPresetSelect: (id: string) => void
    patchUrl: CustomerQualityPatch
    resetPage: () => void
    filterSummary: string
    filteredTotal: number
    total: number
}) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardContent className="flex flex-col gap-3 pt-4">
                <div className="flex flex-wrap items-end gap-3">
                    <div className="space-y-1.5">
                        <Label htmlFor="cq-period-from">期间起</Label>
                        <DatePicker
                            className="w-[10.5rem]"
                            value={resolvedFrom || undefined}
                            onValueChange={(next) => {
                                patchUrl({
                                    from: next || null,
                                    periodSelectionSource: "EXPLICIT",
                                    periodPreset: null,
                                })
                                resetPage()
                            }}
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="cq-period-to">期间止</Label>
                        <DatePicker
                            className="w-[10.5rem]"
                            value={resolvedTo || undefined}
                            onValueChange={(next) => {
                                patchUrl({
                                    to: next || null,
                                    periodSelectionSource: "EXPLICIT",
                                    periodPreset: null,
                                })
                                resetPage()
                            }}
                        />
                    </div>
                    {periodInvalid ? (
                        <p
                            id="cq-period-invalid"
                            className="w-full text-xs text-destructive"
                            role="alert"
                        >
                            开始日期晚于结束日期，将查询不到结果，请调整。
                        </p>
                    ) : null}
                    {presets?.length ? (
                        <div className="space-y-1.5">
                            <Label htmlFor="cq-preset">快捷期间</Label>
                            <OptionCombobox
                                id="cq-preset"
                                value={periodPreset ?? ""}
                                onValueChange={(v) => {
                                    onPresetSelect(v ?? "")
                                }}
                                options={[
                                    { value: "", label: "自定义" },
                                    ...presets.map((p) => ({
                                        value: p.id,
                                        label: p.label,
                                    })),
                                ]}
                                className="w-40"
                                size="sm"
                                allowClear={false}
                                aria-label="快捷期间"
                                placeholder="自定义"
                            />
                        </div>
                    ) : null}
                    <div className="space-y-1.5">
                        <Label htmlFor="cq-funds">票款口径</Label>
                        <OptionCombobox
                            id="cq-funds"
                            value={fundsReview}
                            onValueChange={(v) => {
                                patchUrl({
                                    fundsReview:
                                        (v ?? "all") === "reviewed_only"
                                            ? "reviewed_only"
                                            : null,
                                })
                                resetPage()
                            }}
                            options={[
                                { value: "all", label: "全部授权记录" },
                                {
                                    value: "reviewed_only",
                                    label: "仅已复核卡券票款",
                                },
                            ]}
                            className="w-44"
                            size="sm"
                            allowClear={false}
                            aria-label="票款口径"
                            placeholder="票款口径"
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="cq-nature">业务性质</Label>
                        <OptionCombobox
                            id="cq-nature"
                            value={businessType ?? ""}
                            onValueChange={(v) => {
                                patchUrl({
                                    businessType: v || null,
                                })
                                resetPage()
                            }}
                            options={[
                                { value: "", label: "全部" },
                                { value: "VOUCHER", label: "卡券" },
                                { value: "GOODS_SERVICE", label: "非卡券" },
                            ]}
                            className="w-36"
                            size="sm"
                            allowClear={false}
                            aria-label="业务性质"
                            placeholder="全部"
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="cq-sort">排序</Label>
                        <OptionCombobox
                            id="cq-sort"
                            value={sort}
                            onValueChange={(v) => {
                                patchUrl({ sort: v ?? sort })
                                resetPage()
                            }}
                            options={[
                                {
                                    value: "salesGrossAmount:desc",
                                    label: "成交金额降序",
                                },
                                {
                                    value: "actualProfitLossNet:desc",
                                    label: "实际盈亏降序",
                                },
                                {
                                    value: "overdueGross:desc",
                                    label: "逾期金额降序",
                                },
                                {
                                    value: "costCoverageRate:asc",
                                    label: "覆盖率升序",
                                },
                                {
                                    value: "latestBusinessAt:desc",
                                    label: "最近业务",
                                },
                            ]}
                            className="w-44"
                            size="sm"
                            allowClear={false}
                            aria-label="排序"
                            placeholder="排序"
                        />
                    </div>
                    <div className="min-w-[12rem] flex-1 space-y-1.5">
                        <Label htmlFor="cq-q">搜索客户</Label>
                        <InputGroup>
                            <InputGroupAddon>
                                <SearchIcon aria-hidden="true" />
                            </InputGroupAddon>
                            <InputGroupInput
                                id="cq-q"
                                ref={searchInputRef}
                                value={searchInput}
                                placeholder="客户编号 / 名称（/ 聚焦）"
                                onChange={(e) =>
                                    onSearchInputChange(e.target.value)
                                }
                                onKeyDown={(e) => {
                                    if (e.key === "Enter") {
                                        patchUrl({
                                            q: searchInput.trim() || null,
                                        })
                                    }
                                }}
                            />
                        </InputGroup>
                    </div>
                    {customerId ? (
                        <FilterChip
                            label={`客户：${chipCustomerName ?? "已定位客户"}`}
                            onClear={() => patchUrl({ customerId: null })}
                        />
                    ) : null}
                    {showClearFilters ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : null}
                </div>
                <p
                    className="text-xs text-muted-foreground"
                    aria-live="polite"
                >
                    当前口径：{filterSummary} · 明细 {filteredTotal}/{total} 户
                </p>
            </CardContent>
        </Card>
    )
}
