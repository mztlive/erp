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
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import { hasPermission } from "@/lib/permissions"
import { AccessPasswordDialog } from "@/features/sales-selection/components/access-password-dialog"
import { BookDetailVoucherPanel } from "@/features/sales-selection/components/book-detail-voucher-panel"
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
    const profile = useAccountProfileQuery()
    const canMaintain = hasPermission(
        profile.data?.permissions,
        "sales_selection_booklet:maintain",
    )
    const detailQuery = useBookDetail(bookId)
    const operations = useBookOperations()
    const detail = detailQuery.data
    const [editing, setEditing] = React.useState(false)
    const [passwordEditing, setPasswordEditing] = React.useState(false)

    const pending =
        operations.accessPassword.isPending ||
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
    const passwordDisabledReason =
        detail.status === "PREPARING"
            ? "商品正在准备，请等待准备结束后设置或修改访问密码。"
            : undefined

    return (
        <PageScaffold density="compact">
            <BookDetailHeader
                detail={detail}
                pending={pending}
                operations={operations}
                onEdit={() => setEditing(true)}
            />

            <section
                className="flex flex-wrap items-center justify-between gap-3 rounded-xl border bg-card p-4"
                aria-label="选品册访问密码"
            >
                <div className="space-y-1">
                    <h2 className="text-sm font-semibold">访问密码</h2>
                    <p className="text-xs text-muted-foreground">
                        {detail.access_password_set
                            ? "客户需验证访问密码后查看选品册。"
                            : "该选品册尚未设置密码，客户无法查看商品。请设置密码后发给客户。"}
                    </p>
                    {passwordDisabledReason && (
                        <p
                            id="sales-selection-detail-access-password-wait"
                            className="text-xs text-muted-foreground"
                        >
                            {passwordDisabledReason}
                        </p>
                    )}
                </div>
                <Button
                    id="sales-selection-detail-access-password"
                    variant="outline"
                    size="sm"
                    disabled={
                        pending ||
                        !canMaintain ||
                        Boolean(passwordDisabledReason)
                    }
                    title={
                        !canMaintain
                            ? "当前账号无权维护选品册密码"
                            : passwordDisabledReason
                    }
                    aria-describedby={
                        passwordDisabledReason
                            ? "sales-selection-detail-access-password-wait"
                            : undefined
                    }
                    onClick={() => setPasswordEditing(true)}
                >
                    {detail.access_password_set ? "修改密码" : "设置密码"}
                </Button>
            </section>
            <AccessPasswordDialog
                open={passwordEditing && canMaintain}
                onOpenChange={setPasswordEditing}
                busy={pending || !canMaintain}
                disabledReason={passwordDisabledReason}
                onSubmit={async (password) => {
                    if (!canMaintain || pending || passwordDisabledReason)
                        return
                    await operations.accessPassword.mutateAsync({
                        bookId: activeBookId,
                        expected_version: detail.version,
                        idempotency_key: createIdempotencyKey(),
                        access_password: password,
                    })
                }}
            />
            <BookDetailVoucherPanel detail={detail} />

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
                        pending={pending}
                        deletingItemId={
                            operations.deleteItem.isPending
                                ? operations.deleteItem.variables?.itemId
                                : undefined
                        }
                        regeneratingTierIds={
                            operations.regenerate.isPending
                                ? operations.regenerate.variables?.tier_ids
                                : undefined
                        }
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
