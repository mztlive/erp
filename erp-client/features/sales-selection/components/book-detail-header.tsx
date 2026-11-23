"use client"

import * as React from "react"
import Link from "next/link"
import {
    BanIcon,
    ChevronDownIcon,
    CopyIcon,
    FileTextIcon,
    Link2Icon,
    Link2OffIcon,
    PencilIcon,
    RefreshCwIcon,
    SendIcon,
    SparklesIcon,
} from "lucide-react"

import {
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Popover,
    PopoverContent,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { hasPermission } from "@/lib/permissions"
import { useBookOperations } from "@/features/sales-selection/hooks/queries"
import {
    bookIdentity,
    bookletStatusTone,
    prepareFailure,
} from "@/features/sales-selection/lib/presentation"
import type { SelectionBookDetail } from "@/features/sales-selection/types"
import {
    BOOKLET_STATUS_LABEL,
    POOL_SOURCE_LABEL,
    SELECTION_FORM_LABEL,
} from "@/features/sales-selection/types"

type Operations = ReturnType<typeof useBookOperations>

/**
 * 选品册详情页头：返回列表、对象身份与当前状态可执行动作。
 * 所有高风险与写命令统一经页面级 FormalCommandKeyLedger 与确认弹窗执行。
 */
export function BookDetailHeader({
    detail,
    pending,
    permissions,
    operations,
    onEdit,
    onCopy,
    onOpenInfo,
    onPrepare,
    onPublish,
    onRegenerateAll,
    onReplaceLink,
    onClose,
    onRevoke,
    onVoid,
}: {
    detail: SelectionBookDetail
    pending: boolean
    permissions: readonly string[] | undefined
    operations: Operations
    onEdit: () => void
    onCopy?: () => void
    onOpenInfo: () => void
    onPrepare: () => void
    onPublish: () => void
    onRegenerateAll: () => void
    onReplaceLink: () => void
    onClose: () => void
    onRevoke: () => void
    onVoid: () => void
}) {
    const [moreOpen, setMoreOpen] = React.useState(false)
    const bookId = bookIdentity(detail)
    const formLabel = SELECTION_FORM_LABEL[detail.selection_form]
    const failure = prepareFailure(detail)

    const can = (action: string) =>
        hasPermission(permissions, `sales_selection_booklet:${action}`)
    const canOpenProposal = hasPermission(
        permissions,
        "sales_selection_proposal:get",
    )

    const runAndCloseMore = (action: () => void) => {
        setMoreOpen(false)
        action()
    }

    const primaryAction = (() => {
        if (detail.status === "DRAFT" && can("prepare")) {
            return (
                <LoadingButton
                    id="sales-selection-detail-prepare"
                    loading={operations.prepare.isPending}
                    type="button"
                    size="sm"
                    disabled={pending}
                    onClick={() => {
                        if (failure?.needsSourceChange) {
                            onEdit()
                            return
                        }
                        onPrepare()
                    }}
                >
                    <SparklesIcon data-icon="inline-start" aria-hidden="true" />
                    {operations.prepare.isPending
                        ? "正在启动准备"
                        : failure?.needsSourceChange
                          ? "调整商品来源"
                          : failure
                            ? "重新准备"
                            : "开始准备"}
                </LoadingButton>
            )
        }
        if (detail.status === "PENDING_PUBLISH" && can("publish")) {
            return (
                <LoadingButton
                    id="sales-selection-detail-publish"
                    loading={operations.publish.isPending}
                    type="button"
                    size="sm"
                    disabled={pending}
                    onClick={onPublish}
                >
                    <SendIcon data-icon="inline-start" aria-hidden="true" />
                    发布
                </LoadingButton>
            )
        }
        if (detail.status === "PUBLISHED" && can("copy_link")) {
            return (
                <LoadingButton
                    id="sales-selection-detail-copy-link"
                    loading={operations.copyLink.isPending}
                    type="button"
                    size="sm"
                    disabled={operations.copyLink.isPending || pending}
                    onClick={() => void operations.copyLink.mutateAsync(bookId)}
                >
                    <CopyIcon data-icon="inline-start" aria-hidden="true" />
                    复制链接
                </LoadingButton>
            )
        }
        if (
            detail.status === "SUBMITTED" &&
            detail.proposal_id &&
            canOpenProposal
        ) {
            return (
                <Button
                    id="sales-selection-detail-open-proposal"
                    type="button"
                    size="sm"
                    render={
                        <Link
                            href={`/sales/selection/proposals/${encodeURIComponent(detail.proposal_id)}`}
                        />
                    }
                >
                    查看销售方案
                    {detail.proposal_no ? ` ${detail.proposal_no}` : ""}
                </Button>
            )
        }
        return null
    })()

    const showCopy = Boolean(onCopy)
    const showRegenerateAll =
        detail.status === "PENDING_PUBLISH" &&
        detail.selection_form === "PACKAGE" &&
        can("prepare")
    const showReplaceLink = detail.status === "PUBLISHED" && can("rotate_link")
    const showClose = detail.status === "PUBLISHED" && can("close")
    const showRevoke = detail.status === "SUBMITTED" && can("revoke")
    const showVoid =
        (detail.status === "DRAFT" || detail.status === "PENDING_PUBLISH") &&
        can("void")

    const hasMoreActions =
        showCopy ||
        showRegenerateAll ||
        showReplaceLink ||
        showClose ||
        showRevoke ||
        showVoid

    const secondaryActions = (
        <>
            <Button
                id="sales-selection-detail-info"
                type="button"
                size="sm"
                variant="ghost"
                className={detailWorkspaceStyles.quietButton}
                aria-haspopup="dialog"
                onClick={onOpenInfo}
            >
                <FileTextIcon data-icon="inline-start" aria-hidden="true" />
                选品册资料
            </Button>
            {detail.status === "DRAFT" && can("prepare") ? (
                <Button
                    id="selection-draft-edit"
                    type="button"
                    size="sm"
                    variant="ghost"
                    className={detailWorkspaceStyles.quietButton}
                    disabled={pending}
                    onClick={onEdit}
                >
                    <PencilIcon data-icon="inline-start" aria-hidden="true" />
                    调整来源与档位
                </Button>
            ) : null}
            {detail.status === "PENDING_PUBLISH" && can("prepare") ? (
                <Button
                    id="sales-selection-detail-reprepare"
                    type="button"
                    size="sm"
                    variant="ghost"
                    className={detailWorkspaceStyles.quietButton}
                    disabled={pending}
                    onClick={onEdit}
                >
                    <PencilIcon data-icon="inline-start" aria-hidden="true" />
                    调整来源与档位
                </Button>
            ) : null}
            {canOpenProposal &&
            detail.proposal_id &&
            detail.proposal_no &&
            detail.status !== "SUBMITTED" ? (
                <Button
                    id="sales-selection-detail-open-proposal"
                    type="button"
                    size="sm"
                    variant="ghost"
                    className={detailWorkspaceStyles.quietButton}
                    render={
                        <Link
                            href={`/sales/selection/proposals/${encodeURIComponent(detail.proposal_id)}`}
                        />
                    }
                >
                    查看销售方案 {detail.proposal_no}
                </Button>
            ) : null}
            {hasMoreActions ? (
                <Popover open={moreOpen} onOpenChange={setMoreOpen}>
                    <PopoverTrigger
                        id="sales-selection-detail-more-actions"
                        render={
                            <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                className={detailWorkspaceStyles.quietButton}
                            />
                        }
                    >
                        更多
                        <ChevronDownIcon
                            aria-hidden="true"
                            data-icon="inline-end"
                        />
                    </PopoverTrigger>
                    <PopoverContent
                        align="end"
                        keepMounted
                        className="w-48 gap-1.5 p-2"
                    >
                        <PopoverTitle className="px-2 pt-1 text-xs font-medium text-muted-foreground">
                            选品册操作
                        </PopoverTitle>
                        <div className="flex flex-col gap-0.5">
                            {showCopy && onCopy ? (
                                <Button
                                    id="sales-selection-detail-copy"
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className={`w-full justify-start ${detailWorkspaceStyles.quietButton}`}
                                    disabled={
                                        pending || detail.status === "PREPARING"
                                    }
                                    title={
                                        detail.status === "PREPARING"
                                            ? "请准备完成后再复制"
                                            : undefined
                                    }
                                    onClick={() => runAndCloseMore(onCopy)}
                                >
                                    <CopyIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    复制选品册
                                </Button>
                            ) : null}
                            {showRegenerateAll ? (
                                <LoadingButton
                                    id="sales-selection-detail-regenerate"
                                    loading={
                                        operations.regenerate.isPending &&
                                        !operations.regenerate.variables
                                            ?.tier_ids?.length
                                    }
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className={`w-full justify-start ${detailWorkspaceStyles.quietButton}`}
                                    disabled={pending}
                                    onClick={() =>
                                        runAndCloseMore(onRegenerateAll)
                                    }
                                >
                                    <RefreshCwIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    重新生成整册
                                </LoadingButton>
                            ) : null}
                            {showReplaceLink ? (
                                <LoadingButton
                                    id="sales-selection-detail-replace-link"
                                    loading={operations.replaceLink.isPending}
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className={`w-full justify-start ${detailWorkspaceStyles.quietButton}`}
                                    disabled={pending}
                                    onClick={() =>
                                        runAndCloseMore(onReplaceLink)
                                    }
                                >
                                    <Link2Icon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    更换链接
                                </LoadingButton>
                            ) : null}
                            {showClose ? (
                                <LoadingButton
                                    id="sales-selection-detail-close"
                                    loading={operations.close.isPending}
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className={`w-full justify-start ${detailWorkspaceStyles.quietDangerButton}`}
                                    disabled={pending}
                                    onClick={() => runAndCloseMore(onClose)}
                                >
                                    <BanIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    关闭
                                </LoadingButton>
                            ) : null}
                            {showRevoke ? (
                                <LoadingButton
                                    id="sales-selection-detail-revoke"
                                    loading={operations.revoke.isPending}
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className={`w-full justify-start ${detailWorkspaceStyles.quietDangerButton}`}
                                    disabled={pending}
                                    onClick={() => runAndCloseMore(onRevoke)}
                                >
                                    <Link2OffIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    撤销链接访问
                                </LoadingButton>
                            ) : null}
                            {showVoid ? (
                                <LoadingButton
                                    id="sales-selection-detail-void"
                                    loading={operations.void.isPending}
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className={`w-full justify-start ${detailWorkspaceStyles.quietDangerButton}`}
                                    disabled={pending}
                                    onClick={() => runAndCloseMore(onVoid)}
                                >
                                    <BanIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    作废
                                </LoadingButton>
                            ) : null}
                        </div>
                    </PopoverContent>
                </Popover>
            ) : null}
        </>
    )

    return (
        <DetailWorkspaceChrome
            back={{
                id: "sales-selection-detail-back",
                label: "选品册列表",
                href: "/sales/selection",
            }}
            numberLabel="选品册"
            title={detail.customer_name}
            documentNumber={bookId}
            version={`v${detail.version}`}
            primaryStatus={{
                label: BOOKLET_STATUS_LABEL[detail.status],
                tone: bookletStatusTone(detail.status),
            }}
            titleExtra={
                <span className="inline-flex flex-wrap items-center gap-1.5">
                    <Badge variant="secondary">{formLabel}</Badge>
                </span>
            }
            meta={
                <DetailWorkspaceMeta
                    items={[
                        <span key="customer">
                            客户{" "}
                            <Link
                                id="sales-selection-detail-customer-link"
                                href={`/sales/customers/${encodeURIComponent(detail.customer_id)}`}
                                className="font-medium text-foreground underline-offset-4 hover:text-primary hover:underline"
                            >
                                {detail.customer_name}
                            </Link>
                        </span>,
                        <span key="owner">
                            负责人{" "}
                            <span className="font-medium text-foreground">
                                {detail.sales_owner_name || "未指定"}
                            </span>
                        </span>,
                        <span key="source">
                            来源{" "}
                            <span className="font-medium text-foreground">
                                {POOL_SOURCE_LABEL[detail.source_kind]}
                            </span>
                        </span>,
                    ]}
                />
            }
            primaryAction={primaryAction}
            secondaryActions={secondaryActions}
        />
    )
}
