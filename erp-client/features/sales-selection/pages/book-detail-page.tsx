/**
 * 选品册详情页：对象页头 + 概览分区 + 陈列预览。
 * 发布固定已确认批次，不回传浏览器明细替代后端快照。
 */

"use client"

import * as React from "react"
import Link from "next/link"

import {
    BusinessFailureState,
    DocumentSection,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { BookDetailHeader } from "@/features/sales-selection/components/book-detail-header"
import { BookDetailOverview } from "@/features/sales-selection/components/book-detail-overview"
import { PreviewGrid } from "@/features/sales-selection/components/preview-grid"
import { ReprepareDialog } from "@/features/sales-selection/components/reprepare-dialog"
import {
    useBookDetail,
    useBookOperations,
} from "@/features/sales-selection/hooks/queries"
import { bookIdentity } from "@/features/sales-selection/lib/presentation"
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
                    <Skeleton className="h-24 w-full rounded-xl" />
                    <Skeleton className="h-64 w-full rounded-xl" />
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

            <div
                className={cn(
                    surfacePanelClassName,
                    "min-w-0 overflow-hidden px-5 pt-2",
                )}
            >
                <BookDetailOverview
                    detail={detail}
                    pending={pending}
                    operations={operations}
                />
                <DocumentSection
                    title={`陈列预览（${visibleItems.length} 项）`}
                    description={
                        detail.status === "PENDING_PUBLISH"
                            ? "待发布时可删除不需要的陈列项，发布后将冻结。"
                            : "陈列内容来自最近一次准备结果。"
                    }
                >
                    <PreviewGrid
                        items={visibleItems.map((item) => ({
                            ...item,
                            item_id: item.item_id || item.id || "",
                            kind: item.kind ?? "SINGLE_SKU",
                            price_gross: item.price_gross || item.price || "0",
                        }))}
                        selectionForm={detail.selection_form}
                        onDelete={
                            detail.status === "PENDING_PUBLISH"
                                ? (itemId) =>
                                      void operations.deleteItem.mutateAsync({
                                          bookId:
                                              bookIdentity(detail) || bookId,
                                          itemId,
                                          expected_version: detail.version,
                                      })
                                : undefined
                        }
                    />
                </DocumentSection>
            </div>
        </PageScaffold>
    )
}
