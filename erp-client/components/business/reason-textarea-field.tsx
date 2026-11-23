"use client"

import type { KeyboardEvent, Ref } from "react"

import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import {
    BPM_REASON_MAX_UTF8_BYTES,
    formatByteCounterText,
    formatCharCounterText,
    utf8ByteLength,
} from "@/lib/validation/reason-length"

export type ReasonTextareaMode = "utf8-bytes" | "chars"

export type ReasonTextareaFieldProps = {
    mode: ReasonTextareaMode
    max?: number
    value: string
    onChange: (value: string) => void
    onBlur?: () => void
    placeholder?: string
    rows?: number
    disabled?: boolean
    id?: string
    name?: string
    label?: string
    required?: boolean
    description?: string
    error?: string
    invalid?: boolean
    textareaRef?: Ref<HTMLTextAreaElement>
    "aria-label"?: string
    "aria-describedby"?: string
    className?: string
    textareaClassName?: string
    onCtrlEnterSubmit?: () => void
    quickPhrases?: readonly string[]
}

const TRAILING_SEPARATOR_PATTERN = /[；;。，,、\s]$/

/**
 * 统一原因输入框：内置 UTF-8 字节或字符实时余量计数、快捷短语与 Ctrl/Cmd+Enter 提交。
 */
export function ReasonTextareaField({
    mode,
    max,
    value,
    onChange,
    onBlur,
    placeholder,
    rows = 3,
    disabled = false,
    id,
    name,
    label,
    required = false,
    description,
    error,
    invalid = false,
    textareaRef,
    "aria-label": ariaLabel,
    "aria-describedby": externalDescribedBy,
    className,
    textareaClassName,
    onCtrlEnterSubmit,
    quickPhrases,
}: ReasonTextareaFieldProps) {
    const resolvedMax = max ?? BPM_REASON_MAX_UTF8_BYTES
    const used =
        mode === "utf8-bytes" ? utf8ByteLength(value) : Array.from(value).length
    const overLimit = used > resolvedMax
    const nearLimit =
        !overLimit && resolvedMax > 0 && used >= Math.ceil(resolvedMax * 0.85)
    const counterText =
        mode === "utf8-bytes"
            ? formatByteCounterText(value, resolvedMax)
            : formatCharCounterText(value, resolvedMax)

    const isInvalid = Boolean(invalid || error || overLimit)
    const descriptionId = id && description ? `${id}-description` : undefined
    const counterId = id ? `${id}-counter` : undefined
    const errorId = id && error ? `${id}-error` : undefined
    const describedBy = [externalDescribedBy, descriptionId, counterId, errorId]
        .filter(Boolean)
        .join(" ")

    const uniquePhrases = quickPhrases
        ? [
              ...new Set(
                  quickPhrases.map((phrase) => phrase.trim()).filter(Boolean),
              ),
          ]
        : []
    const phraseIdPrefix = id ? `${id}-phrase` : "reason-textarea-phrase"

    function handleKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
        if (
            !onCtrlEnterSubmit ||
            disabled ||
            event.nativeEvent.isComposing ||
            event.keyCode === 229
        ) {
            return
        }
        if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
            event.preventDefault()
            onCtrlEnterSubmit()
        }
    }

    function applyQuickPhrase(phrase: string) {
        if (disabled) return
        const current = value.trim()
        if (!current) {
            onChange(phrase)
            return
        }
        if (current.includes(phrase)) return
        const separator = TRAILING_SEPARATOR_PATTERN.test(current) ? "" : "；"
        onChange(`${current}${separator}${phrase}`)
    }

    return (
        <div className={cn("flex w-full flex-col gap-2", className)}>
            {label ? (
                <Label htmlFor={id} className="gap-1">
                    <span>{label}</span>
                    {required ? (
                        <span aria-hidden="true" className="text-destructive">
                            *
                        </span>
                    ) : null}
                </Label>
            ) : null}

            {uniquePhrases.length > 0 ? (
                <div
                    className="flex flex-wrap items-center gap-1.5"
                    role="group"
                    aria-label="快捷短语"
                >
                    {uniquePhrases.map((phrase) => (
                        <Button
                            key={phrase}
                            id={`${phraseIdPrefix}-${toAutomationIdSegment(encodeURIComponent(phrase))}`}
                            type="button"
                            variant="outline"
                            size="xs"
                            disabled={disabled}
                            onClick={() => applyQuickPhrase(phrase)}
                            className="text-xs font-normal text-muted-foreground hover:text-foreground"
                        >
                            {phrase}
                        </Button>
                    ))}
                </div>
            ) : null}

            <Textarea
                ref={textareaRef}
                id={id}
                name={name}
                value={value}
                placeholder={placeholder}
                rows={rows}
                disabled={disabled}
                aria-label={ariaLabel}
                aria-invalid={isInvalid || undefined}
                aria-describedby={describedBy || undefined}
                className={textareaClassName}
                onChange={(event) => onChange(event.target.value)}
                onBlur={onBlur}
                onKeyDown={handleKeyDown}
            />

            {description ? (
                <p id={descriptionId} className="text-xs text-muted-foreground">
                    {description}
                </p>
            ) : null}

            <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
                <div className="min-w-0 flex-1">
                    {error ? (
                        <p
                            id={errorId}
                            role="alert"
                            className="text-xs text-destructive"
                        >
                            {error}
                        </p>
                    ) : onCtrlEnterSubmit ? (
                        <span className="text-xs text-muted-foreground">
                            按 Ctrl/⌘ + Enter 快速提交
                        </span>
                    ) : null}
                </div>
                <span
                    id={counterId}
                    aria-live="polite"
                    className={cn(
                        "num shrink-0 text-xs",
                        overLimit
                            ? "font-medium text-destructive"
                            : nearLimit
                              ? "text-warning-soft-foreground"
                              : "text-muted-foreground",
                    )}
                >
                    {counterText}
                </span>
            </div>
        </div>
    )
}
