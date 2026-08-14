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
}: FulfillmentPageStatesProps) {
    if (status === "pending") {
        return (
            <PageScaffold>
                <PageHeader
                    title={headerDescription}
                    description="正在加载队列…"
                />
                <div className="h-20 animate-pulse rounded-lg bg-muted" />
                <div className="grid gap-4 xl:grid-cols-[minmax(16rem,1fr)_minmax(0,2fr)]">
                    <div className="h-80 animate-pulse rounded-lg bg-muted" />
                    <div className="h-96 animate-pulse rounded-lg bg-muted" />
                </div>
            </PageScaffold>
        )
    }

    if (status === "error") {
        return (
            <PageScaffold>
                <PageHeader
                    title={headerDescription}
                    description="队列加载失败"
                />
                <BusinessFailureState
                    error={error}
                    action={
                        <Button type="button" onClick={() => onRetry?.()}>
                            重新加载
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (completed) {
        return (
            <BusinessEmptyState
                kind="no-tasks"
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                title={
                    operationTypes?.length === 1
                        ? OPERATION_CLEARED_LABEL[operationTypes[0]]
                        : "这批活都干完了"
                }
                description="可以换个类型看看，或者清掉筛选、回工作台。"
                action={
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
                        onClick={onClearAllFilters}
                    >
                        回到我能处理的
                    </Button>
                }
            />
        )
    }

    return (
        <BusinessEmptyState
            kind="filter"
            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
            title="没有符合条件的单据"
            description={filterSummary ?? "换个类型或单号试试"}
            action={
                <Button
                    type="button"
                    variant="secondary"
                    className="rounded-lg shadow-none"
                    onClick={onClearAllFilters}
                >
                    清除全部筛选
                </Button>
            }
        />
    )
}
