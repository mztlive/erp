"use client"

import Link from "next/link"
import {
    FileTextIcon,
    LayersIcon,
    Link2Icon,
    RefreshCwIcon,
} from "lucide-react"

import { MoneyValue, QuickPreviewSheet } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupButton,
    InputGroupInput,
} from "@/components/ui/input-group"
import { useBookOperations } from "@/features/sales-selection/hooks/queries"
import {
    bookIdentity,
    formatBookInstant,
    publicSelectionHref,
} from "@/features/sales-selection/lib/presentation"
import { createIdempotencyKey } from "@/features/sales-selection/lib/validation"
import type { SelectionBookDetail } from "@/features/sales-selection/types"
import {
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

    return (
        <QuickPreviewSheet
            idPrefix="sales-selection-detail-info-sheet"
            open={open}
            onOpenChange={onOpenChange}
            size="detail"
            title={detail.customer_name}
            description="选品册资料、客户链接与套餐档位达成情况。"
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
            <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-7 py-6">
                {/* 客户选品链接与交付 */}
                {publicHref &&
                (detail.status === "PUBLISHED" ||
                    detail.status === "SUBMITTED" ||
                    operations.copyLink.data?.public_url) ? (
                    <Card
                        size="sm"
                        className="border-primary/30 bg-card shadow-xs"
                    >
                        <CardHeader className="pb-2">
                            <div className="flex items-center justify-between">
                                <CardTitle className="flex items-center gap-1.5 text-sm font-semibold">
                                    <Link2Icon
                                        className="size-4 text-primary"
                                        aria-hidden="true"
                                    />
                                    客户选品链接
                                </CardTitle>
                                <Badge
                                    variant={
                                        detail.link_revoked
                                            ? "outline"
                                            : "default"
                                    }
                                    className="text-2xs"
                                >
                                    {detail.link_revoked
                                        ? "已撤销"
                                        : "有效（30天）"}
                                </Badge>
                            </div>
                            <CardDescription className="text-xs">
                                客户移动端专属选品地址，提交后生成正式方案
                            </CardDescription>
                        </CardHeader>
                        <CardContent className="space-y-3 pt-0">
                            <InputGroup>
                                <InputGroupInput
                                    id="selection-copy-link-value"
                                    readOnly
                                    value={publicHref}
                                    onFocus={(event) => event.target.select()}
                                    className="text-xs"
                                />
                                <InputGroupAddon align="inline-end">
                                    <InputGroupButton
                                        id="sales-selection-detail-copy-link-inline"
                                        loading={operations.copyLink.isPending}
                                        disabled={
                                            operations.copyLink.isPending ||
                                            pending
                                        }
                                        onClick={() =>
                                            void operations.copyLink.mutateAsync(
                                                bookId,
                                            )
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
                                    className="w-full text-xs"
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
                        </CardContent>
                    </Card>
                ) : null}

                {/* 选品册档案 */}
                <Card size="sm" className="shadow-xs">
                    <CardHeader className="pb-2">
                        <CardTitle className="flex items-center gap-1.5 text-sm font-semibold">
                            <FileTextIcon
                                className="size-4 text-muted-foreground"
                                aria-hidden="true"
                            />
                            选品册档案
                        </CardTitle>
                    </CardHeader>
                    <CardContent className="pt-0">
                        <dl className="divide-y divide-border/60 text-xs">
                            <div className="flex justify-between gap-4 py-2">
                                <dt className="text-muted-foreground">
                                    选品形态
                                </dt>
                                <dd className="font-medium text-foreground">
                                    {
                                        SELECTION_FORM_LABEL[
                                            detail.selection_form
                                        ]
                                    }
                                </dd>
                            </div>
                            <div className="flex justify-between gap-4 py-2">
                                <dt className="text-muted-foreground">
                                    提交方式
                                </dt>
                                <dd className="font-medium text-foreground">
                                    {SUBMIT_MODE_LABEL[detail.submit_mode]}
                                </dd>
                            </div>
                            <div className="flex justify-between gap-4 py-2">
                                <dt className="text-muted-foreground">
                                    商品来源
                                </dt>
                                <dd className="font-medium text-foreground">
                                    {POOL_SOURCE_LABEL[detail.source_kind]}
                                </dd>
                            </div>
                            <div className="flex justify-between gap-4 py-2">
                                <dt className="text-muted-foreground">
                                    资格核对日
                                </dt>
                                <dd className="num font-medium text-foreground">
                                    {formatBookInstant(
                                        detail.eligibility_as_of,
                                    )}
                                </dd>
                            </div>
                            <div className="flex justify-between gap-4 py-2">
                                <dt className="text-muted-foreground">
                                    商品快照时间
                                </dt>
                                <dd className="num font-medium text-foreground text-right">
                                    {formatBookInstant(detail.prepared_at)}
                                </dd>
                            </div>
                            <div className="flex justify-between gap-4 py-2 last:pb-0">
                                <dt className="text-muted-foreground">
                                    更新时间
                                </dt>
                                <dd className="num font-medium text-foreground text-right">
                                    {formatBookInstant(detail.updated_at)}
                                </dd>
                            </div>
                        </dl>
                    </CardContent>
                </Card>

                {/* 套餐档位规则与达成 */}
                {isPackage && detail.tiers.length > 0 ? (
                    <Card size="sm" className="shadow-xs">
                        <CardHeader className="pb-2">
                            <div className="flex items-center justify-between">
                                <CardTitle className="flex items-center gap-1.5 text-sm font-semibold">
                                    <LayersIcon
                                        className="size-4 text-muted-foreground"
                                        aria-hidden="true"
                                    />
                                    档位目标与达成
                                </CardTitle>
                                <Badge variant="secondary" className="text-2xs">
                                    共 {detail.tiers.length} 档
                                </Badge>
                            </div>
                        </CardHeader>
                        <CardContent className="space-y-3 pt-0">
                            <div className="divide-y divide-border/60">
                                {detail.tiers.map((tier) => {
                                    const report = detail.tier_reports.find(
                                        (value) =>
                                            value.tier_id === tier.tier_id,
                                    )
                                    return (
                                        <div
                                            key={tier.tier_id}
                                            className="py-2.5 first:pt-0 last:pb-0"
                                        >
                                            <div className="flex items-center justify-between">
                                                <span className="font-medium text-foreground text-xs">
                                                    {tier.name}
                                                </span>
                                                <span className="flex items-center gap-1 text-2xs text-muted-foreground">
                                                    <span>目标</span>
                                                    <MoneyValue
                                                        value={
                                                            tier.target_amount
                                                        }
                                                        className="font-medium"
                                                    />
                                                    <span>
                                                        (±¥{tier.tolerance})
                                                    </span>
                                                </span>
                                            </div>
                                            <div className="mt-1 flex items-center justify-between text-2xs text-muted-foreground">
                                                <span>
                                                    每套 {tier.sku_count} 件 ·{" "}
                                                    {report
                                                        ? `达成 ${report.actual_count}/${report.expected_count} 套`
                                                        : `计划 ${tier.expected_count} 套`}
                                                </span>
                                                {report?.stop_label ? (
                                                    <Badge
                                                        variant="outline"
                                                        className="h-4 px-1 text-2xs"
                                                    >
                                                        {report.stop_label}
                                                    </Badge>
                                                ) : null}
                                            </div>
                                            {report &&
                                            report.image_failures > 0 ? (
                                                <p className="mt-0.5 text-2xs text-warning-soft-foreground">
                                                    图片失败{" "}
                                                    {report.image_failures} 项
                                                </p>
                                            ) : null}
                                            {detail.status ===
                                            "PENDING_PUBLISH" ? (
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
                                                    className="mt-1.5"
                                                    onClick={() =>
                                                        operations.regenerate.mutate(
                                                            {
                                                                bookId,
                                                                tier_ids: [
                                                                    tier.tier_id,
                                                                ],
                                                                expected_version:
                                                                    detail.version,
                                                                idempotency_key:
                                                                    createIdempotencyKey(),
                                                            },
                                                        )
                                                    }
                                                >
                                                    <RefreshCwIcon
                                                        className="mr-1 size-3"
                                                        aria-hidden="true"
                                                    />
                                                    重生成此档
                                                </LoadingButton>
                                            ) : null}
                                        </div>
                                    )
                                })}
                            </div>
                        </CardContent>
                    </Card>
                ) : null}
            </div>
        </QuickPreviewSheet>
    )
}
