"use client"

import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { FeedbackIssue } from "../lib/feedback-editing"
import type { InquiryFeedbackLine } from "../types"

export function InquiryFeedbackLineIssues({
    prefix,
    value,
    issues,
    onChange,
    onFocusIssue,
}: {
    prefix: string
    value: InquiryFeedbackLine
    issues: FeedbackIssue[]
    onChange: (value: InquiryFeedbackLine) => void
    onFocusIssue: (field: string) => void
}) {
    const unavailable = value.conclusion === "unavailable"
    const fields = [...new Set(issues.map((issue) => issue.field))]
    return (
        <div className="space-y-2">
            {(unavailable || value.conclusion === "partially_available") && (
                <div className="flex items-center gap-3">
                    <Label
                        className="shrink-0"
                        htmlFor={`${prefix}-public-note`}
                    >
                        {unavailable ? "不可供原因" : "缺货原因"}
                    </Label>
                    <Input
                        id={`${prefix}-public-note`}
                        value={value.public_note}
                        aria-required="true"
                        placeholder={
                            unavailable
                                ? "说明无法供应的原因，发布后销售可见"
                                : "说明缺货数量与原因，发布后销售可见"
                        }
                        onChange={(event) =>
                            onChange({
                                ...value,
                                public_note: event.target.value,
                            })
                        }
                    />
                </div>
            )}
            {issues.length > 0 && (
                <div
                    id={`${prefix}-issues`}
                    className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs"
                >
                    <span className="text-muted-foreground">待完善：</span>
                    {fields.map((field) => (
                        <button
                            key={field}
                            id={`${prefix}-issue-${toAutomationIdSegment(field)}`}
                            type="button"
                            className="text-left text-muted-foreground underline decoration-dotted underline-offset-4 hover:text-foreground"
                            onClick={() => onFocusIssue(field)}
                        >
                            {issues
                                .filter((issue) => issue.field === field)
                                .map((issue) => issue.message)
                                .join("；")}
                        </button>
                    ))}
                </div>
            )}
        </div>
    )
}
