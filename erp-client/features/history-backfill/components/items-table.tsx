"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import { ExternalLinkIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type {
    BackfillPipelineStage,
    HistoryBackfillItemView,
    JobSection,
} from "@/features/history-backfill/types"
import {
    COST_BASIS_LABEL,
    FACT_TYPE_LABEL,
    FAILURE_CODE_LABEL,
    ITEM_RESULT_LABEL,
    ITEM_RESULT_TONE,
    PIPELINE_STAGE_LABEL,
} from "@/features/history-backfill/types"
import { formatDateTime } from "@/lib/datetime"

export function ItemsTable({
    items,
    section,
    loading,
    totalCount,
    page,
    onPageChange,
    onReattribute,
    title,
}: {
    items: HistoryBackfillItemView[]
    section: JobSection
    loading?: boolean
    totalCount: number
    page: number
    onPageChange: (page: number) => void
    onReattribute?: (itemId: string) => void
    title?: string
}) {
    const pageSize = 20
    const columns = React.useMemo<ColumnDef<HistoryBackfillItemView>[]>(
        () => [
            {
                id: "factType",
                header: "记录类型",
                cell: ({ row }) => (
                    <span className="text-sm">
                        {FACT_TYPE_LABEL[row.original.factType]}
                    </span>
                ),
            },
            {
                id: "key",
                header: "记录摘要",
                cell: ({ row }) => (
                    <span className="font-mono text-xs">
                        {row.original.businessFactKeySummary}
                    </span>
                ),
            },
            {
                id: "order",
                header: "商城订单",
                cell: ({ row }) => (
                    <div className="space-y-0.5">
                        <div className="font-mono text-xs">
                            {row.original.mallOrderNo}
                        </div>
                        {row.original.sourceDocNo ? (
                            <div className="text-tiny text-muted-foreground">
                                子单 {row.original.sourceDocNo}
                            </div>
                        ) : null}
                    </div>
                ),
            },
            {
                id: "occurred",
                header: "发生时间",
                cell: ({ row }) => (
                    <span className="num text-xs">
                        {formatDateTime(row.original.occurredAt, "dateStyle")}
                    </span>
                ),
            },
            {
                id: "result",
                header: "结果",
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={ITEM_RESULT_LABEL[row.original.result]}
                        tone={ITEM_RESULT_TONE[row.original.result]}
                    />
                ),
            },
            {
                id: "cost",
                header: "成本",
                cell: ({ row }) => {
                    const b = row.original.costBasis
                    if (!b || b === "N_A")
                        return <span className="text-xs">不适用</span>
                    return (
                        <span className="text-xs">
                            {COST_BASIS_LABEL[b]}
                            {b === "NONE"
                                ? " · 成本空"
                                : row.original.costAmountNet
                                  ? ` · ${row.original.costAmountNet}`
                                  : ""}
                        </span>
                    )
                },
            },
            {
                id: "extra",
                header: section === "dedupe" ? "去重证明" : "说明 / 去向",
                cell: ({ row }) => {
                    const item = row.original
                    if (item.dedupeProof) {
                        return (
                            <div className="max-w-[16rem] text-xs">
                                <div>
                                    {item.dedupeProof.matchedSource ===
                                    "REALTIME"
                                        ? "命中实时记录"
                                        : "命中原回填记录"}
                                </div>
                                <div className="text-muted-foreground">
                                    {item.dedupeProof.formalFactSummary}
                                </div>
                            </div>
                        )
                    }
                    if (item.result === "UNATTRIBUTED") {
                        return (
                            <div className="space-y-1">
                                <div className="text-xs">
                                    {item.unattributedReason}
                                </div>
                                <div className="flex flex-wrap gap-1">
                                    <Button
                                        render={
                                            <Link href="/governance/integration-errors?view=mine" />
                                        }
                                        size="sm"
                                        variant="outline"
                                        className="h-7 text-xs"
                                    >
                                        去接口错误中心处理
                                        <ExternalLinkIcon className="size-3" />
                                    </Button>
                                    {onReattribute ? (
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            className="h-7 text-xs"
                                            onClick={() =>
                                                onReattribute(item.itemId)
                                            }
                                        >
                                            重新归集
                                        </Button>
                                    ) : null}
                                </div>
                            </div>
                        )
                    }
                    if (item.failure) {
                        return (
                            <div className="max-w-[14rem] text-xs">
                                <div>
                                    {FAILURE_CODE_LABEL[
                                        item.failure.errorCode
                                    ] ?? item.failure.summary}
                                </div>
                                <div>{item.failure.summary}</div>
                                <div className="text-muted-foreground">
                                    {PIPELINE_STAGE_LABEL[
                                        item.failure
                                            .stage as BackfillPipelineStage
                                    ] ?? item.failure.stage}{" "}
                                    ·{" "}
                                    {item.failure.retryable
                                        ? "可续跑"
                                        : "需业务修复"}
                                </div>
                            </div>
                        )
                    }
                    return (
                        <span className="text-xs text-muted-foreground">
                            {item.fulfillmentChain === "LEGACY_MANUAL"
                                ? "历史手工口径"
                                : "—"}
                        </span>
                    )
                },
            },
        ],
        [section, onReattribute],
    )

    if (items.length === 0) {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="当前筛选无明细"
                description="同一商城订单的多笔关键记录分别保留；支付/取消/完成/多次退款/多次余额恢复不会被合并。"
            />
        )
    }

    return (
        <BusinessTableFrame
            title={
                title ??
                (section === "dedupe"
                    ? "去重证明"
                    : section === "unattributed"
                      ? "待归集（原记录已保存）"
                      : section === "failures"
                        ? "失败诊断"
                        : "记录结果")
            }
            description="不含卡号/卡密/手机/完整地址/原始消息内容"
            table={
                <DataTable
                    data={[...items]}
                    columns={columns}
                    getRowId={(row) => row.itemId}
                    rowCount={totalCount}
                    pagination={{ pageIndex: Math.max(0, page - 1), pageSize }}
                    onPaginationChange={(next) =>
                        onPageChange(next.pageIndex + 1)
                    }
                    layout="flush"
                    density="compact"
                    loading={loading}
                    showRefreshingBanner={false}
                />
            }
        />
    )
}
