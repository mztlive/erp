"use client"

import * as React from "react"
import Link from "next/link"
import {
    BarChart3Icon,
    FileTextIcon,
    LayersIcon,
    Link2Icon,
    RefreshCwIcon,
} from "lucide-react"

import { MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
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

export function BookDetailSidebar({
    detail,
    pending,
    operations,
}: {
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
        <aside aria-label="选品册详情与控制" className="flex flex-col gap-4">
            {/* 1. 客户选品链接与交付卡片 (已发布/已提交态高亮显示) */}
            {publicHref &&
            (detail.status === "PUBLISHED" ||
                detail.status === "SUBMITTED" ||
                operations.copyLink.data?.public_url) ? (
                <Card size="sm" className="border-primary/30 bg-card shadow-xs">
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
                                    detail.link_revoked ? "outline" : "default"
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
                                    disabled={
                                        operations.copyLink.isPending || pending
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

            {/* 2. 陈列统计指标卡 */}
            <Card size="sm" className="shadow-xs">
                <CardHeader className="pb-2">
                    <CardTitle className="flex items-center gap-1.5 text-sm font-semibold">
                        <BarChart3Icon
                            className="size-4 text-muted-foreground"
                            aria-hidden="true"
                        />
                        陈列指标
                    </CardTitle>
                </CardHeader>
                <CardContent className="pt-0">
                    <div className="grid grid-cols-2 gap-2">
                        <div className="rounded-lg border border-border/70 bg-muted/40 p-2.5 text-center">
                            <div className="text-2xs text-muted-foreground">
                                有效陈列
                            </div>
                            <div className="num mt-1 text-lg font-bold text-foreground">
                                {detail.display_count}
                            </div>
                        </div>
                        <div className="rounded-lg border border-border/70 bg-muted/40 p-2.5 text-center">
                            <div className="text-2xs text-muted-foreground">
                                已剔除项
                            </div>
                            <div className="num mt-1 text-lg font-bold text-muted-foreground">
                                {detail.removed_count}
                            </div>
                        </div>
                        <div className="rounded-lg border border-border/70 bg-muted/40 p-2.5 text-center">
                            <div className="text-2xs text-muted-foreground">
                                缺少图片
                            </div>
                            <div
                                className={`num mt-1 text-lg font-bold ${detail.missing_image_count > 0 ? "text-amber-600 dark:text-amber-400" : "text-foreground"}`}
                            >
                                {detail.missing_image_count}
                            </div>
                        </div>
                        <div className="rounded-lg border border-border/70 bg-muted/40 p-2.5 text-center">
                            <div className="text-2xs text-muted-foreground">
                                选品档位
                            </div>
                            <div className="num mt-1 text-lg font-bold text-foreground">
                                {isPackage
                                    ? `${detail.tiers.length} 档`
                                    : "单品"}
                            </div>
                        </div>
                    </div>
                </CardContent>
            </Card>

            {/* 3. 选品册档案卡片 */}
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
                        <div className="flex justify-between py-2 first:pt-0">
                            <dt className="text-muted-foreground">客户名称</dt>
                            <dd className="font-medium text-foreground text-right">
                                {detail.customer_name}
                            </dd>
                        </div>
                        <div className="flex justify-between py-2">
                            <dt className="text-muted-foreground">选品形态</dt>
                            <dd className="font-medium text-foreground">
                                {SELECTION_FORM_LABEL[detail.selection_form]}
                            </dd>
                        </div>
                        <div className="flex justify-between py-2">
                            <dt className="text-muted-foreground">提交方式</dt>
                            <dd className="font-medium text-foreground">
                                {SUBMIT_MODE_LABEL[detail.submit_mode]}
                            </dd>
                        </div>
                        <div className="flex justify-between py-2">
                            <dt className="text-muted-foreground">商品来源</dt>
                            <dd className="font-medium text-foreground">
                                {POOL_SOURCE_LABEL[detail.source_kind]}
                            </dd>
                        </div>
                        <div className="flex justify-between py-2">
                            <dt className="text-muted-foreground">
                                资格核对日
                            </dt>
                            <dd className="num font-medium text-foreground">
                                {formatBookInstant(detail.eligibility_as_of)}
                            </dd>
                        </div>
                        <div className="flex justify-between py-2">
                            <dt className="text-muted-foreground">
                                商品快照时间
                            </dt>
                            <dd className="num font-medium text-foreground text-right">
                                {formatBookInstant(detail.prepared_at)}
                            </dd>
                        </div>
                        <div className="flex justify-between py-2 last:pb-0">
                            <dt className="text-muted-foreground">更新时间</dt>
                            <dd className="num font-medium text-foreground text-right">
                                {formatBookInstant(detail.updated_at)}
                            </dd>
                        </div>
                    </dl>
                </CardContent>
            </Card>

            {/* 4. 档位规则与达成 (仅套餐形态) */}
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
                                    (value) => value.tier_id === tier.tier_id,
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
                                                    value={tier.target_amount}
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
                                                    className="h-4 px-1 text-3xs"
                                                >
                                                    {report.stop_label}
                                                </Badge>
                                            ) : null}
                                        </div>
                                        {report && report.image_failures > 0 ? (
                                            <p className="mt-0.5 text-3xs text-amber-600 dark:text-amber-400">
                                                图片失败 {report.image_failures}{" "}
                                                项
                                            </p>
                                        ) : null}
                                        {detail.status === "PENDING_PUBLISH" ? (
                                            <Button
                                                id={`selection-regenerate-${tier.tier_id}`}
                                                type="button"
                                                size="xs"
                                                variant="ghost"
                                                disabled={pending}
                                                className="mt-1.5 h-6 text-2xs text-primary hover:bg-primary/10"
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
                                            </Button>
                                        ) : null}
                                    </div>
                                )
                            })}
                        </div>
                    </CardContent>
                </Card>
            ) : null}
        </aside>
    )
}
