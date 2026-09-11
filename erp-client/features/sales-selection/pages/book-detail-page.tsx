/**
 * 选品册详情页：对象页头 + 流程向导 + 双栏作业工作台（左侧陈列预览画廊 + 右侧档案指标侧边栏）。
 * 发布固定已确认批次，不回传浏览器明细替代后端快照。
 */

"use client"

import * as React from "react"
import Link from "next/link"

import {
    BusinessFailureState,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { BookDetailHeader } from "@/features/sales-selection/components/book-detail-header"
import { BookDetailSidebar } from "@/features/sales-selection/components/book-detail-sidebar"
import { BookWorkflowBanner } from "@/features/sales-selection/components/book-workflow-banner"
import { PreviewGrid } from "@/features/sales-selection/components/preview-grid"
import { ReprepareDialog } from "@/features/sales-selection/components/reprepare-dialog"
import {
    useBookDetail,
    useBookOperations,
} from "@/features/sales-selection/hooks/queries"
import { bookIdentity } from "@/features/sales-selection/lib/presentation"
import { createIdempotencyKey } from "@/features/sales-selection/lib/validation"
import { cn } from "@/lib/utils"

/**
 * 选品册详情页。
 * @param bookId 选品册身份
 */
export const BookDetailPage = ({ bookId }: { bookId: string }) => {
    const detailQuery = useBookDetail(bookId)
    const operations = useBookOperations()
    const detail = detailQuery.data
    const [editing, setEditing] = React.useState(false)

    const pending =
        operations.prepare.isPending ||
        operations.regenerate.isPending ||
        operations.reprepare.isPending ||
        operations.publish.isPending ||
        operations.replaceLink.isPending ||
        operations.close.isPending ||
        operations.revoke.isPending ||
        operations.void.isPending ||
        operations.deleteItem.isPending

    if (detailQuery.isPending) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="选品册" description="正在加载选品册…" />
                <div className="space-y-3" aria-busy="true" aria-label="加载中">
                    <Skeleton className="h-16 w-full rounded-lg" />
                    <Skeleton className="h-20 w-full rounded-xl" />
                    <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_340px]">
                        <Skeleton className="h-96 w-full rounded-xl" />
                        <Skeleton className="h-96 w-full rounded-xl" />
                    </div>
                </div>
            </PageScaffold>
        )
    }

    if (detailQuery.isError || !detail) {
        return (
            <PageScaffold density="compact">
                <PageHeader
                    title="选品册"
                    actions={
                        <Button
                            id="sales-selection-detail-back-error"
                            variant="outline"
                            size="sm"
                            render={<Link href="/sales/selection" />}
                        >
                            返回列表
                        </Button>
                    }
                />
                <BusinessFailureState
                    id="sales-selection-detail-retry"
                    title="选品册加载失败"
                    error={detailQuery.error}
                    description="可能是选品册不存在，或当前账号无权查看。"
                    onRetry={() => void detailQuery.refetch()}
                />
            </PageScaffold>
        )
    }

    const visibleItems = detail.items.filter((item) => !item.removed)
    const activeBookId = bookIdentity(detail) || bookId

    return (
        <PageScaffold density="compact">
            <BookDetailHeader
                detail={detail}
                pending={pending}
                operations={operations}
                onEdit={() => setEditing(true)}
            />

            {editing ? (
                <ReprepareDialog
                    key={detail.version}
                    detail={detail}
                    onClose={() => setEditing(false)}
                />
            ) : null}

            {/* 流程推进向导横幅 */}
            <BookWorkflowBanner
                detail={detail}
                pending={pending}
                operations={operations}
                onEdit={() => setEditing(true)}
            />

            {/* 双栏工作台：左侧陈列预览与核对 + 右侧档案与控制侧边栏 */}
            <div className="grid min-w-0 items-start gap-6 xl:grid-cols-[minmax(0,1fr)_340px] 2xl:grid-cols-[minmax(0,1fr)_380px]">
                {/* 左侧主要工作面：商品与套餐陈列预览 */}
                <div
                    className={cn(
                        surfacePanelClassName,
                        "min-w-0 overflow-hidden rounded-xl border border-border p-5",
                    )}
                >
                    <div className="mb-4">
                        <div className="flex items-baseline justify-between gap-2">
                            <h2 className="text-sm font-semibold text-foreground">
                                陈列预览
                                <span className="ml-2 font-normal text-xs text-muted-foreground">
                                    （共 {visibleItems.length} 项）
                                </span>
                            </h2>
                        </div>
                        <p className="mt-0.5 text-xs text-muted-foreground">
                            {detail.status === "PENDING_PUBLISH"
                                ? "待发布时可剔除不需要的陈列项；套餐可单档重生成。发布后陈列将正式冻结。"
                                : "陈列内容来自最近一次准备结果。"}
                        </p>
                    </div>

                    <PreviewGrid
                        items={visibleItems.map((item) => ({
                            ...item,
                            item_id: item.item_id || item.id || "",
                            kind: item.kind ?? "SINGLE_SKU",
                            price_gross: item.price_gross || item.price || "0",
                        }))}
                        selectionForm={detail.selection_form}
                        tiers={detail.tiers}
                        onRegenerateTier={
                            detail.status === "PENDING_PUBLISH"
                                ? (tierId) =>
                                      void operations.regenerate.mutateAsync({
                                          bookId: activeBookId,
                                          tier_ids: [tierId],
                                          expected_version: detail.version,
                                          idempotency_key:
                                              createIdempotencyKey(),
                                      })
                                : undefined
                        }
                        onDelete={
                            detail.status === "PENDING_PUBLISH"
                                ? (itemId) =>
                                      void operations.deleteItem.mutateAsync({
                                          bookId: activeBookId,
                                          itemId,
                                          expected_version: detail.version,
                                      })
                                : undefined
                        }
                    />
                </div>

                {/* 右侧吸顶控制台与档案侧边栏 */}
                <div className="min-w-0 xl:sticky xl:top-4">
                    <BookDetailSidebar
                        detail={detail}
                        pending={pending}
                        operations={operations}
                    />
                </div>
            </div>
        </PageScaffold>
    )
}
