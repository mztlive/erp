/**
 * 选品册详情页：对象页头 + 流程向导 + 全宽陈列工作面与按需查看的资料抽屉。
 * 发布固定已确认批次，不回传浏览器明细替代后端快照。
 */

"use client"

import * as React from "react"
import Link from "next/link"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    GuardedBusinessAction,
    detailWorkspaceStyles,
} from "@/components/business"
import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import { isNotFoundError } from "@/lib/api/errors"
import {
    FormalCommandKeyLedger,
    classifyFormalCommandError,
} from "@/lib/formal-command"
import { useRouter } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"
import { AccessPasswordDialog } from "@/features/sales-selection/components/access-password-dialog"
import { BookDetailHeader } from "@/features/sales-selection/components/book-detail-header"
import { BookCopyDialog } from "@/features/sales-selection/components/book-copy-dialog"
import { BookDetailInfoSheet } from "@/features/sales-selection/components/book-detail-info-sheet"
import {
    BookPrepareFailureAlert,
    BookWorkflowBanner,
} from "@/features/sales-selection/components/book-workflow-banner"
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
import { cn } from "@/lib/utils"

type BookConfirmAction =
    | { kind: "void" }
    | { kind: "close" }
    | { kind: "revoke" }
    | { kind: "replaceLink" }
    | { kind: "regenerateAll" }
    | { kind: "regenerateTier"; tierId: string; tierName: string }
    | { kind: "deleteItem"; itemId: string; itemName: string }
    | { kind: "publishRisk"; risks: string[] }

const backToList = {
    id: "sales-selection-detail-back",
    label: "选品册列表",
    href: "/sales/selection",
} as const

/**
 * 选品册详情页。
 * @param bookId 选品册身份
 */
