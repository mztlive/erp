"use client"

import { MoneyValue } from "@/components/business"
import { compareDecimal, subtractFixed } from "@/lib/fixed-decimal"
import {
    calculateExtraCostAmounts,
    formatPercentValue,
} from "@/features/sales-orders/lib/extra-cost"

/** 附加费用含税 / 税额 / 不含税成本实时试算条（口径与服务端 fee_amounts 一致）。 */
export function ExtraCostAmountBreakdown({
    grossAmount,
    taxRatePercent,
    quotaLimit,
}: {
    grossAmount: string
    taxRatePercent: string
    /** 可选可登记额度上限（询价来源模式），用于展示登记后剩余额度。 */
    quotaLimit?: string
}) {
    const breakdown = calculateExtraCostAmounts(grossAmount, taxRatePercent)
    const rateLabel = formatPercentValue(taxRatePercent)

    let quotaStatus:
        | { kind: "exceeded" }
        | { kind: "full" }
        | { kind: "remaining"; value: string }
        | null = null
    if (quotaLimit && breakdown) {
        try {
            const cmp = compareDecimal(breakdown.gross, quotaLimit, 2)
            if (cmp > 0) {
                quotaStatus = { kind: "exceeded" }
            } else if (cmp === 0) {
                quotaStatus = { kind: "full" }
            } else {
                quotaStatus = {
                    kind: "remaining",
                    value: subtractFixed(quotaLimit, breakdown.gross, {
                        maxScale: 2,
                        outputScale: 2,
                    }),
                }
            }
        } catch {
            quotaStatus = null
        }
    }

    return (
        <div
            aria-label="费用成本试算"
            className="rounded-lg bg-muted/40 px-4 py-2.5"
        >
            <dl className="grid grid-cols-3 gap-3">
                <div className="min-w-0 space-y-0.5">
                    <dt className="text-xs text-muted-foreground">含税金额</dt>
                    <dd className="num font-medium text-foreground">
                        {breakdown ? (
                            <MoneyValue
                                value={breakdown.gross}
                                display="exact"
                            />
                        ) : (
                            <span className="font-normal text-muted-foreground">
                                —
                            </span>
                        )}
                    </dd>
                </div>
                <div className="min-w-0 space-y-0.5">
                    <dt className="text-xs text-muted-foreground">
                        进项税额
                        {rateLabel !== "—" ? (
                            <span className="num ml-1">({rateLabel})</span>
                        ) : null}
                    </dt>
                    <dd className="num text-foreground/85">
                        {breakdown ? (
                            <MoneyValue value={breakdown.tax} display="exact" />
                        ) : (
                            <span className="text-muted-foreground">—</span>
                        )}
                    </dd>
                </div>
                <div className="min-w-0 space-y-0.5">
                    <dt className="text-xs text-muted-foreground">
                        计入成本（不含税）
                    </dt>
                    <dd className="num font-semibold text-foreground">
                        {breakdown ? (
                            <MoneyValue value={breakdown.net} display="exact" />
                        ) : (
                            <span className="font-normal text-muted-foreground">
                                —
                            </span>
                        )}
                    </dd>
                </div>
            </dl>
            {quotaStatus ? (
                <div className="mt-2 border-t border-border/60 pt-2 text-xs">
                    {quotaStatus.kind === "exceeded" ? (
                        <p className="text-destructive">
                            登记金额超出可登记额度上限{" "}
                            <MoneyValue value={quotaLimit} display="exact" />
                        </p>
                    ) : quotaStatus.kind === "full" ? (
                        <p className="text-muted-foreground">
                            本次将全额登记该笔询价费用额度。
                        </p>
                    ) : (
                        <p className="text-muted-foreground">
                            登记后该笔询价费用剩余可登记额度：
                            <MoneyValue
                                value={quotaStatus.value}
                                display="exact"
                                className="ml-1"
                            />
                        </p>
                    )}
                </div>
            ) : null}
        </div>
    )
}
