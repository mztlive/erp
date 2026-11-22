/**
 * 选品册详情页：对象页头 + 流程向导 + 全宽陈列工作面与按需查看的资料抽屉。
 * 发布固定已确认批次，不回传浏览器明细替代后端快照。
 */

"use client"

import * as React from "react"
import Link from "next/link"

import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import { hasPermission } from "@/lib/permissions"
import { AccessPasswordDialog } from "@/features/sales-selection/components/access-password-dialog"
import { BookDetailVoucherPanel } from "@/features/sales-selection/components/book-detail-voucher-panel"
import { BookDetailHeader } from "@/features/sales-selection/components/book-detail-header"
import { BookCopyDialog } from "@/features/sales-selection/components/book-copy-dialog"
import { useRouter } from "@/lib/navigation"
import { BookDetailInfoSheet } from "@/features/sales-selection/components/book-detail-info-sheet"
import { BookWorkflowBanner } from "@/features/sales-selection/components/book-workflow-banner"
import { PreviewGrid } from "@/features/sales-selection/components/preview-grid"
import { PriceEditDialog } from "@/features/sales-selection/components/price-edit-dialog"
import type { DisplayItem } from "@/features/sales-selection/types"
import { ReprepareDialog } from "@/features/sales-selection/components/reprepare-dialog"
import {
    useBookDetail,
    useBookOperations,
} from "@/features/sales-selection/hooks/queries"
import {
    bookIdentity,
    prepareFailure,
} from "@/features/sales-selection/lib/presentation"
import { createIdempotencyKey } from "@/features/sales-selection/lib/validation"
import { cn } from "@/lib/utils"

/**
 * 选品册详情页。
 * @param bookId 选品册身份
 */
