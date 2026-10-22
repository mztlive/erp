"use client"

import { DatePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
    feedbackCommonTerms,
    type FeedbackTerms,
} from "../lib/feedback-editing"
import { inquiryDateInput, inquiryDateSeconds } from "../lib/format"
import type { InquiryFeedbackLine } from "../types"

export function InquiryFeedbackCommonTerms({
    lines,
    disabled,
    onChange,
}: {
    lines: InquiryFeedbackLine[]
    disabled: boolean
    onChange: (patch: Partial<FeedbackTerms>) => void
}) {
    const common = feedbackCommonTerms(lines)
    const inactive =
        disabled || lines.every((line) => line.conclusion === "unavailable")
    return (
        <div
            role="group"
            aria-label="统一报价条件"
            aria-describedby="procurement-feedback-common-hint"
            className="flex max-w-full flex-wrap items-center gap-x-4 gap-y-2"
        >
            <span id="procurement-feedback-common-hint" className="sr-only">
                修改后应用到全部可供及待确认商品；个别差异在行内“调整”。
            </span>
            <div className="flex items-center gap-2">
                <Label
                    htmlFor="procurement-feedback-common-tax"
                    className="shrink-0 text-xs text-muted-foreground"
                    title="修改后统一应用到全部可供及待确认商品"
                >
                    统一税率
                </Label>
                <Input
                    id="procurement-feedback-common-tax"
                    className="h-8 w-28 text-sm"
                    disabled={inactive}
                    inputMode="decimal"
                    aria-describedby="procurement-feedback-common-tax-hint procurement-feedback-common-hint"
                    title="填写小数，例如 0.13 表示 13%"
                    value={common.tax_rate ?? ""}
                    placeholder={
                        common.tax_rate === null ? "各商品不同" : "如 0.13"
                    }
                    onChange={(event) =>
                        onChange({ tax_rate: event.target.value || null })
                    }
                />
                <span
                    id="procurement-feedback-common-tax-hint"
                    className="sr-only"
                >
                    0.13 表示 13%
                </span>
            </div>
            <div className="flex items-center gap-2">
                <Label
                    htmlFor="procurement-feedback-common-valid"
                    className="shrink-0 text-xs text-muted-foreground"
                    title="修改后统一应用到全部可供及待确认商品"
                >
                    报价有效至
                </Label>
                <div className="w-44">
                    <DatePicker
                        id="procurement-feedback-common-valid"
                        disabled={inactive}
                        size="sm"
                        value={
                            inquiryDateInput(common.valid_until ?? 0) ||
                            undefined
                        }
                        placeholder={
                            common.valid_until === null
                                ? "各商品不同"
                                : "选择日期"
                        }
                        onValueChange={(date) =>
                            onChange({
                                valid_until: inquiryDateSeconds(date ?? ""),
                            })
                        }
                    />
                </div>
            </div>
        </div>
    )
}