export const BookDetailPage = ({ bookId }: { bookId: string }) => {
    const router = useRouter()
    const profile = useAccountProfileQuery()
    const permissions = profile.data?.permissions
    const canCopy =
        hasPermission(permissions, "sales_selection_booklet:create") &&
        hasPermission(permissions, "sales_selection_booklet:get")
    const canMaintain = hasPermission(
        permissions,
        "sales_selection_booklet:maintain",
    )
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

    const ledgerRef = React.useRef<{
        bookId: string
        ledger: FormalCommandKeyLedger
    }>({
        bookId,
        ledger: new FormalCommandKeyLedger(),
    })
    if (ledgerRef.current.bookId !== bookId) {
        ledgerRef.current = {
            bookId,
            ledger: new FormalCommandKeyLedger(),
        }
    }

    const [infoOpen, setInfoOpen] = React.useState(false)
    const [editing, setEditing] = React.useState(false)
    const [copying, setCopying] = React.useState(false)
    const [passwordEditing, setPasswordEditing] = React.useState(false)
    const [confirmAction, setConfirmAction] =
        React.useState<BookConfirmAction | null>(null)
    const [priceEditing, setPriceEditing] = React.useState<{
        item?: DisplayItem
    } | null>(null)
    const [savingSkuIds, setSavingSkuIds] = React.useState<ReadonlySet<string>>(
        () => new Set(),
    )

    const isBulkPriceUpdating =
        operations.updatePrices.isPending &&
        operations.updatePrices.variables?.change.kind !== "SINGLE"

    const pending =
        isBulkPriceUpdating ||
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

    const runWithLedger = React.useCallback(
        async <T,>(
            slot: string,
            prefix: string,
            payload: T,
            run: (identity: {
                idempotencyKey: string
                payload: T
            }) => Promise<unknown>,
        ) => {
            const ledger = ledgerRef.current.ledger
            const identity = ledger.acquire(slot, prefix, payload)
            try {
                await run(identity)
                ledger.settle(slot, "succeeded")
            } catch (error) {
                ledger.settle(slot, classifyFormalCommandError(error))
                throw error
            }
        },
        [],
    )

    if (detailQuery.isPending) {
        return (
            <DetailWorkspaceSkeleton
                title="选品册"
                description="正在加载选品册…"
            />
        )
    }

    if (detailQuery.isError && !isNotFoundError(detailQuery.error)) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="选品册"
                    back={backToList}
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
                            description="请稍后重试，或返回选品册列表确认。"
                            onRetry={() => void detailQuery.refetch()}
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (
        !detail ||
        (detailQuery.isError && isNotFoundError(detailQuery.error))
    ) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="选品册不存在" back={backToList} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessEmptyState
                            kind="no-data"
                            title="未找到这份选品册"
                            description="可能选品册编号有误，或当前账号无权查看。"
                            action={
                                <Button
                                    id="sales-selection-detail-back-error"
                                    size="sm"
                                    render={<Link href="/sales/selection" />}
                                >
                                    返回选品册列表
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const visibleItems = detail.items.filter((item) => !item.removed)
    const failure = prepareFailure(detail)
    const isTerminal = detail.status === "CLOSED" || detail.status === "VOIDED"
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
            : isTerminal
              ? `该选品册已${detail.status === "CLOSED" ? "关闭" : "作废"}，不可修改访问密码。`
              : undefined

    const executePrepare = () => {
        const slot = `prepare:${activeBookId}:${detail.version}`
        void runWithLedger(
            slot,
            "selection-prepare",
            { bookId: activeBookId, version: detail.version },
            ({ idempotencyKey }) =>
                operations.prepare.mutateAsync({
                    bookId: activeBookId,
                    kind: "FIRST_PREPARE",
                    expected_version: detail.version,
                    idempotency_key: idempotencyKey,
                }),
        ).catch(() => undefined)
    }

    const executePublish = () => {
        const slot = `publish:${activeBookId}:${detail.version}`
        void runWithLedger(
            slot,
            "selection-publish",
            {
                bookId: activeBookId,
                version: detail.version,
                batchId: detail.batch_id ?? undefined,
            },
            ({ idempotencyKey }) =>
                operations.publish.mutateAsync({
                    bookId: activeBookId,
                    batch_id: detail.batch_id ?? undefined,
                    expected_version: detail.version,
                    idempotency_key: idempotencyKey,
                }),
        ).catch(() => undefined)
    }

    const handleRequestPublish = () => {
        const risks: string[] = []
        if (!detail.access_password_set) {
            risks.push(
                "尚未设置访问密码：发布后客户打开链接将无法解锁查看商品，建议先设置密码再发布。",
            )
        }
        if (detail.missing_image_count > 0) {
            risks.push(
                `有 ${detail.missing_image_count} 项陈列缺少商品图片：客户选品页将显示缺图占位，建议核对后再发布。`,
            )
        }
        if (risks.length > 0) {
            setConfirmAction({ kind: "publishRisk", risks })
            return
        }
        executePublish()
    }

    const handleConfirmAction = async () => {
        if (!confirmAction) return
        const action = confirmAction
        setConfirmAction(null)
        try {
            switch (action.kind) {
                case "publishRisk":
                    executePublish()
                    break
                case "void":
                    await runWithLedger(
                        `void:${activeBookId}:${detail.version}`,
                        "selection-void",
                        { bookId: activeBookId, version: detail.version },
                        ({ idempotencyKey }) =>
                            operations.void.mutateAsync({
                                bookId: activeBookId,
                                expected_version: detail.version,
                                idempotency_key: idempotencyKey,
                            }),
                    )
                    break
                case "close":
                    await runWithLedger(
                        `close:${activeBookId}:${detail.version}`,
                        "selection-close",
                        { bookId: activeBookId, version: detail.version },
                        ({ idempotencyKey }) =>
                            operations.close.mutateAsync({
                                bookId: activeBookId,
                                expected_version: detail.version,
                                idempotency_key: idempotencyKey,
                            }),
                    )
                    break
                case "revoke":
                    await runWithLedger(
                        `revoke:${activeBookId}:${detail.version}`,
                        "selection-revoke",
                        { bookId: activeBookId, version: detail.version },
                        ({ idempotencyKey }) =>
                            operations.revoke.mutateAsync({
                                bookId: activeBookId,
                                expected_version: detail.version,
                                idempotency_key: idempotencyKey,
                            }),
                    )
                    break
                case "replaceLink":
                    await runWithLedger(
                        `replace-link:${activeBookId}:${detail.version}`,
                        "selection-replace-link",
                        { bookId: activeBookId, version: detail.version },
                        ({ idempotencyKey }) =>
                            operations.replaceLink.mutateAsync({
                                bookId: activeBookId,
                                expected_version: detail.version,
                                idempotency_key: idempotencyKey,
                            }),
                    )
                    break
                case "regenerateAll":
                    await runWithLedger(
                        `regenerate-all:${activeBookId}:${detail.version}`,
                        "selection-regenerate-all",
                        { bookId: activeBookId, version: detail.version },
                        ({ idempotencyKey }) =>
                            operations.regenerate.mutateAsync({
                                bookId: activeBookId,
                                expected_version: detail.version,
                                idempotency_key: idempotencyKey,
                            }),
                    )
                    break
                case "regenerateTier":
                    await runWithLedger(
                        `regenerate-tier:${activeBookId}:${detail.version}:${action.tierId}`,
                        "selection-regenerate-tier",
                        {
                            bookId: activeBookId,
                            version: detail.version,
                            tierId: action.tierId,
                        },
                        ({ idempotencyKey }) =>
                            operations.regenerate.mutateAsync({
                                bookId: activeBookId,
                                tier_ids: [action.tierId],
                                expected_version: detail.version,
                                idempotency_key: idempotencyKey,
                            }),
                    )
                    break
                case "deleteItem":
                    await operations.deleteItem.mutateAsync({
                        bookId: activeBookId,
                        itemId: action.itemId,
                        expected_version: detail.version,
                    })
                    break
            }
        } catch {
            // 错误提示由 mutation 全局处理
        }
    }

    const confirmMeta = (() => {
        if (!confirmAction) return null
        switch (confirmAction.kind) {
            case "void":
                return {
                    title: "确认作废选品册？",
                    description:
                        "作废后该选品册将永久终止，不可恢复或再次发布。确认要继续吗？",
                    confirmLabel: "确认作废",
                    destructive: true,
                }
            case "close":
                return {
                    title: "确认关闭选品册？",
                    description:
                        "关闭后客户选品链接将立即失效，不再接受新的选品提交，且不可重新打开。",
                    confirmLabel: "确认关闭",
                    destructive: true,
                }
            case "revoke":
                return {
                    title: "确认撤销链接访问？",
                    description:
                        "撤销后客户公开选品页将立即失效且不再展示内容，已提交的销售方案不受影响。",
                    confirmLabel: "确认撤销",
                    destructive: true,
                }
            case "replaceLink":
                return {
                    title: "确认更换客户链接？",
                    description:
                        "更换后旧的客户选品链接将立即失效，您需要复制新链接重新发送给客户。",
                    confirmLabel: "确认更换",
                    destructive: false,
                }
            case "regenerateAll":
                return {
                    title: "确认重新生成整册？",
                    description:
                        "将沿用当前商品池重新生成所有档位套餐，当前陈列与已自定义的客户售价将被重置，需重新核对。",
                    confirmLabel: "确认重新生成",
                    destructive: false,
                }
            case "regenerateTier":
                return {
                    title: `确认重新生成「${confirmAction.tierName}」档位？`,
                    description:
                        "将沿用当前商品池重新生成该档位套餐，该档位现有陈列与自定义客户售价将被重置。",
                    confirmLabel: "确认重新生成",
                    destructive: false,
                }
            case "deleteItem":
                return {
                    title: "确认删除该陈列项？",
                    description: `确定将「${confirmAction.itemName}」从本选品册移除？删除后不可直接撤销，如需恢复需重新生成或重新准备。`,
                    confirmLabel: "确认删除",
                    destructive: true,
                }
            case "publishRisk":
                return {
                    title: "发布前风险核对",
                    description:
                        "当前选品册存在以下待完善项，发布后商品清单与售价将锁定对客展示：",
                    risks: confirmAction.risks,
                    confirmLabel: "仍要发布",
                    destructive: false,
                }
        }
    })()

    const handleInlinePriceSubmit = async (
        input: Parameters<
            typeof operations.updatePrices.mutateAsync
        >[0] extends infer P
            ? Omit<P, "bookId">
            : never,
    ) => {
        const skuId =
            input.change.kind === "SINGLE" ? input.change.sku_id : undefined
        if (skuId) {
            setSavingSkuIds((prev) => {
                const next = new Set(prev)
                next.add(skuId)
                return next
            })
        }
        try {
            return await operations.updatePrices.mutateAsync({
                bookId: activeBookId,
                ...input,
            })
        } finally {
            if (skuId) {
                setSavingSkuIds((prev) => {
                    const next = new Set(prev)
                    next.delete(skuId)
                    return next
                })
            }
        }
    }

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
                onPrepare={executePrepare}
                onPublish={handleRequestPublish}
                onRegenerateAll={() =>
                    setConfirmAction({ kind: "regenerateAll" })
                }
                onReplaceLink={() => setConfirmAction({ kind: "replaceLink" })}
                onClose={() => setConfirmAction({ kind: "close" })}
                onRevoke={() => setConfirmAction({ kind: "revoke" })}
                onVoid={() => setConfirmAction({ kind: "void" })}
            />
            <DetailWorkspaceBody
                alerts={
                    failure ? (
                        <BookPrepareFailureAlert
                            detail={detail}
                            pending={pending}
                            canPrepare={canPrepare}
                            preparePending={operations.prepare.isPending}
                            onRetryPrepare={executePrepare}
                            onEdit={
                                canPrepare ? () => setEditing(true) : undefined
                            }
                        />
                    ) : undefined
                }
            >
                <DetailWorkspaceSection ariaLabel="选品流程">
                    <BookWorkflowBanner
                        detail={detail}
                        pending={pending}
                        operations={operations}
                        canPrepare={canPrepare}
                        canCopyLink={canCopyLink}
                        canOpenProposal={canOpenProposal}
                        onRetryPrepare={executePrepare}
                        onEdit={canPrepare ? () => setEditing(true) : undefined}
                        showFailureAlert={false}
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
                                      onSubmit: handleInlinePriceSubmit,
                                  }
                                : undefined
                        }
                        selectionForm={detail.selection_form}
                        tiers={detail.tiers}
                        pending={pending}
                        savingSkuIds={savingSkuIds}
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
                                ? (tierId, tierName) =>
                                      setConfirmAction({
                                          kind: "regenerateTier",
                                          tierId,
                                          tierName,
                                      })
                                : undefined
                        }
                        onDelete={
                            canMaintain && detail.status === "PENDING_PUBLISH"
                                ? (item) =>
                                      setConfirmAction({
                                          kind: "deleteItem",
                                          itemId: item.item_id,
                                          itemName: item.name,
                                      })
                                : undefined
                        }
                    />
                </DetailWorkspaceSection>
                <DetailWorkspaceSection
                    title="访问密码"
                    ariaLabel="选品册访问密码"
                    width="form"
                    actions={
                        canMaintain ? (
                            <GuardedBusinessAction
                                id="sales-selection-detail-access-password"
                                variant="ghost"
                                className={detailWorkspaceStyles.subtleButton}
                                size="sm"
                                disabled={
                                    pending || Boolean(passwordDisabledReason)
                                }
                                reason={passwordDisabledReason}
                                onClick={() => setPasswordEditing(true)}
                            >
                                {detail.access_password_set
                                    ? "修改密码"
                                    : "设置密码"}
                            </GuardedBusinessAction>
                        ) : null
                    }
                    contentClassName="space-y-2"
                >
                    <p className="text-body-compact text-muted-foreground">
                        {detail.access_password_set
                            ? "客户需验证访问密码后查看选品册。"
                            : "该选品册尚未设置密码，客户无法查看商品。请设置密码后发给客户。"}
                    </p>
                </DetailWorkspaceSection>
            </DetailWorkspaceBody>

            <AlertDialog
                open={confirmMeta != null}
                onOpenChange={(open) => {
                    if (!open) setConfirmAction(null)
                }}
            >
                <AlertDialogContent>
                    <AlertDialogHeader>
                        <AlertDialogTitle>
                            {confirmMeta?.title}
                        </AlertDialogTitle>
                        <AlertDialogDescription>
                            {confirmMeta?.description}
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    {confirmMeta?.risks ? (
                        <ul className="list-disc space-y-1.5 pl-5 text-body-compact text-foreground">
                            {confirmMeta.risks.map((risk) => (
                                <li key={risk}>{risk}</li>
                            ))}
                        </ul>
                    ) : null}
                    <AlertDialogFooter>
                        <AlertDialogCancel id="sales-selection-detail-confirm-cancel">
                            取消
                        </AlertDialogCancel>
                        <AlertDialogAction
                            id="sales-selection-detail-confirm-ok"
                            variant={
                                confirmMeta?.destructive
                                    ? "destructive"
                                    : "default"
                            }
                            onClick={() => void handleConfirmAction()}
                        >
                            {confirmMeta?.confirmLabel}
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>

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
                open={passwordEditing && canMaintain && !passwordDisabledReason}
                onOpenChange={setPasswordEditing}
                busy={pending || !canMaintain}
                disabledReason={passwordDisabledReason}
                onSubmit={async (password) => {
                    if (!canMaintain || pending || passwordDisabledReason)
                        return
                    await runWithLedger(
                        `access-password:${activeBookId}:${detail.version}`,
                        "selection-password",
                        { bookId: activeBookId, version: detail.version },
                        ({ idempotencyKey }) =>
                            operations.accessPassword.mutateAsync({
                                bookId: activeBookId,
                                expected_version: detail.version,
                                idempotency_key: idempotencyKey,
                                access_password: password,
                            }),
                    )
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
                canPrepare={canPrepare}
                canCopyLink={canCopyLink}
                canOpenProposal={canOpenProposal}
                onRegenerateTier={
                    canPrepare && detail.status === "PENDING_PUBLISH"
                        ? (tierId, tierName) => {
                              setInfoOpen(false)
                              setConfirmAction({
                                  kind: "regenerateTier",
                                  tierId,
                                  tierName,
                              })
                          }
                        : undefined
                }
            />
        </DetailWorkspacePage>
    )
}