export const BookDetailPage = ({ bookId }: { bookId: string }) => {
    const router = useRouter()
    const profile = useAccountProfileQuery()
    const canCopy =
        hasPermission(
            profile.data?.permissions,
            "sales_selection_booklet:create",
        ) &&
        hasPermission(profile.data?.permissions, "sales_selection_booklet:get")
    const canMaintain = hasPermission(
        profile.data?.permissions,
        "sales_selection_booklet:maintain",
    )
    const permissions = profile.data?.permissions
    const canPrepare = hasPermission(
        permissions,
        "sales_selection_booklet:prepare",
    )
    const canCopyLink = hasPermission(
        permissions,
        "sales_selection_booklet:copy_link",
    )
    const canOpenProposal = hasPermission(
        permissions,
        "sales_selection_proposal:get",
    )
    const detailQuery = useBookDetail(bookId)
    const operations = useBookOperations()
    const detail = detailQuery.data
    const [infoOpen, setInfoOpen] = React.useState(false)
    const [editing, setEditing] = React.useState(false)
    const [copying, setCopying] = React.useState(false)
    const [passwordEditing, setPasswordEditing] = React.useState(false)
    const [priceEditing, setPriceEditing] = React.useState<{
        item?: DisplayItem
    } | null>(null)

    const pending =
        operations.updatePrices.isPending ||
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
            <DetailWorkspaceSkeleton
                title="选品册"
                description="正在加载选品册…"
            />
        )
    }

    if (detailQuery.isError || !detail) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="选品册"
                    secondaryActions={
                        <Button
                            id="sales-selection-detail-back-error"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietButton}
                            size="sm"
                            render={<Link href="/sales/selection" />}
                        >
                            返回列表
                        </Button>
                    }
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            id="sales-selection-detail-retry"
                            title="选品册加载失败"
                            error={detailQuery.error}
                            description="可能是选品册不存在，或当前账号无权查看。"
                            onRetry={() => void detailQuery.refetch()}
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const visibleItems = detail.items.filter((item) => !item.removed)
    const failure = prepareFailure(detail)
    const emptyState =
        detail.status === "PREPARING"
            ? {
                  title: "正在准备陈列",
                  description:
                      "准备完成后将在这里显示商品，系统会自动更新进度，无需重复操作。",
              }
            : failure && !failure.preservesDisplay
              ? {
                    title: "本次准备未完成",
                    description: failure.needsSourceChange
                        ? "请在上方调整商品来源，重新准备成功后将在这里显示商品。"
                        : "请在上方点击「重新准备」，成功后将在这里显示商品。",
                }
              : undefined
    const activeBookId = bookIdentity(detail) || bookId
    const passwordDisabledReason =
        detail.status === "PREPARING"
            ? "商品正在准备，请等待准备结束后设置或修改访问密码。"
            : undefined

    return (
        <DetailWorkspacePage>
            <BookDetailHeader
                detail={detail}
                pending={pending}
                permissions={permissions}
                operations={operations}
                onOpenInfo={() => setInfoOpen(true)}
                onEdit={() => setEditing(true)}
                onCopy={canCopy ? () => setCopying(true) : undefined}
            />
            <DetailWorkspaceBody>
                <DetailWorkspaceSection
                    title="访问密码"
                    ariaLabel="选品册访问密码"
                    width="form"
                    actions={
                        canMaintain ? (
                            <Button
                                id="sales-selection-detail-access-password"
                                variant="ghost"
                                className={detailWorkspaceStyles.subtleButton}
                                size="sm"
                                disabled={
                                    pending || Boolean(passwordDisabledReason)
                                }
                                title={passwordDisabledReason}
                                aria-describedby={
                                    passwordDisabledReason
                                        ? "sales-selection-detail-access-password-wait"
                                        : undefined
                                }
                                onClick={() => setPasswordEditing(true)}
                            >
                                {detail.access_password_set
                                    ? "修改密码"
                                    : "设置密码"}
                            </Button>
                        ) : null
                    }
                    contentClassName="space-y-2"
                >
                    <p className="text-body-compact text-muted-foreground">
                        {detail.access_password_set
                            ? "客户需验证访问密码后查看选品册。"
                            : "该选品册尚未设置密码，客户无法查看商品。请设置密码后发给客户。"}
                    </p>
                    {canMaintain && passwordDisabledReason && (
                        <p
                            id="sales-selection-detail-access-password-wait"
                            className="text-xs text-muted-foreground"
                        >
                            {passwordDisabledReason}
                        </p>
                    )}
                </DetailWorkspaceSection>
                <BookDetailVoucherPanel detail={detail} />
                <DetailWorkspaceSection ariaLabel="选品流程">
                    <BookWorkflowBanner
                        detail={detail}
                        pending={pending}
                        operations={operations}
                        canPrepare={canPrepare}
                        canCopyLink={canCopyLink}
                        canOpenProposal={canOpenProposal}
                        onEdit={canPrepare ? () => setEditing(true) : undefined}
                    />
                </DetailWorkspaceSection>
                <DetailWorkspaceSection title="陈列预览">
                    <PreviewGrid
                        summary={
                            <dl
                                aria-label="陈列指标"
                                className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground"
                            >
                                <div className="flex gap-1">
                                    <dt>有效陈列</dt>
                                    <dd className="num font-medium text-foreground">
                                        {detail.display_count}
                                    </dd>
                                </div>
                                <div className="flex gap-1">
                                    <dt>已剔除</dt>
                                    <dd className="num">
                                        {detail.removed_count}
                                    </dd>
                                </div>
                                <div
                                    className={cn(
                                        "flex gap-1",
                                        detail.missing_image_count > 0 &&
                                            "font-medium text-warning-soft-foreground",
                                    )}
                                >
                                    <dt>缺少图片</dt>
                                    <dd className="num">
                                        {detail.missing_image_count}
                                    </dd>
                                </div>
                                {detail.selection_form === "PACKAGE" ? (
                                    <div className="flex gap-1">
                                        <dt>选品档位</dt>
                                        <dd className="num">
                                            {detail.tiers.length} 档
                                        </dd>
                                    </div>
                                ) : null}
                            </dl>
                        }
                        actions={
                            detail.status === "PENDING_PUBLISH" &&
                            canMaintain &&
                            visibleItems.length > 0 ? (
                                <Button
                                    id="selection-prices-markup"
                                    variant="ghost"
                                    className={
                                        detailWorkspaceStyles.subtleButton
                                    }
                                    size="sm"
                                    disabled={pending || !detail.batch_id}
                                    onClick={() => setPriceEditing({})}
                                >
                                    统一加价
                                </Button>
                            ) : null
                        }
                        emptyState={emptyState}
                        items={visibleItems.map((item) => ({
                            ...item,
                            item_id: item.item_id || item.id || "",
                            kind: item.kind ?? "SINGLE_SKU",
                            price_gross: item.price_gross || item.price || "0",
                        }))}
                        priceEditor={
                            detail.status === "PENDING_PUBLISH" &&
                            canMaintain &&
                            detail.batch_id
                                ? {
                                      batchId: detail.batch_id,
                                      version: detail.version,
                                      onSubmit: (input) =>
                                          operations.updatePrices.mutateAsync({
                                              bookId: activeBookId,
                                              ...input,
                                          }),
                                  }
                                : undefined
                        }
                        selectionForm={detail.selection_form}
                        tiers={detail.tiers}
                        pending={pending}
                        onEditPrice={
                            detail.status === "PENDING_PUBLISH" &&
                            canMaintain &&
                            detail.batch_id
                                ? (item) => setPriceEditing({ item })
                                : undefined
                        }
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
                            canPrepare && detail.status === "PENDING_PUBLISH"
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
                            canMaintain && detail.status === "PENDING_PUBLISH"
                                ? (itemId) =>
                                      void operations.deleteItem.mutateAsync({
                                          bookId: activeBookId,
                                          itemId,
                                          expected_version: detail.version,
                                      })
                                : undefined
                        }
                    />
                </DetailWorkspaceSection>
            </DetailWorkspaceBody>

            {canCopy && copying ? (
                <BookCopyDialog
                    source={detail}
                    onClose={() => setCopying(false)}
                    onCopied={(copied) => {
                        setCopying(false)
                        router.push(`/sales/selection/${bookIdentity(copied)}`)
                    }}
                />
            ) : null}

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
            {canPrepare && editing ? (
                <ReprepareDialog
                    key={detail.version}
                    detail={detail}
                    onClose={() => setEditing(false)}
                />
            ) : null}
            {priceEditing &&
            detail.status === "PENDING_PUBLISH" &&
            canMaintain &&
            detail.batch_id ? (
                <PriceEditDialog
                    item={priceEditing.item}
                    batchId={detail.batch_id}
                    version={detail.version}
                    busy={pending}
                    onClose={() => setPriceEditing(null)}
                    onSubmit={(input) =>
                        operations.updatePrices.mutateAsync({
                            bookId: activeBookId,
                            ...input,
                        })
                    }
                />
            ) : null}

            <BookDetailInfoSheet
                open={infoOpen}
                onOpenChange={setInfoOpen}
                detail={detail}
                pending={pending}
                operations={operations}
            />
        </DetailWorkspacePage>
    )
}
