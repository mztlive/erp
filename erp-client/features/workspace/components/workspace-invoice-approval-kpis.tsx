"use client"

import type { ReactNode } from "react"
import { CircleQuestionMarkIcon } from "lucide-react"

import { MoneyValue, RateValue } from "@/components/business"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
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
        <TooltipProvider>
            <div
                role="region"
                aria-label="开票审批关键指标"
                className="grid min-w-0 grid-cols-1 gap-x-6 gap-y-3.5 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/invoice-approval:grid-cols-2 @[840px]/invoice-approval:grid-cols-3"
            >
                <div className="min-w-0 space-y-1">
                    <KpiLabel
                        label="本次申请开票金额"
                        basis="含税"
                        id={`${prefix}-amount-formula`}
                    >
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
                    </KpiLabel>
                    <MoneyValue
                        value={submission.amount}
                        display="exact"
                        className="text-value-section font-semibold tracking-tight [&_[data-slot=money-value-number]]:whitespace-normal [&_[data-slot=money-value-number]]:wrap-anywhere"
                    />
                    {submission.accountLabel || submission.lines.length > 0 ? (
                        <p className="num wrap-anywhere text-tiny text-muted-foreground">
                            {submission.accountLabel ??
                                `共 ${submission.lines.length} 项申请明细`}
                        </p>
                    ) : null}
                </div>

                <div className="min-w-0 space-y-1">
                    <KpiLabel
                        label="可开票额度"
                        basis="扣除其他占用后"
                        id={`${prefix}-quota-formula`}
                    >
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
                    </KpiLabel>
                    <MoneyValue
                        value={quota?.beforeRequest}
                        display="exact"
                        className="text-value-compact font-semibold tracking-tight [&_[data-slot=money-value-number]]:whitespace-normal [&_[data-slot=money-value-number]]:wrap-anywhere"
                    />
                    {review ? (
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
                    ) : null}
                </div>

                <div className="min-w-0 space-y-1.5 @[480px]/invoice-approval:col-span-2 @[840px]/invoice-approval:col-span-1">
                    <KpiLabel
                        label="票款进度"
                        basis="本笔应收"
                        id={`${prefix}-progress-formula`}
                    >
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
                    </KpiLabel>
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
            </div>
        </TooltipProvider>
    )
}

function KpiLabel({
    id,
    label,
    basis,
    children,
}: {
    id: string
    label: string
    basis: string
    children: ReactNode
}) {
    return (
        <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
            <span>{label}</span>
            <span className="text-tiny text-muted-foreground/80">{basis}</span>
            <Tooltip>
                <TooltipTrigger
                    render={
                        <button
                            id={id}
                            type="button"
                            aria-label={`${label}计算口径`}
                            className="inline-flex size-4 shrink-0 cursor-help items-center justify-center rounded-sm text-muted-foreground/80 transition-colors hover:text-foreground focus-visible:outline-2 focus-visible:outline-primary"
                        />
                    }
                >
                    <CircleQuestionMarkIcon
                        aria-hidden="true"
                        className="size-3"
                    />
                </TooltipTrigger>
                <TooltipContent className="block">{children}</TooltipContent>
            </Tooltip>
        </div>
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
                <span className="text-tiny text-muted-foreground">{label}</span>
                <span className="inline-flex max-w-full flex-wrap items-baseline justify-end gap-x-1.5">
                    <MoneyValue
                        value={amount}
                        display="exact"
                        className="text-body-compact font-semibold tracking-tight [&_[data-slot=money-value-number]]:whitespace-normal [&_[data-slot=money-value-number]]:wrap-anywhere"
                    />
                    {rate != null ? (
                        <RateValue
                            value={rate}
                            precision={1}
                            className="text-tiny text-muted-foreground"
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
