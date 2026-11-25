"use client"

import type { KeyboardEvent, KeyboardEventHandler, ReactNode } from "react"

import { useFieldA11y } from "@/components/form/field-a11y"
import { FieldFrame } from "@/components/form/field-frame"
import { InputGroupTextarea } from "@/components/ui/input-group"
import { Kbd } from "@/components/ui/kbd"
import { Textarea } from "@/components/ui/textarea"
import { cn } from "@/lib/utils"
import {
    BPM_REASON_MAX_UTF8_BYTES,
    formatByteCounterText,
    formatCharCounterText,
    utf8ByteLength,
} from "@/lib/validation/reason-length"

export type TextareaCounterMode = "utf8-bytes" | "chars"

export type TextareaFieldProps = {
    label: string
    hideLabel?: boolean
    inInputGroup?: boolean
    description?: ReactNode
    placeholder?: string
    disabled?: boolean
    required?: boolean
    rows?: number
    maxLength?: number
    /** 开启实时余量计数：按 UTF-8 字节或 Unicode 字符数统计（85% 预警）。 */
    counterMode?: TextareaCounterMode
    /** 计数上限；`utf8-bytes` 默认 512 字节，`chars` 默认复用 `maxLength`。 */
    counterMax?: number
    /** 支持按 Ctrl/Cmd + Enter 快捷提交（自动避开输入法合成态）。 */
    onCtrlEnterSubmit?: () => void
    /** 是否展示快捷键提示；传入 `onCtrlEnterSubmit` 时默认展示。 */
    showShortcutHint?: boolean
    className?: string
    textareaClassName?: string
    id?: string
    onKeyDown?: KeyboardEventHandler<HTMLTextAreaElement>
}

/**
 * 绑定 TanStack Form field 的多行文本（shadcn Field + Textarea）。
 * 通过 `form.AppField` → `field.TextareaField` 使用。
 */
export function TextareaField({
    label,
    hideLabel = false,
    inInputGroup = false,
    description,
    placeholder,
    disabled,
    required,
    rows,
    maxLength,
    counterMode,
    counterMax,
    onCtrlEnterSubmit,
    showShortcutHint,
    className,
    textareaClassName,
    id,
    onKeyDown,
}: TextareaFieldProps) {
    const {
        field,
        isInvalid,
        errors,
        resolvedId,
        descriptionId,
        errorId,
        describedBy,
    } = useFieldA11y<string>(description, id)
    const Control = inInputGroup ? InputGroupTextarea : Textarea
    const value = field.state.value ?? ""

    const resolvedCounterMax =
        counterMode === "utf8-bytes"
            ? (counterMax ?? BPM_REASON_MAX_UTF8_BYTES)
            : counterMode === "chars"
              ? (counterMax ?? maxLength)
              : undefined
    const showCounter = Boolean(
        counterMode && resolvedCounterMax && resolvedCounterMax > 0,
    )
    const used = showCounter
        ? counterMode === "utf8-bytes"
            ? utf8ByteLength(value)
            : Array.from(value).length
        : 0
    const overLimit = Boolean(
        showCounter && resolvedCounterMax && used > resolvedCounterMax,
    )
    const nearLimit = Boolean(
        showCounter &&
        resolvedCounterMax &&
        !overLimit &&
        used >= Math.ceil(resolvedCounterMax * 0.85),
    )
    const counterText =
        showCounter && resolvedCounterMax
            ? counterMode === "utf8-bytes"
                ? formatByteCounterText(value, resolvedCounterMax)
                : formatCharCounterText(value, resolvedCounterMax)
            : undefined
    const counterId = `${resolvedId}-counter`
    const showShortcut = Boolean(
        onCtrlEnterSubmit && (showShortcutHint ?? true),
    )
    const mergedDescribedBy = [describedBy, showCounter ? counterId : undefined]
        .filter(Boolean)
        .join(" ")
    const effectiveInvalid = isInvalid || overLimit

    function handleKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
        onKeyDown?.(event)
        if (
            event.defaultPrevented ||
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

    return (
        <FieldFrame
            className={cn(className)}
            label={label}
            labelClassName={hideLabel ? "sr-only" : undefined}
            htmlFor={resolvedId}
            required={required}
            description={description}
            descriptionId={descriptionId}
            isInvalid={effectiveInvalid}
            disabled={disabled}
            errorId={errorId}
            errors={errors}
            afterDescription={
                showCounter || showShortcut ? (
                    <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
                        <div className="min-w-0 flex-1">
                            {showShortcut ? (
                                <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
                                    <span>按</span>
                                    <Kbd>Ctrl/⌘</Kbd>
                                    <span>+</span>
                                    <Kbd>Enter</Kbd>
                                    <span>提交</span>
                                </span>
                            ) : null}
                        </div>
                        {showCounter ? (
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
                        ) : null}
                    </div>
                ) : null
            }
        >
            <Control
                id={resolvedId}
                name={field.name}
                value={value}
                placeholder={placeholder}
                disabled={disabled}
                rows={rows}
                maxLength={maxLength}
                aria-invalid={effectiveInvalid || undefined}
                aria-describedby={mergedDescribedBy || undefined}
                className={cn(
                    inInputGroup && "min-h-control",
                    textareaClassName,
                )}
                onKeyDown={handleKeyDown}
                onBlur={field.handleBlur}
                onChange={(e) => field.handleChange(e.target.value)}
            />
        </FieldFrame>
    )
}
