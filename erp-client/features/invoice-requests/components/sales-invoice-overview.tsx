"use client"

import type { ReactNode } from "react"

import {
    DetailHint,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspaceSection,
    MoneyValue,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { sumFixed } from "@/lib/fixed-decimal"
import type { useInvoiceRequestAvailability } from "../hooks/queries"

/** 销售单摘要只合计完整读取的应收额度，不从申请列表推算。 */
export function SalesInvoiceOverview({
    summary,
    availability,
    canRead,
    canSubmit,
    onCreate,
}: {
    summary: { invoiced: ReactNode; uninvoiced: ReactNode }
    availability: ReturnType<typeof useInvoiceRequestAvailability>
    canRead: boolean
    canSubmit: boolean
    onCreate?: () => void
}) {
    const complete =
        canRead &&
        availability.length > 0 &&
        availability.every((query) => query.isSuccess && !query.isFetching)
    const failed = canRead && availability.some((query) => query.isError)
    const metrics = [
        ["可申请金额", "available_amount"],
        ["审批中金额", "pending_amount"],
        ["待财务开票", "approved_remaining_amount"],
    ] as const
    const blockedReason = Array.from(
        new Set(
            availability
                .map((query) => query.data?.unavailable_reason)
                .filter(Boolean),
        ),
    ).join("；")
    return (
        <DetailWorkspaceSection
            width="full"
            title="开票概况"
            badge={
                <DetailHint
                    id="sales-order-invoice-summary-hint"
                    label="开票金额"
                >
                    已开票净额只统计分配到本单的含税净额，已扣除冲减金额。未开票金额包含审批中及待财务开票的金额；可申请金额已扣除这些占用。开票与回款分别统计。
                </DetailHint>
            }
            actions={
                canSubmit ? (
                    <Button
                        id="invoice-request-create"
                        size="sm"
                        className="px-3.5"
                        disabled={!onCreate}
                        title={
                            !onCreate ? blockedReason || undefined : undefined
                        }
                        onClick={onCreate}
                    >
                        申请开票
                    </Button>
                ) : null
            }
        >
            <DetailWorkspaceMetrics
                columns={2}
                variant="inline"
                ariaLabel="开票概况"
                className="max-w-xl"
            >
                <DetailWorkspaceMetricItem
                    emphasis="primary"
                    label="已开票净额"
                    value={summary.invoiced}
                />
                <DetailWorkspaceMetricItem
                    label="未开票金额"
                    value={summary.uninvoiced}
                />
            </DetailWorkspaceMetrics>

            {canRead ? (
                <div className="space-y-2 border-t border-border/60 pt-3">
                    <dl className="flex flex-wrap gap-x-6 gap-y-2 text-xs text-muted-foreground">
                        {metrics.map(([label, field]) => (
                            <div
                                key={field}
                                className="flex items-baseline gap-2"
                            >
                                <dt>{label}</dt>
                                <dd className="num font-medium text-foreground">
                                    {complete ? (
                                        <MoneyValue
                                            value={sumFixed(
                                                availability.map(
                                                    (query) =>
                                                        query.data![field],
                                                ),
                                                { maxScale: 2, outputScale: 2 },
                                            )}
                                        />
                                    ) : (
                                        "待确认"
                                    )}
                                </dd>
                            </div>
                        ))}
                    </dl>
                    {failed ? (
                        <div
                            className="flex items-center gap-2 text-xs text-destructive"
                            role="alert"
                        >
                            <span>申请额度读取失败</span>
                            <Button
                                id="sales-invoice-amounts-retry"
                                variant="link"
                                size="sm"
                                className="h-auto p-0"
                                onClick={() =>
                                    void Promise.all(
                                        availability.map((query) =>
                                            query.refetch(),
                                        ),
                                    )
                                }
                            >
                                重试
                            </Button>
                        </div>
                    ) : null}
                    {complete && canSubmit && !onCreate && blockedReason ? (
                        <p className="text-xs text-muted-foreground">
                            {blockedReason}
                        </p>
                    ) : null}
                    {onCreate ? (
                        <p className="text-xs text-muted-foreground">
                            销售单生效后即可申请，无需等待回款或发货。审批通过后由财务开票。
                        </p>
                    ) : null}
                </div>
            ) : null}
        </DetailWorkspaceSection>
    )
}
