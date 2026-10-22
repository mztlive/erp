"use client"

import { PencilIcon, Trash2Icon } from "lucide-react"
import { MoneyValue, RateValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { compactFixed, multiplyFixed } from "@/lib/fixed-decimal"
import { feedbackFeeIssues } from "../lib/feedback-fee-validation"
import type { InquiryFee } from "../types"

export function InquiryFeeSummary({
    fee,
    idPrefix,
    disabled,
    onEdit,
    onRemove,
}: {
    fee: InquiryFee
    idPrefix: string
    disabled: boolean
    onEdit: () => void
    onRemove: () => void
}) {
    const issues = feedbackFeeIssues([fee])
    const invalidAmount = issues.some((issue) => issue.field === "amount_gross")
    const invalidRate = issues.some((issue) => issue.field === "tax_rate")
    const rate = invalidRate
        ? null
        : compactFixed(
              multiplyFixed(fee.tax_rate.trim(), "100", {
                  leftMaxScale: 6,
                  rightMaxScale: 0,
                  outputScale: 4,
              }),
          )
    return (
        <div className="rounded-lg border bg-background px-4 py-3">
            <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
                <div className="min-w-0 flex-1">
                    <p className="break-words text-sm font-medium">
                        {fee.name.trim() || "未填写费用名称"}
                    </p>
                    <div className="mt-1 flex flex-wrap items-baseline gap-x-3 gap-y-1 text-xs text-muted-foreground">
                        <span>
                            含税金额{" "}
                            {invalidAmount ? (
                                "待填写"
                            ) : (
                                <MoneyValue
                                    value={fee.amount_gross.trim()}
                                    display="exact"
                                />
                            )}
                        </span>
                        <span>
                            税率{" "}
                            {rate === null ? (
                                "待填写"
                            ) : (
                                <RateValue
                                    value={rate}
                                    precision={rate.split(".")[1]?.length ?? 0}
                                />
                            )}
                        </span>
                        {issues.length > 0 && (
                            <span className="text-destructive">待完善</span>
                        )}
                    </div>
                </div>
                <div className="flex shrink-0 items-center gap-1">
                    <Button
                        id={`${idPrefix}-edit`}
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={disabled}
                        onClick={onEdit}
                    >
                        <PencilIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        编辑
                    </Button>
                    <Button
                        id={`${idPrefix}-remove`}
                        type="button"
                        size="sm"
                        variant="ghost"
                        className="text-muted-foreground hover:text-destructive"
                        disabled={disabled}
                        onClick={onRemove}
                    >
                        <Trash2Icon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        移除
                    </Button>
                </div>
            </div>
            {fee.public_note.trim() && (
                <p className="mt-2 whitespace-pre-wrap break-words text-xs text-muted-foreground">
                    {fee.public_note}
                </p>
            )}
        </div>
    )
}
