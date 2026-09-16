"use client"

import { CircleAlertIcon, LoaderCircleIcon } from "lucide-react"

import {
    DocumentSection,
    DocumentSummary,
    MetricItem,
    MetricStrip,
    MoneyValue,
    surfaceInsetClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupButton,
    InputGroupInput,
} from "@/components/ui/input-group"
import {
    Progress,
    ProgressLabel,
    ProgressValue,
} from "@/components/ui/progress"
import { useBookOperations } from "@/features/sales-selection/hooks/queries"
import {
    bookIdentity,
    formatBookInstant,
    publicSelectionHref,
} from "@/features/sales-selection/lib/presentation"
import { createIdempotencyKey } from "@/features/sales-selection/lib/validation"
import type { SelectionBookDetail } from "@/features/sales-selection/types"
import { POOL_SOURCE_LABEL } from "@/features/sales-selection/types"
import { cn } from "@/lib/utils"

const STAGE_LABELS: Record<string, string> = {
    QUEUED: "排队",
    SNAPSHOT: "冻结商品池",
    SEARCH: "生成套餐",
    IMAGES: "准备图片",
    WRITE: "保存结果",
}

type Operations = ReturnType<typeof useBookOperations>

/**
 * 选品册概览：准备提示、指标、来源摘要、档位结果与公开链接。
 */
