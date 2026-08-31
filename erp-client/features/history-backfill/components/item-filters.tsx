"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import type { HistoryBackfillUrlState } from "@/features/history-backfill/lib/url-state"
import type {
    CostBasis,
    ItemResult,
    JobSection,
    MallOrderFactType,
} from "@/features/history-backfill/types"
import {
    COST_BASIS_LABEL,
    FACT_TYPE_LABEL,
    ITEM_RESULT_LABEL,
} from "@/features/history-backfill/types"

export function ItemFilters({
    urlState,
    patchUrl,
    section,
}: {
    urlState: HistoryBackfillUrlState
    patchUrl: (patch: Partial<HistoryBackfillUrlState>) => void
    section: JobSection
}) {
    const [qDraft, setQDraft] = React.useState(urlState.q ?? "")
    return (
        <div className="flex flex-wrap items-end gap-2 rounded-lg bg-muted/40 p-3">
            {section === "facts" ? (
                <>
                    <div className="space-y-1">
                        <Label className="text-xs">结果</Label>
                        <OptionCombobox
                            id="operations-history-backfill-detail-filter-result"
                            value={urlState.result ?? "all"}
                            onValueChange={(v) => {
                                if (v == null) return
                                patchUrl({
                                    result:
                                        v === "all"
                                            ? undefined
                                            : (v as ItemResult),
                                    page: 1,
                                })
                            }}
                            options={[
                                { value: "all", label: "全部结果" },
                                ...(
                                    Object.keys(
                                        ITEM_RESULT_LABEL,
                                    ) as ItemResult[]
                                ).map((r) => ({
                                    value: r,
                                    label: ITEM_RESULT_LABEL[r],
                                })),
                            ]}
                            className="w-[10rem]"
                            size="sm"
                            allowClear={false}
                        />
                    </div>
                    <div className="space-y-1">
                        <Label className="text-xs">记录类型</Label>
                        <OptionCombobox
                            id="operations-history-backfill-detail-filter-fact-type"
                            value={urlState.factType ?? "all"}
                            onValueChange={(v) => {
                                if (v == null) return
                                patchUrl({
                                    factType:
                                        v === "all"
                                            ? undefined
                                            : (v as MallOrderFactType),
                                    page: 1,
                                })
                            }}
                            options={[
                                { value: "all", label: "全部五类" },
                                ...(
                                    Object.keys(
                                        FACT_TYPE_LABEL,
                                    ) as MallOrderFactType[]
                                ).map((t) => ({
                                    value: t,
                                    label: FACT_TYPE_LABEL[t],
                                })),
                            ]}
                            className="w-[12rem]"
                            size="sm"
                            allowClear={false}
                        />
                    </div>
                    <div className="space-y-1">
                        <Label className="text-xs">成本口径</Label>
                        <OptionCombobox
                            id="operations-history-backfill-detail-filter-cost-basis"
                            value={urlState.costBasis ?? "all"}
                            onValueChange={(v) => {
                                if (v == null) return
                                patchUrl({
                                    costBasis:
                                        v === "all"
                                            ? undefined
                                            : (v as CostBasis),
                                    page: 1,
                                })
                            }}
                            options={[
                                { value: "all", label: "全部" },
                                ...(
                                    Object.keys(COST_BASIS_LABEL) as CostBasis[]
                                ).map((b) => ({
                                    value: b,
                                    label: COST_BASIS_LABEL[b],
                                })),
                            ]}
                            className="w-[9rem]"
                            size="sm"
                            allowClear={false}
                        />
                    </div>
                </>
            ) : null}
            <div className="space-y-1">
                <Label className="text-xs">搜索</Label>
                <form
                    className="flex gap-1"
                    onSubmit={(e) => {
                        e.preventDefault()
                        patchUrl({ q: qDraft.trim() || undefined, page: 1 })
                    }}
                >
                    <Input
                        id="operations-history-backfill-detail-filter-search"
                        className="h-8 w-[12rem]"
                        value={qDraft}
                        onChange={(e) => setQDraft(e.target.value)}
                        placeholder="商城订单号 / 子单号"
                    />
                    {urlState.q ? (
                        <Button
                            id="operations-history-backfill-detail-filter-clear"
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={() => {
                                setQDraft("")
                                patchUrl({ q: undefined, page: 1 })
                            }}
                        >
                            清除
                        </Button>
                    ) : null}
                    <Button
                        id="operations-history-backfill-detail-filter-search-submit"
                        type="submit"
                        size="sm"
                        variant="secondary"
                    >
                        搜索
                    </Button>
                </form>
            </div>
            <p className="w-full text-xs text-muted-foreground">
                同一商城订单的多笔关键记录分别保留，多次退款/恢复不合并
            </p>
        </div>
    )
}
