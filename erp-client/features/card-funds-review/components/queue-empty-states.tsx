"use client"

import Link from "next/link"

import { BusinessEmptyState } from "@/components/business"
import { Button } from "@/components/ui/button"

/** 队列已清空的空态（可清除筛选 / 返回工作台）。 */
export function CompletedQueueEmptyState({
    hasActiveQueueFilters,
    onClearFilters,
}: {
    hasActiveQueueFilters: boolean
    onClearFilters: () => void
}) {
    return (
        <BusinessEmptyState
            kind="no-tasks"
            title="当前筛选项已处理完"
            description="卡券票款复核有效队列已清空。可清除筛选、切换类型/跳过范围，或返回工作台。"
            action={
                <div className="flex flex-wrap gap-2">
                    {hasActiveQueueFilters ? (
                        <Button
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : null}
                    <Button
                        variant="secondary"
                        className="rounded-lg shadow-none"
                        render={<Link href="/workspace" />}
                    >
                        返回今日工作台
                    </Button>
                </div>
            }
        />
    )
}

/** 筛选无结果的空态。 */
export function FilterQueueEmptyState({
    onClearFilters,
}: {
    onClearFilters: () => void
}) {
    return (
        <BusinessEmptyState
            kind="filter"
            title="筛选无结果"
            description="当前类型/范围没有任务，可清除筛选。"
            action={
                <Button
                    type="button"
                    variant="secondary"
                    className="rounded-lg shadow-none"
                    onClick={onClearFilters}
                >
                    清除筛选
                </Button>
            }
        />
    )
}
