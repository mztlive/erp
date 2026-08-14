"use client"

import { MetricFilterItem, MetricStrip } from "@/components/business"
import type {
    SupplierOrdersUrlState,
    SupplierOrdersUrlUpdater,
} from "@/features/supplier-orders/lib/url-state"
import type { SupplierOrderMetric } from "@/features/supplier-orders/types"

export type SupplierOrdersListMetricStripProps = {
    metrics: SupplierOrderMetric[]
    url: SupplierOrdersUrlState
    updateUrl: SupplierOrdersUrlUpdater
}

/**
 * 指标一键筛选。
 * 互斥规则（D21 记录在案）：履约状态 / 售后待处理 / 视图三组指标互斥——
 * 点击任一组都会静默重置其它组（避免组合出矛盾空结果），
 * 已通过按钮 title 提示；履约多值（fulfillmentStatuses）与单值
 * （fulfillmentStatus）为同一维度。
 */
export function SupplierOrdersListMetricStrip({
    metrics,
    url,
    updateUrl,
}: SupplierOrdersListMetricStripProps) {
    return (
        <MetricStrip>
            {metrics.map((m) => (
                <MetricFilterItem
                    key={m.key}
                    label={m.label}
                    value={m.value}
                    title={
                        m.fulfillmentStatuses?.length || m.fulfillmentStatus
                            ? "应用履约状态筛选，会清除「售后待处理」指标"
                            : m.aftersalePending
                              ? "应用「售后待处理」筛选，会清除履约状态筛选"
                              : m.view
                                ? "切换视图，会清除履约与售后筛选"
                                : undefined
                    }
                    active={
                        m.fulfillmentStatuses?.length
                            ? (url.fulfillmentStatuses?.length ?? 0) ===
                                  m.fulfillmentStatuses.length &&
                              m.fulfillmentStatuses.every((s) =>
                                  url.fulfillmentStatuses?.includes(s),
                              )
                            : m.fulfillmentStatus
                              ? (url.fulfillmentStatuses?.includes(
                                    m.fulfillmentStatus,
                                ) ?? false)
                              : m.aftersalePending
                                ? url.aftersalePending === true
                                : m.view
                                  ? url.view === m.view &&
                                    !url.fulfillmentStatuses?.length &&
                                    !url.aftersalePending
                                  : false
                    }
                    onClick={() => {
                        if (m.fulfillmentStatuses?.length) {
                            updateUrl({
                                fulfillmentStatuses: m.fulfillmentStatuses,
                                aftersalePending: false,
                                view: m.fulfillmentStatuses.includes(
                                    "RESULT_UNKNOWN",
                                )
                                    ? "all"
                                    : url.view,
                                page: 1,
                            })
                        } else if (m.fulfillmentStatus) {
                            updateUrl({
                                fulfillmentStatuses: [m.fulfillmentStatus],
                                aftersalePending: false,
                                view:
                                    m.fulfillmentStatus === "RESULT_UNKNOWN"
                                        ? "all"
                                        : url.view,
                                page: 1,
                            })
                        } else if (m.aftersalePending) {
                            updateUrl({
                                view: "all",
                                fulfillmentStatuses: undefined,
                                aftersalePending: !url.aftersalePending,
                                page: 1,
                            })
                        } else if (m.view) {
                            updateUrl({
                                view: m.view,
                                fulfillmentStatuses: undefined,
                                aftersalePending: false,
                                page: 1,
                            })
                        }
                    }}
                />
            ))}
        </MetricStrip>
    )
}
