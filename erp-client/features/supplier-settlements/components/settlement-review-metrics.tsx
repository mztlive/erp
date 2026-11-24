import type { ReactNode } from "react"
import { CircleHelpIcon } from "lucide-react"

import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
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

function MetricHelp({
    id,
    label,
    children,
}: {
    id: string
    label: string
    children: ReactNode
}) {
    return (
        <Tooltip>
            <TooltipTrigger
                id={id}
                type="button"
                aria-label={label}
                className="inline-flex size-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
            >
                <CircleHelpIcon className="size-3.5" aria-hidden="true" />
            </TooltipTrigger>
            <TooltipContent>{children}</TooltipContent>
        </Tooltip>
    )
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
    const segments = [
        { label: "已匹配", count: matched, color: "bg-success" },
        { label: "待处理差异", count: pending, color: "bg-warning" },
        { label: "争议", count: disputed, color: "bg-destructive" },
    ]

    return (
        <TooltipProvider>
            <dl
                aria-label="结算复核摘要"
                className="grid grid-cols-1 gap-x-4 gap-y-3 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/settlement-task:grid-cols-2 @[840px]/settlement-task:grid-cols-4"
            >
                <div className="min-w-0">
                    <dt className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                        <span>结算单总额</span>
                        <span className="text-tiny text-muted-foreground/80">
                            含税
                        </span>
                        <MetricHelp
                            id={`${idPrefix}-supplier-formula`}
                            label="查看结算单总额口径"
                        >
                            供应商账单整单含税金额。账单金额缺失时显示待同步。
                        </MetricHelp>
                    </dt>
                    <dd className="num mt-0.5 text-value-section font-semibold tracking-tight wrap-anywhere">
                        {supplierTotal === "—" ? "待同步" : supplierTotal}
                    </dd>
                    <dd className="text-xs text-muted-foreground">
                        供应商账单 · 含税
                    </dd>
                </div>
                <div className="min-w-0">
                    <dt className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                        <span>ERP 核算总额</span>
                        <span className="text-tiny text-muted-foreground/80">
                            含税
                        </span>
                        <MetricHelp
                            id={`${idPrefix}-erp-formula`}
                            label="查看 ERP 核算与差额口径"
                        >
                            ERP
                            含税金额为订单结算价加运费和服务费，再扣除退款。差额为供应商账单总额减
                            ERP 核算总额。
                        </MetricHelp>
                    </dt>
                    <dd className="num mt-0.5 text-value-compact font-semibold wrap-anywhere">
                        {settlementMoney(totals.erpAmountGross)}
                    </dd>
                    <dd className={`num text-xs wrap-anywhere ${delta.tone}`}>
                        账单差额 {delta.label}
                    </dd>
                </div>
                <div className="min-w-0">
                    <dt className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                        <span>核对进度</span>
                        <span className="text-tiny text-muted-foreground/80">
                            按明细 / 差异
                        </span>
                        <MetricHelp
                            id={`${idPrefix}-progress-formula`}
                            label="查看核对进度统计口径"
                        >
                            分子为账单行与 ERP
                            核算金额一致的明细项数，分母为结算明细总数；进度条按已匹配、待处理差异与争议阻塞比例展示。
                        </MetricHelp>
                    </dt>
                    <dd className="num mt-0.5 text-value-compact font-semibold">
                        {matched} / {items.length} 项
                    </dd>
                    <dd>
                        <div
                            className="my-1.5 flex h-1.5 overflow-hidden rounded-full bg-muted"
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
                        <p className="mt-0.5 text-tiny text-muted-foreground">
                            金额核对按明细计，未结差异按记录计
                        </p>
                    </dd>
                </div>
                <div className="min-w-0">
                    <dt className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                        <span>结算期间</span>
                        <span className="text-tiny text-muted-foreground/80">
                            账期
                        </span>
                    </dt>
                    <dd className="num mt-1 text-body-compact font-semibold wrap-anywhere">
                        {statement.periodStart} 至 {statement.periodEnd}
                    </dd>
                    <dd className="mt-0.5 text-xs text-muted-foreground">
                        {statement.status === "CONFIRMED"
                            ? "已确认结算"
                            : canConfirm
                              ? "复核条件已满足"
                              : "确认需差异已处理并满足复核条件"}
                    </dd>
                </div>
            </dl>
        </TooltipProvider>
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
