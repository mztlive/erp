"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
import { DatePicker } from "@/components/ui/date-picker"
import { Label } from "@/components/ui/label"
import type { CustomerQualityPeriodPolicy } from "../types"
import type { CustomerQualityPatch } from "../hooks/use-customer-quality-navigation-state"

const SORT_OPTIONS: ReadonlyArray<{ value: string; label: string }> = [
    { value: "salesGrossAmount:desc", label: "成交金额降序" },
    { value: "actualProfitLossNet:desc", label: "实际盈亏降序" },
    { value: "overdueGross:desc", label: "逾期金额降序" },
    { value: "costCoverageRate:asc", label: "覆盖率升序" },
    { value: "latestBusinessAt:desc", label: "最近业务" },
]

/**
 * 分析维度与视图控件条（docs/ui-filter-design.md §2.3）：
 * 统计期间与排序会改变整张分析报表的含义，属于分析维度/视图参数，
 * 位于明细表面之外；不属于筛选表单，也不被「清空全部」清除。
 */
export function CustomerQualityPeriodBar({
    resolvedFrom,
    resolvedTo,
    periodInvalid,
    presets,
    periodPreset,
    sort,
    onPresetSelect,
    patchUrl,
    resetPage,
}: {
    resolvedFrom?: string
    resolvedTo?: string
    periodInvalid: boolean
    presets?: CustomerQualityPeriodPolicy["presets"]
    periodPreset?: string
    sort: string
    onPresetSelect: (id: string) => void
    patchUrl: CustomerQualityPatch
    resetPage: () => void
}) {
    return (
        <div
            data-slot="customer-quality-period-bar"
            aria-label="统计期间与排序"
            className="flex flex-wrap items-end gap-3"
        >
            <div className="space-y-1.5">
                <Label htmlFor="customers-quality-period-from">期间起</Label>
                <DatePicker
                    id="customers-quality-period-from"
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
                <Label htmlFor="customers-quality-period-to">期间止</Label>
                <DatePicker
                    id="customers-quality-period-to"
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
                    <Label htmlFor="customers-quality-preset">快捷期间</Label>
                    <OptionCombobox
                        id="customers-quality-preset"
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
                        allowClear={false}
                        aria-label="快捷期间"
                        placeholder="自定义"
                    />
                </div>
            ) : null}
            <div className="space-y-1.5">
                <Label htmlFor="customers-quality-sort">排序</Label>
                <OptionCombobox
                    id="customers-quality-sort"
                    value={sort}
                    onValueChange={(v) => {
                        patchUrl({ sort: v ?? sort })
                        resetPage()
                    }}
                    options={SORT_OPTIONS}
                    className="w-44"
                    allowClear={false}
                    aria-label="排序"
                    placeholder="排序"
                />
            </div>
        </div>
    )
}
