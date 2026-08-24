import * as React from "react"

import { useOwnerOptionsQuery } from "@/hooks/use-options"
import {
    salesOrderCloseLabel,
    salesOrderCollectionLabel,
    salesOrderCommercialStatusLabel,
    salesOrderFulfillmentLabel,
    salesOrderInvoiceLabel,
    salesOrderReviewStatusLabel,
    salesOrderSummaryLabels,
} from "@/features/sales-orders/lib/filter-orders"
import { NATURE_LABEL, ORIGIN_LABEL } from "@/features/sales-orders/lib/labels"
import type { SalesOrdersUrlState } from "@/features/sales-orders/lib/url-state"
import type { SalesOrderListItem } from "@/features/sales-orders/types"
import type { SalesOrdersListFilterKey } from "./use-sales-orders-list-filters"

export type SalesOrdersAppliedChip = Readonly<{
    key: SalesOrdersListFilterKey
    label: string
    clearLabel: string
    onClear: () => void
}>

/**
 * 已生效筛选的 chip 清单：关键词、工作视图、结构化条件与来源锁定参数
 * （customerId 等）全部显性展示，每个 chip 只移除自己的条件。
 */
export function useSalesOrdersListChips(
    url: SalesOrdersUrlState,
    items: readonly SalesOrderListItem[],
    removeFilter: (key: SalesOrdersListFilterKey) => void,
): readonly SalesOrdersAppliedChip[] {
    const ownerOptionsQuery = useOwnerOptionsQuery()
    // 来源锁定条件命中时，当前页所有行都属于同一客户/合同，可用行数据解析业务名
    const sample = items[0]

    return React.useMemo(() => {
        const chips: SalesOrdersAppliedChip[] = []
        if (url.search) {
            chips.push({
                key: "search",
                label: `搜索：${url.search}`,
                clearLabel: "清除关键词",
                onClear: () => removeFilter("search"),
            })
        }
        if (url.summary !== "all") {
            chips.push({
                key: "summary",
                label: `工作视图：${salesOrderSummaryLabels(url.summary)}`,
                clearLabel: "清除工作视图",
                onClear: () => removeFilter("summary"),
            })
        }
        if (url.customerId) {
            chips.push({
                key: "customerId",
                label: `客户：${sample?.customerName ?? "已选客户"}`,
                clearLabel: "清除客户筛选",
                onClear: () => removeFilter("customerId"),
            })
        }
        if (url.contractId) {
            chips.push({
                key: "contractId",
                label: `合同：${sample?.contractNumber || "已选合同"}`,
                clearLabel: "清除合同筛选",
                onClear: () => removeFilter("contractId"),
            })
        }
        if (url.createdBy) {
            const ownerLabel = ownerOptionsQuery.data?.find(
                (owner) => owner.userId === url.createdBy,
            )?.displayName
            chips.push({
                key: "createdBy",
                label: `创建人：${ownerLabel ?? "已选创建人"}`,
                clearLabel: "清除创建人筛选",
                onClear: () => removeFilter("createdBy"),
            })
        }
        if (url.nature !== "all") {
            chips.push({
                key: "nature",
                label: `业务性质：${NATURE_LABEL[url.nature]}`,
                clearLabel: "清除业务性质",
                onClear: () => removeFilter("nature"),
            })
        }
        if (url.origin !== "all") {
            chips.push({
                key: "origin",
                label: `创建来源：${ORIGIN_LABEL[url.origin]}`,
                clearLabel: "清除创建来源",
                onClear: () => removeFilter("origin"),
            })
        }
        if (url.commercialStatus !== "all") {
            chips.push({
                key: "commercialStatus",
                label: `商业状态：${salesOrderCommercialStatusLabel(
                    url.commercialStatus,
                )}`,
                clearLabel: "清除商业状态",
                onClear: () => removeFilter("commercialStatus"),
            })
        }
        if (url.reviewStatus !== "all") {
            chips.push({
                key: "reviewStatus",
                label: `审核状态：${salesOrderReviewStatusLabel(
                    url.reviewStatus,
                )}`,
                clearLabel: "清除审核状态",
                onClear: () => removeFilter("reviewStatus"),
            })
        }
        if (url.fulfillment !== "all") {
            chips.push({
                key: "fulfillment",
                label: `履约进度：${salesOrderFulfillmentLabel(
                    url.fulfillment,
                )}`,
                clearLabel: "清除履约进度",
                onClear: () => removeFilter("fulfillment"),
            })
        }
        if (url.collection !== "all") {
            chips.push({
                key: "collection",
                label: `回款进度：${salesOrderCollectionLabel(url.collection)}`,
                clearLabel: "清除回款进度",
                onClear: () => removeFilter("collection"),
            })
        }
        if (url.invoice !== "all") {
            chips.push({
                key: "invoice",
                label: `开票进度：${salesOrderInvoiceLabel(url.invoice)}`,
                clearLabel: "清除开票进度",
                onClear: () => removeFilter("invoice"),
            })
        }
        if (url.closeStatus !== "all") {
            chips.push({
                key: "closeStatus",
                label: `关闭状态：${salesOrderCloseLabel(url.closeStatus)}`,
                clearLabel: "清除关闭状态",
                onClear: () => removeFilter("closeStatus"),
            })
        }
        if (url.createdFrom || url.createdTo) {
            chips.push({
                key: "createdDate",
                label: `创建日期：${url.createdFrom || "不限"} 至 ${
                    url.createdTo || "不限"
                }`,
                clearLabel: "清除创建日期",
                onClear: () => removeFilter("createdDate"),
            })
        }
        return chips
    }, [
        ownerOptionsQuery.data,
        removeFilter,
        sample,
        url.closeStatus,
        url.collection,
        url.commercialStatus,
        url.contractId,
        url.createdBy,
        url.createdFrom,
        url.createdTo,
        url.customerId,
        url.fulfillment,
        url.invoice,
        url.nature,
        url.origin,
        url.reviewStatus,
        url.search,
        url.summary,
    ])
}
