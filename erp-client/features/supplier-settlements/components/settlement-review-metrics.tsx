import {
    DecisionKpiBar,
    DecisionKpiCell,
    type DecisionKpiFootnoteTone,
} from "@/components/business"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"
import {
    compareDecimal,
    formatCurrencyFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"

const MONEY_DISPLAY = { maxScale: 6, minimumFractionDigits: 2 } as const
const MONEY_SUM = { maxScale: 6, outputScale: 6, allowNegative: true } as const

export function settlementMoney(value: string | undefined): string {
    if (value == null) return "—"
    try {
        return formatCurrencyFixed(value, MONEY_DISPLAY)
    } catch {
        return "—"
    }
}

export function settlementLineDelta(
    supplierAmount: string | undefined,
    erpAmount: string,
) {
    if (supplierAmount == null) return undefined
    try {
        return subtractFixed(supplierAmount, erpAmount, MONEY_SUM)
    } catch {
        return undefined
    }
}

export function settlementDifference(value: string | undefined) {
    if (value == null || settlementMoney(value) === "—") {
        return { label: "待核对", tone: "text-muted-foreground" }
    }
    const sign = compareDecimal(value, "0", MONEY_SUM.maxScale)
    if (sign === 0) {
        return {
            label: `一致 ${settlementMoney("0")}`,
            tone: "text-success-soft-foreground",
        }
    }
    return sign > 0
        ? {
              label: `多 ${settlementMoney(value)}`,
              tone: "text-destructive-soft-foreground",
          }
        : {
              label: `少 ${settlementMoney(value.replace(/^-/, ""))}`,
              tone: "text-warning-soft-foreground",
          }
}

/** 服务端应付口径为冻结 ERP 金额加上 ERP 已接受的含税差异。 */
export function settlementPayablePreview(detail: SettlementDetailView) {
    const accepted = detail.differences.filter(
        (difference) => difference.status === "ERP_ACCEPTED",
    )
    try {
        const costDelta = sumFixed(
            accepted.map((difference) => difference.amountGross ?? ""),
            MONEY_SUM,
        )
        const netPayable = sumFixed(
            [detail.statement.erpAmountGross, costDelta],
            MONEY_SUM,
        )
        return { costDelta, netPayable }
    } catch {
        return { costDelta: undefined, netPayable: undefined }
    }
}

export function SettlementReviewMetrics({
    detail,
    idPrefix,
    matched,
    pending,
    disputed,
    canConfirm,
}: {
    detail: SettlementDetailView
    idPrefix: string
    matched: number
    pending: number
    disputed: number
    canConfirm: boolean
}) {
    const { statement, totals, items } = detail
    const supplierTotal = settlementMoney(totals.supplierAmountGross)
    const delta = settlementDifference(totals.differenceAmountGross)
    const deltaFootnoteTone: DecisionKpiFootnoteTone =
        delta.tone === "text-destructive-soft-foreground"
            ? "destructive"
            : delta.tone === "text-warning-soft-foreground"
              ? "warning"
              : delta.tone === "text-success-soft-foreground"
                ? "success"
                : "muted"
    const segments = [
        { label: "已匹配", count: matched, color: "bg-success" },
        { label: "待处理差异", count: pending, color: "bg-warning" },
        { label: "争议", count: disputed, color: "bg-destructive" },
    ]

    return (
        <DecisionKpiBar
            columns={4}
            ariaLabel="结算复核摘要"
            gridClassName="gap-x-4 gap-y-3"
        >
            <DecisionKpiCell
                label="结算单总额"
                basis="含税"
                emphasis="primary"
                formulaTooltipId={`${idPrefix}-supplier-formula`}
                formulaTooltipLabel="查看结算单总额口径"
                formulaTooltip="供应商账单整单含税金额。账单金额缺失时显示待同步。"
                value={supplierTotal === "—" ? "待同步" : supplierTotal}
                footnote="供应商账单 · 含税"
            />
            <DecisionKpiCell
                label="ERP 核算总额"
                basis="含税"
                emphasis="secondary"
                formulaTooltipId={`${idPrefix}-erp-formula`}
                formulaTooltipLabel="查看 ERP 核算与差额口径"
                formulaTooltip="ERP 含税金额为订单结算价加运费和服务费，再扣除退款。差额为供应商账单总额减 ERP 核算总额。"
                value={settlementMoney(totals.erpAmountGross)}
                footnote={`账单差额 ${delta.label}`}
                footnoteTone={deltaFootnoteTone}
            />
            <DecisionKpiCell
                label="核对进度"
                basis="按明细 / 差异"
                emphasis="secondary"
                formulaTooltipId={`${idPrefix}-progress-formula`}
                formulaTooltipLabel="查看核对进度统计口径"
                formulaTooltip="分子为账单行与 ERP 核算金额一致的明细项数，分母为结算明细总数；进度条按已匹配、待处理差异与争议阻塞比例展示。"
                value={`${matched} / ${items.length} 项`}
                extra={
                    <>
                        <div
                            className="mb-1.5 flex h-1.5 overflow-hidden rounded-full bg-muted"
                            aria-hidden="true"
                        >
                            {segments.map(
                                (part) =>
                                    part.count > 0 && (
                                        <div
                                            key={part.label}
                                            className={part.color}
                                            style={{ flex: part.count }}
                                        />
                                    ),
                            )}
                        </div>
                        <ul className="flex flex-wrap gap-x-2 gap-y-0.5 text-tiny text-muted-foreground">
                            {segments.map((part) => (
                                <li
                                    key={part.label}
                                    className="inline-flex items-center gap-1"
                                >
                                    <span
                                        className={`size-1.5 rounded-full ${part.color}`}
                                        aria-hidden="true"
                                    />
                                    {part.label}{" "}
                                    <span className="num">{part.count}</span>
                                </li>
                            ))}
                        </ul>
                    </>
                }
                footnote="金额核对按明细计，未结差异按记录计"
            />
            <DecisionKpiCell
                label="结算期间"
                basis="账期"
                emphasis="terms"
                value={`${statement.periodStart} 至 ${statement.periodEnd}`}
                footnote={
                    statement.status === "CONFIRMED"
                        ? "已确认结算"
                        : canConfirm
                          ? "复核条件已满足"
                          : "确认需差异已处理并满足复核条件"
                }
            />
        </DecisionKpiBar>
    )
}

export function SettlementReviewFooterSummary({
    itemCount,
    matched,
    unresolved,
    netPayable,
}: {
    itemCount: number
    matched: number
    unresolved: number
    netPayable?: string
}) {
    const metrics = [
        { label: "明细", value: itemCount },
        { label: "匹配", value: matched },
        { label: "待处理", value: unresolved },
        { label: "净应付", value: settlementMoney(netPayable) },
    ]
    return (
        <dl
            aria-label="整单核对汇总"
            aria-live="polite"
            className="flex min-w-0 flex-wrap items-baseline gap-x-2.5 gap-y-0.5 text-xs"
        >
            {metrics.map(({ label, value }) => (
                <div key={label} className="flex min-w-0 items-baseline gap-1">
                    <dt className="shrink-0 text-muted-foreground">{label}</dt>
                    <dd className="num min-w-0 font-semibold wrap-anywhere">
                        {value}
                    </dd>
                </div>
            ))}
        </dl>
    )
}
