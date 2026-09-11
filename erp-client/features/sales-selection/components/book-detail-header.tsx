"use client"

import Link from "next/link"
import {
    BanIcon,
    CopyIcon,
    Link2Icon,
    Link2OffIcon,
    PencilIcon,
    RefreshCwIcon,
    SendIcon,
    SparklesIcon,
} from "lucide-react"

import { DetailPageHeader } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { useBookOperations } from "@/features/sales-selection/hooks/queries"
import {
    bookIdentity,
    bookletStatusTone,
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
    operations,
    onEdit,
}: {
    detail: SelectionBookDetail
    pending: boolean
    operations: Operations
    onEdit: () => void
}) {
    const bookId = bookIdentity(detail)
    const key = withKey(detail.version)
    const formLabel = SELECTION_FORM_LABEL[detail.selection_form]
    const modeLabel = SUBMIT_MODE_LABEL[detail.submit_mode]

    const primaryAction = (() => {
        if (detail.status === "DRAFT") {
            return (
                <Button
                    id="sales-selection-detail-prepare"
                    type="button"
                    size="sm"
                    disabled={pending}
                    onClick={() =>
                        void operations.prepare.mutateAsync({
                            bookId,
                            kind: "FIRST_PREPARE",
                            ...key,
                        })
                    }
                >
                    <SparklesIcon data-icon="inline-start" aria-hidden="true" />
                    开始准备
                </Button>
            )
        }
        if (detail.status === "PENDING_PUBLISH") {
            return (
                <Button
                    id="sales-selection-detail-publish"
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
                </Button>
            )
        }
        if (detail.status === "PUBLISHED") {
            return (
                <Button
                    id="sales-selection-detail-copy-link"
                    type="button"
                    size="sm"
                    disabled={operations.copyLink.isPending || pending}
                    onClick={() => void operations.copyLink.mutateAsync(bookId)}
                >
                    <CopyIcon data-icon="inline-start" aria-hidden="true" />
                    复制链接
                </Button>
            )
        }
        if (detail.status === "SUBMITTED" && detail.proposal_id) {
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
        <div className="flex flex-wrap items-center justify-end gap-2">
            {detail.status === "DRAFT" ? (
                <>
                    <Button
                        id="selection-draft-edit"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={pending}
                        onClick={onEdit}
                    >
                        <PencilIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        调整来源与档位
                    </Button>
                    <Button
                        id="sales-selection-detail-void"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={pending}
                        onClick={() =>
                            void operations.void.mutateAsync({ bookId, ...key })
                        }
                    >
                        <BanIcon data-icon="inline-start" aria-hidden="true" />
                        作废
                    </Button>
                </>
            ) : null}
            {detail.status === "PENDING_PUBLISH" ? (
                <>
                    {detail.selection_form === "PACKAGE" ? (
                        <Button
                            id="sales-selection-detail-regenerate"
                            type="button"
                            size="sm"
                            variant="outline"
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
                            整册重生成
                        </Button>
                    ) : null}
                    <Button
                        id="sales-selection-detail-reprepare"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={pending}
                        onClick={onEdit}
                    >
                        <PencilIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        调整来源与档位
                    </Button>
                    <Button
                        id="sales-selection-detail-void"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={pending}
                        onClick={() =>
                            void operations.void.mutateAsync({ bookId, ...key })
                        }
                    >
                        <BanIcon data-icon="inline-start" aria-hidden="true" />
                        作废
                    </Button>
                </>
            ) : null}
            {detail.status === "PUBLISHED" ? (
                <>
                    <Button
                        id="sales-selection-detail-replace-link"
                        type="button"
                        size="sm"
                        variant="outline"
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
                    </Button>
                    <Button
                        id="sales-selection-detail-close"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={pending}
                        onClick={() =>
                            void operations.close.mutateAsync({
                                bookId,
                                ...key,
                            })
                        }
                    >
                        关闭
                    </Button>
                </>
            ) : null}
            {detail.status === "SUBMITTED" ? (
                <Button
                    id="sales-selection-detail-revoke"
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={pending}
                    onClick={() =>
                        void operations.revoke.mutateAsync({ bookId, ...key })
                    }
                >
                    <Link2OffIcon data-icon="inline-start" aria-hidden="true" />
                    撤销链接访问
                </Button>
            ) : null}
            {detail.proposal_id &&
            detail.proposal_no &&
            detail.status !== "SUBMITTED" ? (
                <Button
                    id="sales-selection-detail-open-proposal"
                    type="button"
                    size="sm"
                    variant="outline"
                    render={
                        <Link
                            href={`/sales/selection/proposals/${detail.proposal_id}`}
                        />
                    }
                >
                    查看销售方案 {detail.proposal_no}
                </Button>
            ) : null}
        </div>
    )

    const hasSecondary =
        detail.status === "DRAFT" ||
        detail.status === "PENDING_PUBLISH" ||
        detail.status === "PUBLISHED" ||
        detail.status === "SUBMITTED" ||
        Boolean(detail.proposal_id)

    return (
        <DetailPageHeader
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
            secondaryActions={hasSecondary ? secondaryActions : undefined}
        />
    )
}
