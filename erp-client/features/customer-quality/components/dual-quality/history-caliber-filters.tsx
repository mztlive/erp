"use client"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { OptionCombobox } from "@/components/business/option-combobox"
import { BusinessFailureState } from "@/components/business"
import type { useHistoricalDirectory } from "@/lib/historical-directory"
import { serializeCsvIds } from "../../lib/dual-url-state"
import { toggleId } from "../../lib/dual-filter-state"
import { CandidatePicker } from "./candidate-picker"
import type { useHistoryQualityFilters } from "../../hooks/use-history-quality-filters"
import type { PatchDual } from "../../lib/dual-filter-state"

const HISTORY_SORT_OPTIONS = [
    { value: "orderCount:desc", label: "订单数降序" },
    { value: "orderCount:asc", label: "订单数升序" },
    { value: "grossTotal:desc", label: "含税总额降序" },
    { value: "grossTotal:asc", label: "含税总额升序" },
    { value: "label:asc", label: "分组名称升序" },
]

export function HistoryCaliberFilters({
    filters,
    patchDual,
    directoryQuery,
}: {
    filters: ReturnType<typeof useHistoryQualityFilters>
    patchDual: PatchDual
    directoryQuery: ReturnType<typeof useHistoricalDirectory>
}) {
    const {
        userIds,
        orgIds,
        attributionGroup,
        dimension,
        sort,
        qDraft,
        setQDraft,
        idDraft,
        setIdDraft,
        hasFilters,
        applySearch,
        applyIdDraft,
    } = filters
    const directory =
        directoryQuery.isError || directoryQuery.isFetching
            ? undefined
            : directoryQuery.data
    return (
        <>
            <div className="grid min-w-0 grid-cols-1 gap-2 sm:grid-cols-2">
                <div className="flex min-w-0 gap-2">
                    <label
                        htmlFor="customers-quality-dual-search"
                        className="sr-only"
                    >
                        搜索客户或单号
                    </label>
                    <Input
                        id="customers-quality-dual-search"
                        type="search"
                        value={qDraft}
                        onChange={(e) => setQDraft(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                e.preventDefault()
                                applySearch()
                            }
                        }}
                        placeholder="客户名称 / 单号"
                        className="min-w-0 flex-1"
                    />
                    <Button
                        id="customers-quality-dual-apply"
                        type="button"
                        size="sm"
                        onClick={applySearch}
                    >
                        查询
                    </Button>
                </div>
                <div className="flex min-w-0 gap-2">
                    <label
                        htmlFor="customers-quality-dual-attribution-id"
                        className="sr-only"
                    >
                        历史归属销售 ID
                    </label>
                    <Input
                        id="customers-quality-dual-attribution-id"
                        value={idDraft}
                        onChange={(e) => setIdDraft(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                e.preventDefault()
                                applyIdDraft()
                            }
                        }}
                        placeholder="历史归属销售 ID（逗号分隔）"
                        className="min-w-0 flex-1 font-mono"
                    />
                    <Button
                        id="customers-quality-dual-attribution-add"
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={applyIdDraft}
                    >
                        添加
                    </Button>
                </div>
            </div>

            <div className="flex min-w-0 flex-wrap items-center gap-2 text-body-compact">
                <label
                    htmlFor="customers-quality-dual-dimension"
                    className="text-muted-foreground"
                >
                    分组
                </label>
                <OptionCombobox
                    id="customers-quality-dual-dimension"
                    value={dimension}
                    aria-label="分组"
                    allowClear={false}
                    onValueChange={(value) => {
                        if (value === null) return
                        patchDual({
                            dualDimension:
                                value === "attribution_user" ? null : value,
                            attributionGroup: null,
                            scopeVersion: null,
                            dualPage: null,
                        })
                    }}
                    options={[
                        { value: "attribution_user", label: "按历史归属销售" },
                        { value: "attribution_org", label: "按历史归属组织" },
                    ]}
                    className="w-auto min-w-40"
                />
                <label
                    htmlFor="customers-quality-dual-sort"
                    className="text-muted-foreground"
                >
                    排序
                </label>
                <OptionCombobox
                    id="customers-quality-dual-sort"
                    value={sort}
                    aria-label="排序"
                    allowClear={false}
                    onValueChange={(value) => {
                        if (value === null) return
                        patchDual({
                            dualSort:
                                value === "orderCount:desc" ? null : value,
                            scopeVersion: null,
                            dualPage: null,
                        })
                    }}
                    options={HISTORY_SORT_OPTIONS}
                    className="w-auto min-w-40"
                />
                {hasFilters ? (
                    <Button
                        id="customers-quality-dual-clear"
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() =>
                            patchDual({
                                attributionUserIds: null,
                                attributionOrgUnitIds: null,
                                attributionGroup: null,
                                dualCustomerId: null,
                                dualQ: null,
                                scopeVersion: null,
                                dualPage: null,
                            })
                        }
                    >
                        清除筛选
                    </Button>
                ) : null}
            </div>

            {attributionGroup ? (
                <div className="flex min-w-0 flex-wrap items-center gap-2 rounded-xl border border-border p-2 text-body-compact">
                    <span className="min-w-0 truncate">
                        历史分组下钻：{attributionGroup}
                    </span>
                    <Button
                        id="customers-quality-dual-drill-clear"
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() =>
                            patchDual({
                                attributionGroup: null,
                                scopeVersion: null,
                                dualPage: null,
                            })
                        }
                    >
                        清除下钻
                    </Button>
                </div>
            ) : null}

            {directoryQuery.isError && (
                <BusinessFailureState
                    title="历史候选加载失败"
                    error={directoryQuery.error}
                    onRetry={() => void directoryQuery.refetch()}
                />
            )}
            <div className="grid min-w-0 grid-cols-1 gap-2 sm:grid-cols-2">
                <CandidatePicker
                    idPrefix="customers-quality-dual-attribution-user"
                    title="历史归属销售候选"
                    options={directory?.attributionUserOptions ?? []}
                    selected={userIds}
                    emptyLabel={
                        directoryQuery.isFetching
                            ? "正在加载历史候选…"
                            : "该期间授权范围内无历史销售候选。"
                    }
                    onToggle={(id) => {
                        const merged = serializeCsvIds(toggleId(userIds, id))
                        patchDual({
                            attributionUserIds: merged || null,
                            scopeVersion: null,
                            dualPage: null,
                        })
                    }}
                />
                <CandidatePicker
                    idPrefix="customers-quality-dual-attribution-org"
                    title="历史归属组织候选"
                    options={directory?.attributionOrgOptions ?? []}
                    selected={orgIds}
                    emptyLabel={
                        directoryQuery.isFetching
                            ? "正在加载历史候选…"
                            : "该期间授权范围内无历史组织候选。"
                    }
                    onToggle={(id) => {
                        const merged = serializeCsvIds(toggleId(orgIds, id))
                        patchDual({
                            attributionOrgUnitIds: merged || null,
                            scopeVersion: null,
                            dualPage: null,
                        })
                    }}
                />
            </div>
        </>
    )
}
