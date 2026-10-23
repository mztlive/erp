"use client"
import { cn } from "@/lib/utils"
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
    const exclusions = new Set<string>()
    const current = latestSupplierQuotes(quotes).filter(
        (quote) => quote.line_id === line.line_id,
    )
    try {
        comparable = current.filter((quote) => {
            const reason = quoteExclusion(quote, line, round, tax)
            if (reason) exclusions.add(reason)
            return !reason
        })
    } catch {
        return (
            <p className="mt-1 text-xs text-muted-foreground">请核对税率格式</p>
        )
    }
    const hint = quotePriceHint(comparable, price, line.quantity)
    const priceRange = hint
        ? hint.min === hint.max
            ? `¥${hint.min}`
            : `¥${hint.min}–${hint.max}`
        : ""
    return (
        <div
            className="mt-1.5 min-w-0 space-y-1 text-xs leading-4 text-muted-foreground"
            aria-live="polite"
        >
            {hint ? (
                <>
                    <p
                        className="flex min-w-0 items-baseline justify-end gap-1 whitespace-nowrap"
                        title={`参考单价 ${priceRange}，${new Set(comparable.map((item) => item.supplier_id)).size} 家供应商`}
                        aria-label={`参考单价 ${priceRange}，${new Set(comparable.map((item) => item.supplier_id)).size} 家供应商`}
                    >
                        <span className="min-w-0 truncate font-medium tabular-nums">
                            {priceRange}
                        </span>
                        <span className="shrink-0">
                            ·{" "}
                            {
                                new Set(
                                    comparable.map((item) => item.supplier_id),
                                ).size
                            }{" "}
                            家 ·{" "}
                            {comparable.some((item) => item.system_price_kind)
                                ? "运费待核"
                                : "含运费"}
                        </span>
                    </p>
                    {hint.message && (
                        <p
                            className={cn(
                                "truncate tabular-nums",
                                hint.below && "text-warning-soft-foreground",
                            )}
                            title={hint.message}
                            aria-label={hint.message}
                        >
                            {hint.compactMessage}
                        </p>
                    )}
                </>
            ) : (
                <p
                    className="truncate"
                    title={
                        current.length
                            ? `已有 ${current.length} 家报价，暂不可比：${[...exclusions].join("；")}`
                            : "暂无供应商报价"
                    }
                >
                    {current.length
                        ? `${current.length} 家暂不可比 · ${[...exclusions].join("；")}`
                        : "暂无供应商报价"}
                </p>
            )}
        </div>
    )
}
