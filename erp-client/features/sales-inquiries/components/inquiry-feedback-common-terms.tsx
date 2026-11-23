"use client"

import { InquiryTaxRateInput } from "./inquiry-tax-rate-input"
import { Label } from "@/components/ui/label"
import { cn } from "@/lib/utils"
import type { FeedbackTerms } from "../lib/feedback-editing"
import type { InquiryFeedbackLine } from "../types"

function resolveUniformTaxRate(lines: readonly InquiryFeedbackLine[]): {
    state: "unset" | "uniform" | "mixed"
    value: string
} {
    const active = lines.filter((line) => line.conclusion !== "unavailable")
    if (active.length === 0) return { state: "unset", value: "" }
    const rates = active.map((line) => line.prices[0]?.tax_rate ?? "")
    const nonEmpty = rates.filter(Boolean)
    if (nonEmpty.length === 0) return { state: "unset", value: "" }
    const first = rates[0]
    if (first && rates.every((rate) => rate === first)) {
        return { state: "uniform", value: first }
    }
    return { state: "mixed", value: "" }
}

export function InquiryFeedbackCommonTerms({
    lines,
    disabled,
    onChange,
}: {
    lines: InquiryFeedbackLine[]
    disabled: boolean
    onChange: (patch: Partial<FeedbackTerms>) => void
}) {
    const uniform = resolveUniformTaxRate(lines)
    const inactive =
        disabled || lines.every((line) => line.conclusion === "unavailable")
    const hasUniformValue = uniform.state === "uniform"
    return (
        <div
            role="group"
            aria-label="统一报价条件"
            aria-describedby="procurement-feedback-common-hint"
            className="flex max-w-full flex-wrap items-center"
        >
            <span id="procurement-feedback-common-hint" className="sr-only">
                修改后应用到全部可供及待确认商品；个别差异可直接在商品行修改。
            </span>
            <div
                className={cn(
                    "inline-flex h-control-sm items-center gap-1.5 rounded-md border border-transparent bg-muted/65 pr-1 pl-2.5 transition-colors",
                    !inactive && "hover:bg-muted",
                    hasUniformValue &&
                        !inactive &&
                        "border-primary/25 bg-sidebar-accent/70 hover:bg-sidebar-accent",
                    "focus-within:border-primary/60 focus-within:bg-card focus-within:ring-[3px] focus-within:ring-ring",
                    inactive && "opacity-60",
                )}
            >
                <Label
                    htmlFor="procurement-feedback-common-tax"
                    className={cn(
                        "shrink-0 cursor-pointer text-xs font-normal",
                        hasUniformValue && !inactive
                            ? "text-sidebar-accent-foreground/80"
                            : "text-muted-foreground",
                    )}
                    title="填写后统一应用到全部非不供应商品"
                >
                    统一税率
                </Label>
                <InquiryTaxRateInput
                    id="procurement-feedback-common-tax"
                    className={cn(
                        "h-6 border-0 bg-transparent pl-1 pr-7 text-right text-body-compact shadow-none focus-visible:border-transparent focus-visible:ring-0",
                        uniform.state === "mixed" ? "w-24" : "w-18",
                        hasUniformValue &&
                            !inactive &&
                            "font-medium text-sidebar-accent-foreground",
                    )}
                    disabled={inactive}
                    inputMode="decimal"
                    aria-describedby="procurement-feedback-common-tax-hint procurement-feedback-common-hint"
                    title="填写百分数，例如 13 表示 13%，修改后应用到全部非不供应商品"
                    value={uniform.value}
                    placeholder={
                        uniform.state === "mixed" ? "各行不同" : "如 13"
                    }
                    onValueChange={(rate) =>
                        onChange({ tax_rate: rate || null })
                    }
                />
                <span
                    id="procurement-feedback-common-tax-hint"
                    className="sr-only"
                >
                    输入 13 表示 13%，最多 4 位小数
                </span>
            </div>
        </div>
    )
}
