"use client"

import Link from "next/link"
import { RefreshCwIcon } from "lucide-react"

import {
    BusinessStatusBadge,
    MoneyValue,
    QuickPreviewSheet,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupButton,
    InputGroupInput,
} from "@/components/ui/input-group"
import { useBookOperations } from "@/features/sales-selection/hooks/queries"
import {
    bookIdentity,
    bookletStatusTone,
    formatBookInstant,
    publicSelectionHref,
} from "@/features/sales-selection/lib/presentation"
import { createIdempotencyKey } from "@/features/sales-selection/lib/validation"
import type { SelectionBookDetail } from "@/features/sales-selection/types"
import {
    BOOKLET_STATUS_LABEL,
    POOL_SOURCE_LABEL,
    SELECTION_FORM_LABEL,
    SUBMIT_MODE_LABEL,
} from "@/features/sales-selection/types"

type Operations = ReturnType<typeof useBookOperations>

/** 按需查看选品册档案、客户链接和套餐档位达成情况。 */
export function BookDetailInfoSheet({
    open,
    onOpenChange,
    detail,
    pending,
    operations,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    detail: SelectionBookDetail
    pending: boolean
    operations: Operations
}) {
    const bookId = bookIdentity(detail)
    const publicHref = publicSelectionHref(
        operations.copyLink.data?.public_url ?? detail.public_url,
    )
    const isPackage = detail.selection_form === "PACKAGE"
    const archiveFields = [
        ["商品来源", POOL_SOURCE_LABEL[detail.source_kind]],
        ["资格核对日", formatBookInstant(detail.eligibility_as_of)],
        ["清单生成时间", formatBookInstant(detail.prepared_at)],
        ["更新时间", formatBookInstant(detail.updated_at)],
    ]

    return (
        <QuickPreviewSheet
            idPrefix="sales-selection-detail-info-sheet"
            open={open}
            onOpenChange={onOpenChange}
            size="preview"
            title={detail.customer_name}
            description={`选品册资料 · ${SELECTION_FORM_LABEL[detail.selection_form]} · ${SUBMIT_MODE_LABEL[detail.submit_mode]}`}
            summary={
                <div className="flex flex-wrap items-center gap-2">
                    <BusinessStatusBadge
                        context="preview"
                        label={BOOKLET_STATUS_LABEL[detail.status]}
                        tone={bookletStatusTone(detail.status)}
                    />
                    <span className="num text-xs text-muted-foreground">
                        版本 v{detail.version}
                    </span>
                </div>
            }
            finalFocus={() =>
                document.getElementById("sales-selection-detail-info")
            }
            footer={
                <Button
                    id="sales-selection-detail-info-dismiss"
                    type="button"
                    variant="outline"
                    onClick={() => onOpenChange(false)}
                >
                    关闭
                </Button>
            }
        >
            <section className="space-y-4">
                <h3 className="font-medium">选品册档案</h3>
                <dl className="space-y-3 text-body-compact">
                    {archiveFields.map(([label, value]) => (
                        <div
                            key={label}
                            className="flex min-w-0 items-baseline justify-between gap-5"
                        >
                            <dt className="shrink-0 text-xs text-muted-foreground">
                                {label}
                            </dt>
                            <dd className="num min-w-0 break-words text-right">
                                {value}
                            </dd>
                        </div>
                    ))}
                </dl>
            </section>

            {publicHref &&
            (detail.status === "PUBLISHED" ||
                detail.status === "SUBMITTED" ||
                operations.copyLink.data?.public_url) ? (
                <section className="space-y-3 border-t border-border pt-6">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                        <h3 className="font-medium">客户选品链接</h3>
                        <Badge
                            variant={
                                detail.link_revoked ? "outline" : "default"
                            }
                        >
                            {detail.link_revoked ? "已撤销" : "有效（30天）"}
                        </Badge>
                    </div>
                    <p className="text-xs text-muted-foreground">
                        客户在手机端选品，提交后生成销售方案。
                    </p>
                    <InputGroup>
                        <InputGroupInput
                            id="selection-copy-link-value"
                            aria-label="客户选品链接"
                            readOnly
                            value={publicHref}
                            onFocus={(event) => event.target.select()}
                            className="min-w-0 text-xs"
                        />
                        <InputGroupAddon align="inline-end">
                            <InputGroupButton
                                id="sales-selection-detail-copy-link-inline"
                                loading={operations.copyLink.isPending}
                                disabled={
                                    operations.copyLink.isPending || pending
                                }
                                onClick={() =>
                                    void operations.copyLink.mutateAsync(bookId)
                                }
                            >
                                复制
                            </InputGroupButton>
                        </InputGroupAddon>
                    </InputGroup>

                    {detail.proposal_id ? (
                        <Button
                            id="sales-selection-sidebar-open-proposal"
                            type="button"
                            size="sm"
                            variant="outline"
                            className="h-auto max-w-full whitespace-normal break-words text-left"
                            render={
                                <Link
                                    href={`/sales/selection/proposals/${detail.proposal_id}`}
                                />
                            }
                        >
                            查看销售方案
                            {detail.proposal_no
                                ? ` (${detail.proposal_no})`
                                : ""}
                        </Button>
                    ) : null}
                </section>
            ) : null}

            {isPackage && detail.tiers.length > 0 ? (
                <section className="space-y-4 border-t border-border pt-6">
                    <div className="flex flex-wrap items-baseline justify-between gap-2">
                        <h3 className="font-medium">档位目标与达成</h3>
                        <span className="text-xs text-muted-foreground">
                            共 {detail.tiers.length} 档
                        </span>
                    </div>
                    <div className="divide-y divide-border/60">
                        {detail.tiers.map((tier) => {
                            const report = detail.tier_reports.find(
                                (value) => value.tier_id === tier.tier_id,
                            )
                            return (
                                <div
                                    key={tier.tier_id}
                                    className="space-y-2 py-4 first:pt-0 last:pb-0"
                                >
                                    <div className="flex flex-wrap items-start justify-between gap-3">
                                        <h4 className="min-w-0 break-words text-body-compact font-medium">
                                            {tier.name}
                                        </h4>
                                        {report?.stop_label ? (
                                            <Badge
                                                variant="outline"
                                                className="max-w-full shrink-0 whitespace-normal"
                                            >
                                                {report.stop_label}
                                            </Badge>
                                        ) : null}
                                    </div>
                                    <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1 text-xs">
                                        <span className="text-muted-foreground">
                                            目标金额
                                        </span>
                                        <MoneyValue
                                            value={tier.target_amount}
                                        />
                                        <span className="num text-muted-foreground">
                                            ±¥{tier.tolerance}
                                        </span>
                                    </div>
                                    <p className="text-xs text-muted-foreground">
                                        每套 {tier.sku_count} 件 ·{" "}
                                        {report
                                            ? `达成 ${report.actual_count}/${report.expected_count} 套`
                                            : `计划 ${tier.expected_count} 套`}
                                    </p>
                                    {report && report.image_failures > 0 ? (
                                        <p className="text-xs text-warning-soft-foreground">
                                            图片失败 {report.image_failures} 项
                                        </p>
                                    ) : null}
                                    {detail.status === "PENDING_PUBLISH" ? (
                                        <LoadingButton
                                            id={`selection-regenerate-${toAutomationIdSegment(tier.tier_id)}`}
                                            loading={
                                                operations.regenerate
                                                    .isPending &&
                                                operations.regenerate.variables?.tier_ids?.includes(
                                                    tier.tier_id,
                                                ) === true
                                            }
                                            type="button"
                                            size="xs"
                                            variant="ghost"
                                            disabled={pending}
                                            onClick={() =>
                                                operations.regenerate.mutate({
                                                    bookId,
                                                    tier_ids: [tier.tier_id],
                                                    expected_version:
                                                        detail.version,
                                                    idempotency_key:
                                                        createIdempotencyKey(),
                                                })
                                            }
                                        >
                                            <RefreshCwIcon aria-hidden="true" />
                                            重新生成此档
                                        </LoadingButton>
                                    ) : null}
                                </div>
                            )
                        })}
                    </div>
                </section>
            ) : null}
        </QuickPreviewSheet>
    )
}
