"use client"

import * as React from "react"
import { ChevronDownIcon, PercentIcon } from "lucide-react"
import { detailWorkspaceStyles } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import {
    Popover,
    PopoverContent,
    PopoverDescription,
    PopoverHeader,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { compactFixed, multiplyFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { FeedbackTerms } from "../lib/feedback-editing"
import type { InquiryFeedbackLine } from "../types"
import { InquiryTaxRateInput } from "./inquiry-tax-rate-input"

const COMMON_TAX_PRESETS: readonly { rate: string; label: string }[] = [
    { rate: "0.13", label: "13%" },
    { rate: "0.09", label: "9%" },
    { rate: "0.06", label: "6%" },
    { rate: "0.03", label: "3%" },
    { rate: "0.01", label: "1%" },
    { rate: "0", label: "0%" },
]

function displayPercentLabel(rate: string): string {
    if (!rate) return ""
    try {
        return `${compactFixed(
            multiplyFixed(rate, "100", {
                leftMaxScale: 6,
                rightMaxScale: 0,
                outputScale: 4,
            }),
        )}%`
    } catch {
        return rate
    }
}

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

/**
 * 统一税率批量设置 Popover：
 * - 工具栏内呈克制中性触发按钮，避免高亮蓝胶囊抢夺页面主视觉；
 * - 弹层内提供国内常用增值税率一键预设（13%/9%/6%/3%/1%/0%）与自定义输入。
 */
export function InquiryFeedbackCommonTerms({
    lines,
    disabled,
    onChange,
}: {
    lines: InquiryFeedbackLine[]
    disabled: boolean
    onChange: (patch: Partial<FeedbackTerms>) => void
}) {
    const [open, setOpen] = React.useState(false)
    const uniform = resolveUniformTaxRate(lines)
    const inactive =
        disabled || lines.every((line) => line.conclusion === "unavailable")
    const summaryLabel =
        uniform.state === "uniform"
            ? displayPercentLabel(uniform.value)
            : uniform.state === "mixed"
              ? "各行不同"
              : "未设"

    return (
        <Popover open={open} onOpenChange={setOpen}>
            <PopoverTrigger
                render={
                    <Button
                        id="procurement-feedback-common-tax-trigger"
                        type="button"
                        variant="ghost"
                        size="sm"
                        disabled={inactive}
                        className={cn(
                            detailWorkspaceStyles.subtleButton,
                            "gap-1.5",
                        )}
                        title="批量设置全部非不供应商品的税率"
                    />
                }
            >
                <PercentIcon
                    aria-hidden="true"
                    className="size-3.5 text-muted-foreground"
                />
                <span>统一税率</span>
                <span className="num font-medium text-foreground">
                    {summaryLabel}
                </span>
                <ChevronDownIcon
                    aria-hidden="true"
                    className="size-3.5 text-muted-foreground/80"
                />
            </PopoverTrigger>
            <PopoverContent align="end" className="w-72 gap-3 p-3.5">
                <PopoverHeader>
                    <PopoverTitle className="text-body-compact font-semibold">
                        统一设置税率
                    </PopoverTitle>
                    <PopoverDescription>
                        应用到全部可供及待确认商品，个别差异可直接在商品行修改。
                    </PopoverDescription>
                </PopoverHeader>
                <div className="grid grid-cols-3 gap-1.5">
                    {COMMON_TAX_PRESETS.map((preset) => {
                        const active =
                            uniform.state === "uniform" &&
                            uniform.value === preset.rate
                        return (
                            <button
                                key={preset.rate}
                                id={`procurement-feedback-common-tax-preset-${preset.rate.replace(".", "-")}`}
                                type="button"
                                disabled={inactive}
                                className={cn(
                                    "num h-control-sm cursor-pointer rounded-md border text-xs font-medium transition-colors",
                                    active
                                        ? "border-primary/40 bg-sidebar-accent text-sidebar-accent-foreground"
                                        : "border-border/70 bg-card text-foreground/85 hover:bg-muted/70",
                                )}
                                onClick={() => {
                                    onChange({ tax_rate: preset.rate })
                                    setOpen(false)
                                }}
                            >
                                {preset.label}
                            </button>
                        )
                    })}
                </div>
                <div className="space-y-1.5 border-t border-border/60 pt-2.5">
                    <Label
                        htmlFor="procurement-feedback-common-tax"
                        className="text-xs font-medium text-foreground/75"
                    >
                        自定义税率
                    </Label>
                    <InquiryTaxRateInput
                        id="procurement-feedback-common-tax"
                        className="h-control-sm num text-right"
                        disabled={inactive}
                        inputMode="decimal"
                        aria-describedby="procurement-feedback-common-tax-hint"
                        title="填写百分数，例如 13 表示 13%，修改后应用到全部非不供应商品"
                        value={uniform.value}
                        placeholder={
                            uniform.state === "mixed"
                                ? "当前各行不同，输入覆盖"
                                : "输入百分数，如 13"
                        }
                        onValueChange={(rate) =>
                            onChange({ tax_rate: rate || null })
                        }
                        onKeyDown={(event) => {
                            if (event.key === "Enter") {
                                event.preventDefault()
                                setOpen(false)
                            }
                        }}
                    />
                    <span
                        id="procurement-feedback-common-tax-hint"
                        className="sr-only"
                    >
                        输入 13 表示 13%，最多 4 位小数
                    </span>
                </div>
            </PopoverContent>
        </Popover>
    )
}
