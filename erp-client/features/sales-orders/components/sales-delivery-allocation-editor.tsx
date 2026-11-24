"use client"

import { ListChecksIcon, MapPinIcon, TriangleAlertIcon } from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { RegionName } from "@/features/regions"
import type { DeliveryCoverageHint as CoverageCode } from "../api/delivery-coverage"
import { summarizeOrderDelivery } from "../lib/sales-delivery-pool"
import type { SalesLineFilterKey } from "../lib/sales-order-create-line-health"
import type { SalesOrderDraftLineInput } from "../types"

// 改单页面继续复用相同的数量与完整性摘要。
export { allocationSummary } from "../lib/sales-delivery-allocation-summary"

/**
 * 紧凑单行收货状态摘要条：
 * - 非询价且尚未填写任何地址时隐藏，避免 0 地址空态与分区工具栏重复；
 * - 已配置地址（或询价锁定）时呈现在线摘要，提供地址问题快捷跳转与地址总览入口。
 */
export function SalesDeliverySummaryBar({
    lines,
    inquiryLocked = false,
    coverageHint,
    onOpenOverview,
    onEditUnifiedAddress,
    onFilterLines,
}: {
    lines: SalesOrderDraftLineInput[]
    inquiryLocked?: boolean
    coverageHint?: (sku: string, regionCode: string) => CoverageCode
    onOpenOverview: () => void
    onEditUnifiedAddress?: () => void
    onFilterLines?: (filter: SalesLineFilterKey) => void
}) {
    if (lines.length === 0) return null

    const overview = summarizeOrderDelivery(lines)
    if (!inquiryLocked && overview.allocatedLineCount === 0) return null

    const common = overview.unified

    const coverageWarningCount =
        coverageHint && !inquiryLocked
            ? lines.filter((line) =>
                  (line.deliveryAllocations ?? []).some(
                      (row) =>
                          row.region_code &&
                          coverageHint(line.sku, row.region_code) !== "NONE",
                  ),
              ).length
            : 0

    return (
        <div
            id="sales-delivery-summary-bar"
            role="region"
            aria-label="收货安排"
            className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 rounded-lg border border-border/70 bg-surface-sunken/70 px-3.5 py-2"
        >
            <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2.5 gap-y-1">
                <MapPinIcon
                    className="size-4 shrink-0 text-muted-foreground"
                    aria-hidden="true"
                />
                <span className="shrink-0 text-xs font-medium text-foreground/80">
                    {inquiryLocked ? "询价配送" : "收货安排"}
                </span>

                {common ? (
                    <>
                        <StatusBadge
                            tone="success"
                            appearance="quiet"
                            label="统一地址"
                        />
                        <span
                            className="min-w-0 truncate text-body-compact text-foreground"
                            title={`${common.recipient} ${common.phone} ${common.address}`}
                        >
                            <span className="font-medium">
                                {common.recipient}
                            </span>
                            <span className="num ml-1.5 text-xs text-muted-foreground">
                                {common.phone}
                            </span>
                            <span className="ml-2 text-xs text-muted-foreground">
                                <RegionName code={common.region_code} />
                                {common.address}
                            </span>
                        </span>
                    </>
                ) : overview.allocatedLineCount > 0 ? (
                    <>
                        <StatusBadge
                            tone={
                                overview.incompleteLineCount > 0
                                    ? "warning"
                                    : "success"
                            }
                            appearance="quiet"
                            label={`共 ${overview.uniqueAddressCount} 个地址`}
                        />
                        <span className="num text-xs text-muted-foreground">
                            已完整 {overview.completeLineCount}/
                            {overview.totalLines} 项
                        </span>
                        {overview.incompleteLineCount > 0 && onFilterLines ? (
                            <button
                                id="sales-delivery-summary-jump-issue"
                                type="button"
                                className="num cursor-pointer rounded-sm bg-warning-soft px-1.5 py-0.5 text-xs font-medium text-warning-soft-foreground transition-colors hover:underline"
                                onClick={() => onFilterLines("address_issue")}
                            >
                                {overview.incompleteLineCount} 项地址待核对
                            </button>
                        ) : null}
                        {overview.unsetLineCount > 0 && onFilterLines ? (
                            <button
                                id="sales-delivery-summary-jump-unset"
                                type="button"
                                className="num cursor-pointer rounded-sm bg-muted px-1.5 py-0.5 text-xs text-muted-foreground transition-colors hover:text-foreground hover:underline"
                                onClick={() => onFilterLines("no_address")}
                            >
                                {overview.unsetLineCount} 项未设地址
                            </button>
                        ) : null}
                    </>
                ) : (
                    <span className="text-xs text-muted-foreground">
                        按原询价交付要求办理，无实物配送地址。
                    </span>
                )}

                {coverageWarningCount > 0 ? (
                    <span
                        className="num inline-flex items-center gap-1 rounded-sm bg-warning-soft px-1.5 py-0.5 text-xs text-warning-soft-foreground"
                        title="部分商品在当前收货地区存在配送限制或需另付运费，请点击对应行收货地址查看"
                    >
                        <TriangleAlertIcon
                            className="size-3 shrink-0"
                            aria-hidden="true"
                        />
                        {coverageWarningCount} 项配送提示
                    </span>
                ) : null}
            </div>

            {!inquiryLocked ? (
                <div className="flex shrink-0 flex-wrap items-center gap-1">
                    {common && onEditUnifiedAddress ? (
                        <Button
                            id="sales-delivery-summary-edit-unified"
                            type="button"
                            variant="ghost"
                            size="xs"
                            className={detailWorkspaceStyles.inlineLinkButton}
                            onClick={onEditUnifiedAddress}
                        >
                            修改地址
                        </Button>
                    ) : null}
                    <Button
                        id="sales-delivery-summary-overview"
                        type="button"
                        variant="ghost"
                        size="xs"
                        className={detailWorkspaceStyles.quietButton}
                        onClick={onOpenOverview}
                    >
                        <ListChecksIcon
                            data-icon="inline-start"
                            className="size-3.5"
                            aria-hidden="true"
                        />
                        地址总览
                    </Button>
                </div>
            ) : (
                <Button
                    id="sales-delivery-overview-open"
                    type="button"
                    variant="ghost"
                    size="sm"
                    className={detailWorkspaceStyles.subtleButton}
                    onClick={onOpenOverview}
                >
                    <ListChecksIcon
                        data-icon="inline-start"
                        className="size-3.5"
                        aria-hidden="true"
                    />
                    查看配送分配
                </Button>
            )}
        </div>
    )
}
