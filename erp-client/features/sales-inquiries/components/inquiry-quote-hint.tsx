"use client"
import {
    latestSupplierQuotes,
    quoteExclusion,
    quotePriceHint,
    type InternalQuote,
} from "../lib/internal-quotes"
import type { InquiryLine } from "../types"

export function InquiryQuoteHint({
    quotes,
    line,
    round,
    price,
    tax,
    pending,
    failed,
}: {
    quotes: InternalQuote[]
    line: InquiryLine
    round: number
    price?: string | null
    tax?: string | null
    pending: boolean
    failed: boolean
}) {
    if (failed)
        return (
            <p className="mt-1 whitespace-normal text-xs text-destructive">
                报价参考读取失败，请重试
            </p>
        )
    if (pending)
        return (
            <p className="mt-1 text-xs text-muted-foreground">
                正在读取报价参考…
            </p>
        )
    if (!tax)
        return (
            <p className="mt-1 text-xs text-muted-foreground">
                填写税率后比较报价
            </p>
        )
    let comparable: InternalQuote[]
    try {
        comparable = latestSupplierQuotes(quotes).filter(
            (quote) =>
                quote.line_id === line.line_id &&
                !quoteExclusion(quote, line, round, tax),
        )
    } catch {
        return (
            <p className="mt-1 text-xs text-muted-foreground">请核对税率格式</p>
        )
    }
    const hint = quotePriceHint(comparable, price)
    return (
        <div
            className="mt-1 space-y-0.5 whitespace-normal text-xs leading-4 text-muted-foreground"
            aria-live="polite"
        >
            {hint ? (
                <>
                    <p>
                        ¥{hint.min}–{hint.max} · {comparable.length} 家 · 含运费
                    </p>
                    <p
                        className={
                            hint.below
                                ? "text-warning-soft-foreground"
                                : undefined
                        }
                    >
                        {hint.message}
                    </p>
                </>
            ) : (
                <p>暂无可比报价</p>
            )}
        </div>
    )
}
