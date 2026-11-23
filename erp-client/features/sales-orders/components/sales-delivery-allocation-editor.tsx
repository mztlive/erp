"use client"

import { ListChecksIcon, MapPinIcon, TriangleAlertIcon } from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { toast } from "@/components/ui/toast"
import { RegionName } from "@/features/regions"
import type { DeliveryCoverageHint as CoverageCode } from "../api/delivery-coverage"
import { summarizeOrderDelivery } from "../lib/sales-delivery-pool"
import type { SalesOrderCreateFormApi } from "../lib/sales-order-create-form-types"
import type { SalesLineFilterKey } from "../lib/sales-order-create-line-health"
import {
    applyDeliveryAddressToLines,
    type UnifiedDeliveryAddress,
} from "../lib/sales-order-create-model"
import type { SalesOrderDraftLineInput } from "../types"
import { SalesDeliveryWorkbookActions } from "./sales-delivery-workbook-actions"
import { SalesOrderCreateUnifiedAddressDialog } from "./sales-order-create-unified-address-dialog"

// 改单页面继续复用相同的数量与完整性摘要。
export { allocationSummary } from "../lib/sales-delivery-allocation-summary"

/**
 * 紧凑单行收货安排条：嵌入销售明细工作区上方，彻底消除下方重复的 100 行地址表。
 * 统一地址一行呈现，多地址给出总览与问题快捷跳转；按商品编辑直接在明细表「收货地址」列点开右侧工作台。
 */
export function SalesDeliverySummaryBar({
    form,
    lines,
    draftId,
    inquiryLocked = false,
    coverageHint,
    onOpenOverview,
    onFilterLines,
}: {
    form: SalesOrderCreateFormApi
    lines: SalesOrderDraftLineInput[]
    draftId?: string
    inquiryLocked?: boolean
    coverageHint?: (sku: string, regionCode: string) => CoverageCode
    onOpenOverview: () => void
    onFilterLines?: (filter: SalesLineFilterKey) => void
}) {
    if (lines.length === 0) return null

    const overview = summarizeOrderDelivery(lines)
    const common = overview.unified
    const firstAddress =
        common ??
        (overview.groups[0]
            ? {
                  recipient: overview.groups[0].recipient,
                  phone: overview.groups[0].phone,
                  region_code: overview.groups[0].region_code,
                  address: overview.groups[0].address,
              }
            : undefined)

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

    const applyUnifiedAddress = (address: UnifiedDeliveryAddress) => {
        const next = applyDeliveryAddressToLines(
            form.getFieldValue("lineItems"),
            address,
        )
        form.setFieldValue("lineItems", next)
        toast.add({
            title: "已设置统一收货地址",
            description: `${next.length} 项商品均按全部数量送到该地址。`,
            type: "success",
            timeout: 3000,
        })
    }

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
                    <>
                        <span className="rounded-sm bg-muted px-1.5 py-0.5 text-tiny font-medium text-muted-foreground">
                            选填
                        </span>
                        <span className="text-xs text-muted-foreground">
                            {inquiryLocked
                                ? "按原询价交付要求办理，无实物配送地址。"
                                : "可设统一地址、勾选部分商品分批设置，或点明细行「收货地址」逐项配置"}
                        </span>
                    </>
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
                <div className="flex shrink-0 flex-wrap items-center gap-1.5">
                    <SalesOrderCreateUnifiedAddressDialog
                        lineCount={lines.length}
                        allocatedLineCount={overview.allocatedLineCount}
                        initialAddress={firstAddress}
                        existingAddresses={overview.groups}
                        triggerLabel={common ? "修改统一地址" : "设置统一地址"}
                        onApply={applyUnifiedAddress}
                    />
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
                        地址总览
                        {overview.uniqueAddressCount > 0 ? (
                            <span className="num">
                                （{overview.uniqueAddressCount}）
                            </span>
                        ) : null}
                    </Button>
                    <SalesDeliveryWorkbookActions
                        form={form}
                        lines={lines}
                        draftId={draftId}
                        onApplied={onOpenOverview}
                    />
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
