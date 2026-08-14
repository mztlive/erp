"use client"

import Link from "next/link"

import {
    BusinessEmptyState,
    BusinessFailureState,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ProcurementQueueView } from "@/features/procurement-confirmation/types"

/** 队列加载中的骨架屏。 */
export function ProcurementPagePending() {
    return (
        <PageScaffold>
            <PageHeader title="采购二次确认" description="正在加载队列…" />
            <div
                className="h-12 animate-pulse rounded-lg bg-muted"
                aria-hidden="true"
            />
            <div className="grid gap-3 md:gap-4 xl:grid-cols-[minmax(0,2fr)_minmax(16rem,1fr)]">
                <div className="h-80 animate-pulse rounded-lg bg-muted" />
                <div className="h-64 animate-pulse rounded-lg bg-muted" />
            </div>
        </PageScaffold>
    )
}

export type ProcurementPageErrorProps = {
    error: unknown
    onRetry: () => void
}

/** 队列加载失败。 */
export function ProcurementPageError({
    error,
    onRetry,
}: ProcurementPageErrorProps) {
    return (
        <PageScaffold>
            <PageHeader title="采购二次确认" description="队列加载失败" />
            <BusinessFailureState
                title="队列加载失败"
                error={error}
                onRetry={onRetry}
                action={
                    <Button
                        variant="outline"
                        size="sm"
                        render={<Link href="/workspace" />}
                    >
                        返回今日工作台
                    </Button>
                }
            />
        </PageScaffold>
    )
}

export type ProcurementEmptyStatesProps = {
    emptyReason: ProcurementQueueView["emptyReason"]
    onClearFilters: () => void
}

/** 队列清空后的三种空态（筛选无结果 / 无数据范围 / 已处理完）。 */
export function ProcurementEmptyStates({
    emptyReason,
    onClearFilters,
}: ProcurementEmptyStatesProps) {
    if (emptyReason === "FILTER_NO_RESULT") {
        return (
            <BusinessEmptyState
                kind="filter"
                title="当前筛选无结果"
                description="没有单号或范围匹配的待确认事项，可清除筛选后重试。"
                action={
                    <div className="flex flex-wrap gap-2">
                        <Button variant="outline" onClick={onClearFilters}>
                            清除筛选
                        </Button>
                        <Button render={<Link href="/workspace" />}>
                            返回今日工作台
                        </Button>
                    </div>
                }
            />
        )
    }
    if (emptyReason === "NO_DATA_SCOPE") {
        return (
            <BusinessEmptyState
                kind="no-scope"
                title="当前角色无数据范围"
                description="你可以进入此页面，但当前角色范围内没有可查看的待确认事项。"
                action={
                    <Button render={<Link href="/workspace" />}>
                        返回今日工作台
                    </Button>
                }
            />
        )
    }
    return (
        <BusinessEmptyState
            kind="no-tasks"
            title="本筛选项已处理完"
            description="当前采购二次确认队列已经清空，可以返回工作台处理其它事项。"
            action={
                <Button render={<Link href="/workspace" />}>
                    返回今日工作台
                </Button>
            }
        />
    )
}
