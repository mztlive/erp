"use client"

import { MetricFilterItem, MetricStrip } from "@/components/business"
import type { ConnectionsUrlState } from "@/features/supplier-api-connections/lib/url-state"
import type { ConnectionListView } from "@/features/supplier-api-connections/types"

/** 连接指标筛选条；指标点击为筛选切换，均回第 1 页。 */
export function ConnectionMetricStrip({
    data,
    urlState,
    patchUrl,
}: {
    data: ConnectionListView | undefined
    urlState: ConnectionsUrlState
    patchUrl: (patch: Partial<ConnectionsUrlState>) => void
}) {
    return (
        <MetricStrip columns={5} aria-label="连接指标筛选">
            <MetricFilterItem
                label="已启用"
                value={data?.metrics.enabled ?? 0}
                active={urlState.status === "ENABLED"}
                onClick={() =>
                    patchUrl({
                        status:
                            urlState.status === "ENABLED"
                                ? undefined
                                : "ENABLED",
                        page: 1,
                    })
                }
            />
            <MetricFilterItem
                label="故障"
                value={data?.metrics.faulted ?? 0}
                active={urlState.status === "FAULTED"}
                onClick={() =>
                    patchUrl({
                        status:
                            urlState.status === "FAULTED"
                                ? undefined
                                : "FAULTED",
                        page: 1,
                    })
                }
            />
            <MetricFilterItem
                label="待配置"
                value={data?.metrics.pendingConfig ?? 0}
                active={urlState.status === "PENDING_CONFIG"}
                onClick={() =>
                    patchUrl({
                        status:
                            urlState.status === "PENDING_CONFIG"
                                ? undefined
                                : "PENDING_CONFIG",
                        page: 1,
                    })
                }
            />
            <MetricFilterItem
                label="健康异常"
                value={data?.metrics.healthAbnormal ?? 0}
                active={Boolean(urlState.health)}
                onClick={() =>
                    patchUrl({
                        health: urlState.health
                            ? undefined
                            : "FAILED,AUTH_FAILED,PARTIAL,UNKNOWN",
                        page: 1,
                    })
                }
            />
            <MetricFilterItem
                label="目录陈旧"
                value={data?.metrics.catalogStale ?? 0}
                active={Boolean(urlState.catalogFreshness)}
                onClick={() =>
                    patchUrl({
                        catalogFreshness: urlState.catalogFreshness
                            ? undefined
                            : "STALE,FAILED",
                        page: 1,
                    })
                }
            />
        </MetricStrip>
    )
}