export function BookDetailOverview({
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
    const tierTotal = detail.tiers.length
    const completedTiers = detail.completed_tier_count ?? 0
    const preparePercent =
        detail.status === "PREPARING" && tierTotal > 0
            ? Math.min(100, Math.round((completedTiers / tierTotal) * 100))
            : undefined

    return (
        <>
            <div className="flex flex-col gap-4 pt-4">
                {detail.last_prepare_failure ? (
                    <Alert variant="destructive">
                        <CircleAlertIcon aria-hidden="true" />
                        <AlertTitle>最近一次准备未完成</AlertTitle>
                        <AlertDescription>
                            {detail.last_prepare_failure}
                        </AlertDescription>
                    </Alert>
                ) : null}

                {detail.status === "PREPARING" ? (
                    <Alert variant="info">
                        <LoaderCircleIcon
                            className="animate-spin"
                            aria-hidden="true"
                        />
                        <AlertTitle>正在准备陈列</AlertTitle>
                        <AlertDescription className="space-y-3">
                            <p>
                                当前步骤：
                                {STAGE_LABELS[
                                    detail.prepare_stage ?? "QUEUED"
                                ] ?? "处理商品"}
                                。准备期间不可编辑与发布。
                            </p>
                            {preparePercent != null ? (
                                <Progress value={preparePercent}>
                                    <ProgressLabel>档位进度</ProgressLabel>
                                    <ProgressValue>
                                        {() => (
                                            <span className="num">
                                                {completedTiers}/{tierTotal}
                                            </span>
                                        )}
                                    </ProgressValue>
                                </Progress>
                            ) : null}
                        </AlertDescription>
                    </Alert>
                ) : null}

                {detail.status === "PENDING_PUBLISH" &&
                detail.selection_form === "PACKAGE" ? (
                    <Alert variant="info">
                        <AlertTitle>发布前可继续调整</AlertTitle>
                        <AlertDescription>
                            重生成将重置目标档位的删减；其他档位保持当前选择。调整商品池或规则请使用「调整来源与档位」。
                        </AlertDescription>
                    </Alert>
                ) : null}

                {detail.status === "CLOSED" ||
                detail.status === "VOIDED" ||
                detail.status === "SUBMITTED" ? (
                    <Alert>
                        <AlertTitle>该选品册已结束操作</AlertTitle>
                        <AlertDescription>
                            {detail.status === "SUBMITTED"
                                ? "客户已提交选品，方案已生成。可查看方案或撤销公开链接访问。"
                                : "该选品册为业务终态，仅可查看，不可再操作。"}
                        </AlertDescription>
                    </Alert>
                ) : null}

                <MetricStrip
                    columns={4}
                    density="compact"
                    aria-label="陈列摘要"
                >
                    <MetricItem
                        density="compact"
                        label="有效陈列"
                        value={String(detail.display_count)}
                        detailMode="none"
                    />
                    <MetricItem
                        density="compact"
                        label="已删除"
                        value={String(detail.removed_count)}
                        detailMode="none"
                    />
                    <MetricItem
                        density="compact"
                        label="缺少图片"
                        value={String(detail.missing_image_count)}
                        detail={
                            detail.missing_image_count > 0
                                ? "发布后客户页仍可浏览，缺图项会标明"
                                : undefined
                        }
                        detailMode={
                            detail.missing_image_count > 0 ? "inline" : "none"
                        }
                    />
                    <MetricItem
                        density="compact"
                        label="档位"
                        value={
                            tierTotal > 0
                                ? String(tierTotal)
                                : detail.selection_form === "PACKAGE"
                                  ? "—"
                                  : "单品"
                        }
                        detailMode="none"
                    />
                </MetricStrip>
            </div>

            <DocumentSummary
                columns="two"
                items={[
                    {
                        id: "owner",
                        label: "负责销售",
                        value:
                            detail.sales_owner_name ||
                            detail.sales_owner_user_id,
                    },
                    {
                        id: "org",
                        label: "业务组织",
                        value: detail.business_org_unit_id,
                    },
                    {
                        id: "source",
                        label: "商品来源",
                        value: POOL_SOURCE_LABEL[detail.source_kind],
                    },
                    {
                        id: "eligibility",
                        label: "资格核对日",
                        value: formatBookInstant(detail.eligibility_as_of),
                    },
                    {
                        id: "prepared",
                        label: "商品快照时间",
                        value: formatBookInstant(detail.prepared_at),
                        numeric: true,
                    },
                    {
                        id: "updated",
                        label: "更新时间",
                        value: formatBookInstant(detail.updated_at),
                        numeric: true,
                    },
                ]}
            />

            {publicHref &&
            (detail.status === "PUBLISHED" ||
                operations.copyLink.data?.public_url) ? (
                <div className={cn(surfaceInsetClassName, "space-y-2 p-4")}>
                    <p className="text-xs text-muted-foreground">
                        客户选品链接
                    </p>
                    <InputGroup>
                        <InputGroupInput
                            id="selection-copy-link-value"
                            readOnly
                            value={publicHref}
                            onFocus={(event) => event.target.select()}
                        />
                        <InputGroupAddon align="inline-end">
                            <InputGroupButton
                                id="sales-selection-detail-copy-link-inline"
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
                </div>
            ) : null}

            {detail.tiers.length > 0 ? (
                <DocumentSection
                    title="档位准备"
                    description={
                        detail.selection_form === "PACKAGE"
                            ? "每个档位按目标金额生成套餐。待发布时可单独重生成。"
                            : "单品选品不按档生成套餐。"
                    }
                >
                    <ul className="divide-y divide-border">
                        {detail.tiers.map((tier) => {
                            const report = detail.tier_reports.find(
                                (value) => value.tier_id === tier.tier_id,
                            )
                            return (
                                <li
                                    key={tier.tier_id}
                                    className="flex flex-wrap items-start justify-between gap-3 py-3 first:pt-0 last:pb-0"
                                >
                                    <div className="min-w-0 space-y-1">
                                        <p className="font-medium">
                                            {tier.name}
                                        </p>
                                        <p className="flex flex-wrap items-center gap-x-2 text-sm text-muted-foreground">
                                            <span>目标</span>
                                            <MoneyValue
                                                value={tier.target_amount}
                                            />
                                            <span>
                                                ±{" "}
                                                <span className="num">
                                                    ¥{tier.tolerance}
                                                </span>
                                            </span>
                                            <span aria-hidden="true">·</span>
                                            <span>
                                                每套{" "}
                                                <span className="num">
                                                    {tier.sku_count}
                                                </span>{" "}
                                                件
                                            </span>
                                        </p>
                                        <p className="text-sm text-muted-foreground">
                                            {report
                                                ? `已生成 ${report.actual_count}/${report.expected_count} 套 · ${report.stop_label}${
                                                      report.image_failures > 0
                                                          ? ` · 图片失败 ${report.image_failures} 项`
                                                          : ""
                                                  }`
                                                : `计划生成 ${tier.expected_count} 套`}
                                        </p>
                                    </div>
                                    {detail.status === "PENDING_PUBLISH" ? (
                                        <Button
                                            id={`selection-regenerate-${tier.tier_id}`}
                                            type="button"
                                            size="sm"
                                            variant="outline"
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
                                            重生成此档
                                        </Button>
                                    ) : null}
                                </li>
                            )
                        })}
                    </ul>
                </DocumentSection>
            ) : null}
        </>
    )
}
