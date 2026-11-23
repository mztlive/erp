"use client"

import { ChevronRightIcon, PlusIcon } from "lucide-react"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import {
    compactFixed,
    compareDecimal,
    divideFixed,
    multiplyFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import {
    latestSupplierQuotes,
    quoteExclusion,
    quotePriceHint,
    type InternalQuote,
} from "../lib/internal-quotes"
import type { InquiryLine } from "../types"

function computeCompactPercent(
    price: string,
    min: string,
    max: string,
): { kind: "below" | "equal" | "above"; label: string } | null {
    try {
        const below = compareDecimal(price, min, 6) < 0
        const above = compareDecimal(price, max, 6) > 0
        const base = above ? max : min
        const delta = compactFixed(
            subtractFixed(below ? base : price, below ? price : base, {
                maxScale: 6,
                outputScale: 6,
            }),
        )
        if (compareDecimal(delta, "0", 6) === 0) {
            return { kind: "equal", label: "持平" }
        }
        if (compareDecimal(base, "0", 6) === 0) {
            return {
                kind: below ? "below" : "above",
                label: `${below ? "-" : "+"}¥${delta}`,
            }
        }
        const pct = compactFixed(
            divideFixed(
                multiplyFixed(delta, "100", {
                    leftMaxScale: 6,
                    rightMaxScale: 0,
                    outputScale: 6,
                }),
                base,
                {
                    numeratorMaxScale: 6,
                    denominatorMaxScale: 6,
                    outputScale: 1,
                },
            ),
        )
        return {
            kind: below ? "below" : "above",
            label: `${below ? "-" : "+"}${pct}%`,
        }
    } catch {
        return null
    }
}

/**
 * 含税供给单价下方的统一交互式供应商报价参考条：
 * 将参考单价区间、供应商家数、运费核对状态、加价/倒挂差额与「打开供应商报价弹窗」
 * 收敛为单一可点击触发条，避免 4 行冗余文字堆叠。
 */
export function InquiryQuoteHint({
    id,
    quotes,
    line,
    round,
    price,
    tax,
    pending,
    failed,
    disabled = false,
    unavailable = false,
    onOpenQuotes,
}: {
    id: string
    quotes: InternalQuote[]
    line: InquiryLine
    round: number
    price?: string | null
    tax?: string | null
    pending: boolean
    failed: boolean
    disabled?: boolean
    unavailable?: boolean
    onOpenQuotes: () => void
}) {
    const current = latestSupplierQuotes(quotes).filter(
        (quote) => quote.line_id === line.line_id,
    )
    const uniqueSuppliers = new Set(current.map((item) => item.supplier_id))
        .size

    if (unavailable) {
        return (
            <div className="flex justify-end">
                <button
                    id={id}
                    type="button"
                    disabled={disabled}
                    aria-label={`${line.product_name}供应商报价 · 仅采购可见`}
                    aria-haspopup="dialog"
                    className="inline-flex h-5 cursor-pointer items-center gap-0.5 rounded-sm px-1 text-xs text-muted-foreground transition-colors hover:bg-sidebar-accent/70 hover:text-primary disabled:pointer-events-none disabled:opacity-50"
                    onClick={onOpenQuotes}
                >
                    <span>
                        {uniqueSuppliers > 0
                            ? `查看报价 (${uniqueSuppliers})`
                            : "登记报价"}
                    </span>
                    <ChevronRightIcon
                        aria-hidden="true"
                        className="size-3 shrink-0"
                    />
                </button>
            </div>
        )
    }

    if (failed) {
        return (
            <div className="flex justify-end">
                <button
                    id={id}
                    type="button"
                    disabled={disabled}
                    aria-label={`${line.product_name}供应商报价 · 仅采购可见`}
                    aria-haspopup="dialog"
                    className="inline-flex cursor-pointer items-center gap-0.5 rounded-sm px-1 py-0.5 text-xs text-destructive hover:bg-destructive-soft/60 disabled:pointer-events-none disabled:opacity-50"
                    onClick={onOpenQuotes}
                >
                    <span>报价参考读取失败 · 手动查看</span>
                    <ChevronRightIcon
                        aria-hidden="true"
                        className="size-3 shrink-0"
                    />
                </button>
            </div>
        )
    }

    if (pending) {
        return (
            <div className="flex justify-end">
                <button
                    id={id}
                    type="button"
                    disabled={disabled}
                    aria-label={`${line.product_name}供应商报价 · 仅采购可见`}
                    aria-haspopup="dialog"
                    className="inline-flex h-5 cursor-pointer items-center gap-1 rounded-sm px-1 text-xs text-muted-foreground hover:bg-sidebar-accent/70 hover:text-primary disabled:pointer-events-none disabled:opacity-50"
                    onClick={onOpenQuotes}
                >
                    读取报价参考…
                </button>
            </div>
        )
    }

    if (current.length === 0) {
        return (
            <div className="flex justify-end">
                <button
                    id={id}
                    type="button"
                    disabled={disabled}
                    aria-label={`${line.product_name}登记供应商报价 · 仅采购可见`}
                    aria-haspopup="dialog"
                    className="inline-flex h-5 cursor-pointer items-center gap-0.5 rounded-sm px-1 text-xs text-muted-foreground transition-colors hover:bg-sidebar-accent/70 hover:text-primary disabled:pointer-events-none disabled:opacity-50"
                    onClick={onOpenQuotes}
                >
                    <PlusIcon aria-hidden="true" className="size-3 shrink-0" />
                    <span>登记供应商报价</span>
                </button>
            </div>
        )
    }

    if (!tax) {
        return (
            <div className="flex justify-end">
                <button
                    id={id}
                    type="button"
                    disabled={disabled}
                    aria-label={`${line.product_name}供应商报价 · 仅采购可见`}
                    aria-haspopup="dialog"
                    className="inline-flex h-5 cursor-pointer items-center gap-0.5 rounded-sm px-1 text-xs text-muted-foreground transition-colors hover:bg-sidebar-accent/70 hover:text-primary disabled:pointer-events-none disabled:opacity-50"
                    onClick={onOpenQuotes}
                >
                    <span>{uniqueSuppliers} 家报价 · 填税率后比价</span>
                    <ChevronRightIcon
                        aria-hidden="true"
                        className="size-3 shrink-0"
                    />
                </button>
            </div>
        )
    }

    let comparable: InternalQuote[]
    const exclusions = new Set<string>()
    try {
        comparable = current.filter((quote) => {
            const reason = quoteExclusion(quote, line, round, tax)
            if (reason) exclusions.add(reason)
            return !reason
        })
    } catch {
        return (
            <div className="flex justify-end">
                <button
                    id={id}
                    type="button"
                    disabled={disabled}
                    aria-label={`${line.product_name}供应商报价 · 仅采购可见`}
                    aria-haspopup="dialog"
                    className="inline-flex h-5 cursor-pointer items-center gap-0.5 rounded-sm px-1 text-xs text-muted-foreground hover:bg-sidebar-accent/70 hover:text-primary disabled:pointer-events-none disabled:opacity-50"
                    onClick={onOpenQuotes}
                >
                    <span>核对税率后比价</span>
                    <ChevronRightIcon
                        aria-hidden="true"
                        className="size-3 shrink-0"
                    />
                </button>
            </div>
        )
    }

    const hint = quotePriceHint(comparable, price, line.quantity)
    if (!hint) {
        const exclusionText = [...exclusions].join("；")
        return (
            <div className="flex justify-end">
                <Tooltip>
                    <TooltipTrigger
                        render={
                            <button
                                id={id}
                                type="button"
                                disabled={disabled}
                                aria-label={`${line.product_name}供应商报价（${uniqueSuppliers} 家暂不可比：${exclusionText}）· 仅采购可见`}
                                aria-haspopup="dialog"
                                className="group inline-flex max-w-full cursor-pointer flex-col items-end rounded-md px-1.5 py-0.5 text-right transition-colors hover:bg-sidebar-accent/70 disabled:pointer-events-none disabled:opacity-50"
                                onClick={onOpenQuotes}
                            />
                        }
                    >
                        <span className="inline-flex max-w-full items-center gap-0.5 text-xs text-muted-foreground group-hover:text-primary">
                            <span className="truncate">
                                {uniqueSuppliers} 家暂不可比
                            </span>
                            <ChevronRightIcon
                                aria-hidden="true"
                                className="size-3 shrink-0"
                            />
                        </span>
                        {exclusionText ? (
                            <span className="max-w-full truncate text-tiny text-muted-foreground/80">
                                {exclusionText}
                            </span>
                        ) : null}
                    </TooltipTrigger>
                    <TooltipContent side="bottom" align="end">
                        <div className="space-y-0.5">
                            <p>
                                已有 {uniqueSuppliers} 家报价暂不可比：
                                {exclusionText}
                            </p>
                            <p className="text-muted-foreground">
                                点击查看或调整供应商报价
                            </p>
                        </div>
                    </TooltipContent>
                </Tooltip>
            </div>
        )
    }

    const comparableSuppliers = new Set(
        comparable.map((item) => item.supplier_id),
    ).size
    const priceRange =
        hint.min === hint.max ? `¥${hint.min}` : `¥${hint.min}–${hint.max}`
    const freightPending = comparable.some((item) => item.system_price_kind)
    const compactDelta = price
        ? computeCompactPercent(price, hint.min, hint.max)
        : null

    return (
        <div className="flex justify-end" aria-live="polite">
            <Tooltip>
                <TooltipTrigger
                    render={
                        <button
                            id={id}
                            type="button"
                            disabled={disabled}
                            aria-label={`${line.product_name}供应商报价 · 参考 ${priceRange}，${comparableSuppliers} 家 · 仅采购可见`}
                            aria-haspopup="dialog"
                            className="group -mr-1.5 inline-flex max-w-full cursor-pointer flex-col items-end gap-0.5 rounded-md px-1.5 py-1 text-right transition-colors hover:bg-sidebar-accent/70 focus-visible:outline-2 focus-visible:outline-primary disabled:pointer-events-none disabled:opacity-50"
                            onClick={onOpenQuotes}
                        />
                    }
                >
                    <span className="inline-flex max-w-full items-center justify-end gap-1.5 text-xs leading-4">
                        <span className="num truncate font-medium text-foreground/80 group-hover:text-primary">
                            参考 {priceRange}
                        </span>
                        {compactDelta ? (
                            <span
                                className={cn(
                                    "num shrink-0 rounded-sm px-1 text-tiny leading-4 font-medium",
                                    compactDelta.kind === "below"
                                        ? "bg-warning-soft text-warning-soft-foreground"
                                        : compactDelta.kind === "equal"
                                          ? "bg-success-soft text-success-soft-foreground"
                                          : "bg-muted text-muted-foreground",
                                )}
                            >
                                {compactDelta.label}
                            </span>
                        ) : null}
                        <ChevronRightIcon
                            aria-hidden="true"
                            className="size-3 shrink-0 text-muted-foreground/70 transition-transform group-hover:translate-x-0.5 group-hover:text-primary"
                        />
                    </span>
                    <span className="num text-tiny leading-4 text-muted-foreground/80">
                        {comparableSuppliers} 家
                        {freightPending ? " · 运费待核" : " · 含运费"}
                    </span>
                </TooltipTrigger>
                <TooltipContent side="bottom" align="end">
                    <div className="space-y-0.5">
                        <p className="num font-medium">
                            参考单价 {priceRange}（{comparableSuppliers} 家
                            {freightPending ? " · 运费待核" : " · 含运费"}）
                        </p>
                        {hint.message ? (
                            <p
                                className={cn(
                                    "num",
                                    hint.below &&
                                        "text-warning-soft-foreground",
                                )}
                            >
                                {hint.message}
                            </p>
                        ) : null}
                        <p className="text-muted-foreground">
                            点击查看、登记或采纳供应商报价（仅采购可见）
                        </p>
                    </div>
                </TooltipContent>
            </Tooltip>
        </div>
    )
}
