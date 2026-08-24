"use client"

import Link from "next/link"

import {
    BusinessEmptyState,
    BusinessFailureState,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { FulfillmentOperationType } from "@/features/fulfillment-operations/types"
import {
    OPERATION_CLEARED_LABEL,
    OPERATION_TYPE_SHORT,
} from "@/features/fulfillment-operations/types"

export type FulfillmentPageStatesProps = {
    status: "pending" | "error" | "empty"
    headerDescription: string
    error?: unknown
    onRetry?: () => void
    completed?: boolean
    operationTypes?: FulfillmentOperationType[] | undefined
    emptyReason?: string
    roleLabel?: string
    visibleTypes?: readonly FulfillmentOperationType[]
    filterSummary?: string
    onClearAllFilters?: () => void
    /** 已由页面脚手架承载时跳过 PageScaffold / PageHeader，只渲染状态体 */
    standalone?: boolean
    /** 销售单详情内仅提供刷新，不跳去其它页面或解除销售单范围。 */
    embedded?: boolean
}

/**
 * 队列页的加载、失败与空态。查询失败与「没有符合条件的单据」必须分开：
 * 系统故障 ≠ 没活干。
 */
export function FulfillmentPageStates({
    status,
    headerDescription,
    error,
    onRetry,
    completed,
    operationTypes,
    emptyReason,
    roleLabel,
    visibleTypes,
    filterSummary,
    onClearAllFilters,
    standalone = false,
    embedded = false,
}: FulfillmentPageStatesProps) {
    if (status === "pending") {
        const body = (
            <>
                <div className="h-20 animate-pulse rounded-lg bg-muted" />
                <div className="grid gap-4 xl:grid-cols-[minmax(16rem,1fr)_minmax(0,2fr)]">
                    <div className="h-80 animate-pulse rounded-lg bg-muted" />
                    <div className="h-96 animate-pulse rounded-lg bg-muted" />
                </div>
            </>
        )
        if (standalone) return body
        return (
            <PageScaffold>
                <PageHeader
                    title={headerDescription}
                    description="正在加载队列…"
                />
                {body}
            </PageScaffold>
        )
    }

    if (status === "error") {
        const body = (
            <BusinessFailureState
                error={error}
                action={
                    <Button type="button" onClick={() => onRetry?.()}>
                        重新加载
                    </Button>
                }
            />
        )
        if (standalone) return body
        return (
            <PageScaffold>
                <PageHeader
                    title={headerDescription}
                    description="队列加载失败"
                />
                {body}
            </PageScaffold>
        )
    }

    if (completed) {
        return (
            <BusinessEmptyState
                kind="no-tasks"
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                title={
                    embedded
                        ? "本单当前没有待处理的履约单据"
                        : operationTypes?.length === 1
                          ? OPERATION_CLEARED_LABEL[operationTypes[0]]
                          : "这批活都干完了"
                }
                description={
                    embedded
                        ? "可刷新查看本单最新履约进度；已完成记录仍会保留在销售单摘要中。"
                        : "可以换个类型看看，或者清掉筛选、回工作台。"
                }
                action={
                    embedded ? (
                        <Button
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={onRetry}
                        >
                            刷新
                        </Button>
                    ) : (
                        <div className="flex flex-wrap gap-2">
                            <Button
                                type="button"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                onClick={onClearAllFilters}
                            >
                                清除全部筛选
                            </Button>
                            <Button render={<Link href="/workspace" />}>
                                回今日工作台
                            </Button>
                        </div>
                    )
                }
            />
        )
    }

    if (emptyReason === "NO_PERMISSION") {
        return (
            <BusinessEmptyState
                kind="no-scope"
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                title="你没有这类单据的权限"
                description={`${roleLabel}能处理的是：${(visibleTypes ?? [])
                    .map((t) => OPERATION_TYPE_SHORT[t])
                    .join("、")}。`}
                action={
                    <Button
                        type="button"
                        variant="secondary"
                        className="rounded-lg shadow-none"
                        onClick={embedded ? onRetry : onClearAllFilters}
                    >
                        {embedded ? "重新加载" : "回到我能处理的"}
                    </Button>
                }
            />
        )
    }

    return (
        <BusinessEmptyState
            kind="filter"
            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
            title={
                embedded ? "本单当前没有待处理的履约单据" : "没有符合条件的单据"
            }
            description={
                embedded
                    ? "可刷新查看本单最新履约进度。"
                    : (filterSummary ?? "换个类型或单号试试")
            }
            action={
                <Button
                    type="button"
                    variant="secondary"
                    className="rounded-lg shadow-none"
                    onClick={embedded ? onRetry : onClearAllFilters}
                >
                    {embedded ? "刷新" : "清除全部筛选"}
                </Button>
            }
        />
    )
}
