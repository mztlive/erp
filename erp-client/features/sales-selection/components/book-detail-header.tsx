"use client"

import Link from "next/link"
import { hasPermission } from "@/lib/permissions"
import {
    BanIcon,
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
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { useBookOperations } from "@/features/sales-selection/hooks/queries"
import {
    bookIdentity,
    bookletStatusTone,
    prepareFailure,
} from "@/features/sales-selection/lib/presentation"
import { createIdempotencyKey } from "@/features/sales-selection/lib/validation"
import type { SelectionBookDetail } from "@/features/sales-selection/types"
import {
    BOOKLET_STATUS_LABEL,
    SELECTION_FORM_LABEL,
    SUBMIT_MODE_LABEL,
} from "@/features/sales-selection/types"

type Operations = ReturnType<typeof useBookOperations>

function withKey(version: number) {
    return {
        expected_version: version,
        idempotency_key: createIdempotencyKey(),
    }
}

/**
 * 选品册详情页头：返回列表、对象身份与当前状态可执行动作。
 */
export function BookDetailHeader({
    detail,
    pending,
    permissions,
    operations,
    onEdit,
    onCopy,
    onOpenInfo,
}: {
    detail: SelectionBookDetail
    pending: boolean
    permissions: readonly string[] | undefined
    operations: Operations
    onEdit: () => void
    onCopy?: () => void
    onOpenInfo: () => void
}) {
    const bookId = bookIdentity(detail)
    const key = withKey(detail.version)
    const formLabel = SELECTION_FORM_LABEL[detail.selection_form]
    const modeLabel = SUBMIT_MODE_LABEL[detail.submit_mode]
    const failure = prepareFailure(detail)

    const can = (action: string) =>
        hasPermission(permissions, `sales_selection_booklet:${action}`)
    const canOpenProposal = hasPermission(
        permissions,
        "sales_selection_proposal:get",
    )
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
                        operations.prepare.mutate({
                            bookId,
                            kind: "FIRST_PREPARE",
                            ...key,
                        })
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
                    onClick={() =>
                        void operations.publish.mutateAsync({
                            bookId,
                            batch_id: detail.batch_id ?? undefined,
                            ...key,
                        })
                    }
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
                            href={`/sales/selection/proposals/${detail.proposal_id}`}
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
            {onCopy ? (
                <Button
                    id="sales-selection-detail-copy"
                    type="button"
                    size="sm"
                    variant="ghost"
                    className={detailWorkspaceStyles.quietButton}
                    disabled={pending || detail.status === "PREPARING"}
                    title={
                        detail.status === "PREPARING"
                            ? "请准备完成后再复制"
                            : undefined
                    }
                    onClick={onCopy}
                >
                    <CopyIcon data-icon="inline-start" aria-hidden="true" />
                    复制选品册
                </Button>
            ) : null}
            {detail.status === "DRAFT" ? (
                <>
                    {can("prepare") && (
                        <Button
                            id="selection-draft-edit"
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietButton}
                            disabled={pending}
                            onClick={onEdit}
                        >
                            <PencilIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            调整来源与档位
                        </Button>
                    )}
                    {can("void") && (
                        <LoadingButton
                            id="sales-selection-detail-void"
                            loading={operations.void.isPending}
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietDangerButton}
                            disabled={pending}
                            onClick={() =>
                                void operations.void.mutateAsync({
                                    bookId,
                                    ...key,
                                })
                            }
                        >
                            <BanIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            作废
                        </LoadingButton>
                    )}
                </>
            ) : null}
            {detail.status === "PENDING_PUBLISH" ? (
                <>
                    {detail.selection_form === "PACKAGE" && can("prepare") ? (
                        <LoadingButton
                            id="sales-selection-detail-regenerate"
                            loading={
                                operations.regenerate.isPending &&
                                !operations.regenerate.variables?.tier_ids
                                    ?.length
                            }
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietButton}
                            disabled={pending}
                            onClick={() =>
                                void operations.regenerate.mutateAsync({
                                    bookId,
                                    ...key,
                                })
                            }
                        >
                            <RefreshCwIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            重新生成整册
                        </LoadingButton>
                    ) : null}
                    {can("prepare") && (
                        <Button
                            id="sales-selection-detail-reprepare"
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietButton}
                            disabled={pending}
                            onClick={onEdit}
                        >
                            <PencilIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            调整来源与档位
                        </Button>
                    )}
                    {can("void") && (
                        <LoadingButton
                            id="sales-selection-detail-void"
                            loading={operations.void.isPending}
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietButton}
                            disabled={pending}
                            onClick={() =>
                                void operations.void.mutateAsync({
                                    bookId,
                                    ...key,
                                })
                            }
                        >
                            <BanIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            作废
                        </LoadingButton>
                    )}
                </>
            ) : null}
            {detail.status === "PUBLISHED" ? (
                <>
                    {can("rotate_link") && (
                        <LoadingButton
                            id="sales-selection-detail-replace-link"
                            loading={operations.replaceLink.isPending}
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietButton}
                            disabled={pending}
                            onClick={() =>
                                void operations.replaceLink.mutateAsync({
                                    bookId,
                                    ...key,
                                })
                            }
                        >
                            <Link2Icon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            更换链接
                        </LoadingButton>
                    )}
                    {can("close") && (
                        <LoadingButton
                            id="sales-selection-detail-close"
                            loading={operations.close.isPending}
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietDangerButton}
                            disabled={pending}
                            onClick={() =>
                                void operations.close.mutateAsync({
                                    bookId,
                                    ...key,
                                })
                            }
                        >
                            关闭
                        </LoadingButton>
                    )}
                </>
            ) : null}
            {detail.status === "SUBMITTED" && can("revoke") ? (
                <LoadingButton
                    id="sales-selection-detail-revoke"
                    loading={operations.revoke.isPending}
                    type="button"
                    size="sm"
                    variant="ghost"
                    className={detailWorkspaceStyles.quietDangerButton}
                    disabled={pending}
                    onClick={() =>
                        void operations.revoke.mutateAsync({ bookId, ...key })
                    }
                >
                    <Link2OffIcon data-icon="inline-start" aria-hidden="true" />
                    撤销链接访问
                </LoadingButton>
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
                            href={`/sales/selection/proposals/${detail.proposal_id}`}
                        />
                    }
                >
                    查看销售方案 {detail.proposal_no}
                </Button>
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
                    <Badge variant="secondary">{modeLabel}</Badge>
                </span>
            }
            primaryAction={primaryAction}
            secondaryActions={secondaryActions}
        />
    )
}
