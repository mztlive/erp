"use client"

import * as React from "react"
import { StatusBadge } from "@/components/ui/status-badge"
import { cn } from "@/lib/utils"
import {
    INQUIRY_CONCLUSION_LABEL,
    type InquiryConclusion,
    type InquiryFeedbackLine,
    type InquiryLine,
} from "../types"

type SelectableConclusion = Exclude<InquiryConclusion, "pending">

const CONCLUSION_OPTIONS: readonly {
    value: SelectableConclusion
    shortLabel: string
    fullLabel: string
    idSuffix: string
    dotClass: string
    activeClass: string
}[] = [
    {
        value: "available",
        shortLabel: "可供",
        fullLabel: "可供应（全量）",
        idSuffix: "available",
        dotClass: "bg-success",
        activeClass:
            "bg-card font-medium text-success-soft-foreground shadow-2xs ring-1 ring-success-border/90",
    },
    {
        value: "partially_available",
        shortLabel: "部分",
        fullLabel: "部分可供应",
        idSuffix: "partially-available",
        dotClass: "bg-warning",
        activeClass:
            "bg-card font-medium text-warning-soft-foreground shadow-2xs ring-1 ring-warning-border/90",
    },
    {
        value: "unavailable",
        shortLabel: "不供",
        fullLabel: "不供应",
        idSuffix: "unavailable",
        dotClass: "bg-muted-foreground/70",
        activeClass:
            "bg-card font-medium text-foreground shadow-2xs ring-1 ring-border",
    },
]

function conclusionTone(conclusion: InquiryConclusion) {
    switch (conclusion) {
        case "available":
            return "success" as const
        case "partially_available":
            return "warning" as const
        case "unavailable":
            return "neutral" as const
        default:
            return "info" as const
    }
}

/**
 * 采购反馈「供给结论」单元格：
 * - 编辑态采用一键三段式分段开关（可供 / 部分 / 不供），减少弹窗点击并一眼可扫；
 * - 只读态展示统一语义 StatusBadge；
 * - 缺货 / 不可供原因由表格行下方的全宽附属原因条承载，避免窄列堆叠失衡。
 */
export function InquiryFeedbackConclusionCell({
    readOnly,
    compact = false,
    prefix,
    value,
    line,
    disabled,
    conclusionError,
    onChangeConclusion,
}: {
    readOnly: boolean
    compact?: boolean
    prefix: string
    value: InquiryFeedbackLine
    line: InquiryLine
    disabled: boolean
    conclusionError?: string
    onChangeConclusion: (conclusion: InquiryConclusion) => void
}) {
    if (readOnly) {
        return (
            <div className="flex min-h-control-sm items-center">
                <StatusBadge
                    label={INQUIRY_CONCLUSION_LABEL[value.conclusion]}
                    tone={conclusionTone(value.conclusion)}
                />
            </div>
        )
    }

    const activeIndex = CONCLUSION_OPTIONS.findIndex(
        (option) => option.value === value.conclusion,
    )

    const handleKeyDown = (
        event: React.KeyboardEvent<HTMLButtonElement>,
        index: number,
    ) => {
        if (disabled) return
        let nextIndex = -1
        if (event.key === "ArrowRight" || event.key === "ArrowDown") {
            nextIndex = (index + 1) % CONCLUSION_OPTIONS.length
        } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
            nextIndex =
                (index - 1 + CONCLUSION_OPTIONS.length) %
                CONCLUSION_OPTIONS.length
        }
        if (nextIndex < 0) return
        event.preventDefault()
        const target = CONCLUSION_OPTIONS[nextIndex]
        onChangeConclusion(target.value)
        requestAnimationFrame(() => {
            document
                .getElementById(`${prefix}-conclusion-${target.idSuffix}`)
                ?.focus()
        })
    }

    return (
        <div className="space-y-1">
            <div
                id={`${prefix}-conclusion`}
                role="radiogroup"
                tabIndex={-1}
                aria-label={`${line.product_name}供给结论`}
                aria-invalid={Boolean(conclusionError) || undefined}
                aria-describedby={
                    conclusionError ? `${prefix}-conclusion-error` : undefined
                }
                className={cn(
                    "grid h-control-sm w-full grid-cols-3 items-center gap-0.5 rounded-md bg-muted/60 p-0.5 transition-colors",
                    compact && "bg-surface-sunken",
                    conclusionError &&
                        "bg-destructive-soft/50 ring-1 ring-destructive/65",
                    disabled && "opacity-60",
                )}
            >
                {CONCLUSION_OPTIONS.map((option, index) => {
                    const active = value.conclusion === option.value
                    const tabbable =
                        activeIndex === -1 ? index === 0 : activeIndex === index
                    return (
                        <button
                            key={option.value}
                            id={`${prefix}-conclusion-${option.idSuffix}`}
                            type="button"
                            role="radio"
                            aria-checked={active}
                            aria-label={`${line.product_name}${option.fullLabel}`}
                            title={option.fullLabel}
                            disabled={disabled}
                            tabIndex={tabbable ? 0 : -1}
                            className={cn(
                                "inline-flex h-full min-w-0 cursor-pointer items-center justify-center gap-1 rounded-sm px-1.5 text-xs transition-all outline-none select-none",
                                "focus-visible:ring-2 focus-visible:ring-primary/50",
                                compact && "gap-0.5 px-0.5 text-tiny",
                                active
                                    ? option.activeClass
                                    : "font-normal text-muted-foreground hover:text-foreground",
                                disabled && "cursor-not-allowed",
                            )}
                            onKeyDown={(event) => handleKeyDown(event, index)}
                            onClick={() => {
                                if (!active) onChangeConclusion(option.value)
                            }}
                        >
                            {active ? (
                                <span
                                    aria-hidden="true"
                                    className={cn(
                                        "size-1.5 shrink-0 rounded-full",
                                        option.dotClass,
                                    )}
                                />
                            ) : null}
                            <span className="truncate">
                                {option.shortLabel}
                            </span>
                        </button>
                    )
                })}
            </div>
            {conclusionError ? (
                <p
                    id={`${prefix}-conclusion-error`}
                    className="text-xs text-destructive"
                >
                    {conclusionError}
                </p>
            ) : value.conclusion === "pending" ? (
                <p className="flex items-center gap-1.5 text-xs text-warning-soft-foreground">
                    <span
                        aria-hidden="true"
                        className="size-1.5 shrink-0 rounded-full bg-warning"
                    />
                    {compact ? "待回复" : "待确认结论"}
                </p>
            ) : null}
        </div>
    )
}
