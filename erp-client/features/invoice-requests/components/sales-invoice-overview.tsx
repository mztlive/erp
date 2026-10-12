"use client"

import type { ReactNode } from "react"
import { Button } from "@/components/ui/button"
import { MoneyValue } from "@/components/business"
import { DetailHint } from "@/components/business/detail-presentation"
import { sumFixed } from "@/lib/fixed-decimal"
import type { useInvoiceRequestAvailability } from "../hooks/queries"

/** 销售单摘要只合计完整读取的应收额度，不从申请列表推算。 */
export function SalesInvoiceOverview({
    summary,
    availability,
    canRead,
    onCreate,
}: {
    summary: { invoiced: ReactNode; uninvoiced: ReactNode }
    availability: ReturnType<typeof useInvoiceRequestAvailability>
    canRead: boolean
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
    return (
        <section
            aria-label="开票概况"
            className="rounded-xl border border-border/70 bg-muted/35 px-5 py-4"
        >
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="flex items-center gap-1">
                    <h2 className="text-sm font-semibold">开票概况</h2>
                    <DetailHint
                        id="sales-order-invoice-summary-hint"
                        label="开票金额"
                    >
                        已开票净额只统计分配到本单的含税净额，已扣除冲减金额。未开票金额包含审批中及待财务开票的金额；可申请金额已扣除这些占用。开票与回款分别统计。
                    </DetailHint>
                </div>
                {onCreate ? (
                    <Button
                        id="invoice-request-create"
                        size="sm"
                        onClick={onCreate}
                    >
                        申请开票
                    </Button>
                ) : null}
            </div>
            <dl className="mt-3 grid grid-cols-2 gap-4">
                {[
                    ["已开票净额", summary.invoiced],
                    ["未开票金额", summary.uninvoiced],
                ].map(([label, value], index) => (
                    <div key={index} className="min-w-0">
                        <dt className="text-xs text-muted-foreground">
                            {label}
                        </dt>
                        <dd className="mt-1 text-2xl font-semibold wrap-anywhere">
                            {value}
                        </dd>
                    </div>
                ))}
            </dl>
            {canRead ? (
                <div className="mt-4 border-t border-border/70 pt-3">
                    <dl className="flex flex-wrap gap-x-6 gap-y-2 text-xs text-muted-foreground">
                        {metrics.map(([label, field]) => (
                            <div
                                key={field}
                                className="flex items-baseline gap-2"
                            >
                                <dt>{label}</dt>
                                <dd className="text-foreground">
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
                            className="mt-2 flex items-center gap-2 text-xs"
                            role="alert"
                        >
                            <span>申请额度读取失败</span>
                            <Button
                                id="sales-invoice-amounts-retry"
                                variant="link"
                                size="sm"
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
                    {complete && !onCreate ? (
                        <p className="mt-2 text-xs text-muted-foreground">
                            {Array.from(
                                new Set(
                                    availability
                                        .map(
                                            (query) =>
                                                query.data?.unavailable_reason,
                                        )
                                        .filter(Boolean),
                                ),
                            ).join("；") || "当前账号没有申请开票的权限。"}
                        </p>
                    ) : null}
                    {onCreate ? (
                        <p className="mt-2 text-xs text-muted-foreground">
                            销售单生效后即可申请，无需等待回款或发货。审批通过后由财务开票。
                        </p>
                    ) : null}
                </div>
            ) : null}
        </section>
    )
}
