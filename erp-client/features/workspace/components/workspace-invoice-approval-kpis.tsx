"use client"

import { DecisionKpiBar, MoneyValue, RateValue } from "@/components/business"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { InvoiceApprovalReview } from "../api/invoice-review"
import {
    invoiceApprovalQuota,
    invoiceProgressPercent,
    invoiceProgressRate,
    type invoiceApprovalSubmission,
} from "./workspace-invoice-approval-model"

export function InvoiceApprovalKpis({
    prefix,
    submission,
    review,
    shortage,
}: {
    prefix: string
    submission: ReturnType<typeof invoiceApprovalSubmission>
    review?: InvoiceApprovalReview
    shortage?: string
}) {
    const quota = review && invoiceApprovalQuota(review)
    const availablePositive = (() => {
        if (!review || shortage) return false
        try {
            return compareDecimal(review.amounts.available, "0", 2) > 0
        } catch {
            return false
        }
    })()

    return (
        <DecisionKpiBar
            columns={3}
            ariaLabel="开票审批关键指标"
            items={[
                {
                    id: "amount",
                    label: "本次申请开票金额",
                    basis: "含税",
                    emphasis: "primary",
                    formulaTooltipId: `${prefix}-amount-formula`,
                    formulaTooltip: (
                        <div className="num space-y-0.5">
                            <p>
                                申请金额为各行含税金额之和。每行按数量 ×
                                含税单价计算，并舍入至分。
                            </p>
                            <FormulaAmount
                                label="申请金额"
                                value={submission.amount}
                            />
                            {submission.lineTotal != null ? (
                                <FormulaAmount
                                    label="明细合计"
                                    value={submission.lineTotal}
                                />
                            ) : (
                                <p>当前资料无法逐行计算合计。</p>
                            )}
                        </div>
                    ),
                    value: (
                        <MoneyValue
                            value={submission.amount}
                            display="exact"
                            inheritColor
                        />
                    ),
                    footnote:
                        submission.accountLabel || submission.lines.length > 0
                            ? (submission.accountLabel ??
                              `共 ${submission.lines.length} 项申请明细`)
                            : undefined,
                },
                {
                    id: "quota",
                    label: "可开票额度",
                    basis: "扣除其他占用后",
                    emphasis: "secondary",
                    formulaTooltipId: `${prefix}-quota-formula`,
                    formulaTooltip: (
                        <div className="num space-y-0.5">
                            <p>
                                可开票额度 = 可开票总额 − 已开票净额 −
                                其他有效占用，最低为 0。
                            </p>
                            <p>
                                余量或超额按未截断余额再扣除本次有效占用计算。
                            </p>
                            {review ? (
                                <>
                                    <FormulaAmount
                                        label="可开票总额"
                                        value={review.invoiceable_total}
                                    />
                                    <FormulaAmount
                                        label="已开票净额"
                                        value={review.invoiced_total}
                                    />
                                    <FormulaAmount
                                        label="其他审批中占用"
                                        value={review.amounts.other_pending}
                                    />
                                    <FormulaAmount
                                        label="其他已批准待开票"
                                        value={review.amounts.other_approved}
                                    />
                                    <FormulaAmount
                                        label="本次有效占用"
                                        value={review.amounts.current_reserved}
                                    />
                                </>
                            ) : null}
                        </div>
                    ),
                    value: (
                        <MoneyValue
                            value={quota?.beforeRequest}
                            display="exact"
                            inheritColor
                        />
                    ),
                    extra: review ? (
                        <p
                            className={cn(
                                "num flex flex-wrap items-baseline gap-1 text-xs font-medium",
                                shortage
                                    ? "text-destructive-soft-foreground"
                                    : availablePositive
                                      ? "text-success-soft-foreground"
                                      : "text-muted-foreground",
                            )}
                        >
                            <span>{shortage ? "超额" : "余量"}</span>
                            <MoneyValue
                                value={shortage ?? review.amounts.available}
                                display="exact"
                                inheritColor
                            />
                        </p>
                    ) : undefined,
                },
                {
                    id: "progress",
                    label: "票款进度",
                    basis: "本笔应收",
                    emphasis: "terms",
                    formulaTooltipId: `${prefix}-progress-formula`,
                    formulaTooltip: (
                        <div className="num space-y-0.5">
                            <p>回款进度 = 已核销回款 ÷ 本笔应收。</p>
                            <p>开票进度 = 已实际开票净额 ÷ 可开票总额。</p>
                            {review ? (
                                <>
                                    <FormulaAmount
                                        label="本笔应收"
                                        value={review.gross_total}
                                    />
                                    <FormulaAmount
                                        label="本笔未回款"
                                        value={review.open_total}
                                    />
                                    <FormulaAmount
                                        label="可开票总额"
                                        value={review.invoiceable_total}
                                    />
                                </>
                            ) : null}
                        </div>
                    ),
                    value: (
                        <div className="w-full space-y-1.5">
                            <ProgressAmount
                                label="已回款"
                                amount={review?.settled_total}
                                total={review?.gross_total}
                            />
                            <ProgressAmount
                                label="已开票"
                                amount={review?.invoiced_total}
                                total={review?.invoiceable_total}
                            />
                        </div>
                    ),
                },
            ]}
        />
    )
}

function FormulaAmount({
    label,
    value,
}: {
    label: string
    value?: string | null
}) {
    return (
        <p>
            {label} <MoneyValue value={value} display="exact" inheritColor />
        </p>
    )
}

function ProgressAmount({
    label,
    amount,
    total,
}: {
    label: string
    amount?: string
    total?: string
}) {
    const ratio =
        amount != null && total != null
            ? invoiceProgressPercent(amount, total)
            : undefined
    const rate =
        amount != null && total != null
            ? invoiceProgressRate(amount, total)
            : undefined

    return (
        <div className="min-w-0 space-y-1">
            <div className="flex min-w-0 flex-wrap items-baseline justify-between gap-x-2 gap-y-0.5">
                <span className="text-tiny font-normal text-muted-foreground">
                    {label}
                </span>
                <span className="inline-flex max-w-full flex-wrap items-baseline justify-end gap-x-1.5 font-semibold tracking-tight">
                    <MoneyValue value={amount} display="exact" inheritColor />
                    {rate != null ? (
                        <RateValue
                            value={rate}
                            precision={1}
                            className="text-tiny font-normal text-muted-foreground"
                        />
                    ) : null}
                </span>
            </div>
            {ratio != null ? (
                <div
                    aria-hidden="true"
                    className="h-1.5 overflow-hidden rounded-full bg-muted"
                >
                    <div
                        className={cn(
                            "h-full rounded-full",
                            label === "已回款" ? "bg-success" : "bg-info",
                        )}
                        style={{ width: `calc(${ratio} * 100%)` }}
                    />
                </div>
            ) : null}
        </div>
    )
}
