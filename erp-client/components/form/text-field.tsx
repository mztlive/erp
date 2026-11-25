"use client"

import type { ComponentProps, FocusEventHandler, ReactNode } from "react"

import { hasFieldDescription, useFieldA11y } from "@/components/form/field-a11y"
import { FieldFrame } from "@/components/form/field-frame"
import { FieldDescription } from "@/components/ui/field"
import { Input, type InputSize } from "@/components/ui/input"
import { describeMoney } from "@/lib/money-display"
import { cn } from "@/lib/utils"

export type TextFieldProps = {
    label: string
    hideLabel?: boolean
    description?: ReactNode
    placeholder?: string
    type?: ComponentProps<"input">["type"]
    size?: InputSize
    disabled?: boolean
    required?: boolean
    className?: string
    inputClassName?: string
    autoComplete?: string
    inputMode?: ComponentProps<"input">["inputMode"]
    min?: ComponentProps<"input">["min"]
    max?: ComponentProps<"input">["max"]
    step?: ComponentProps<"input">["step"]
    testId?: string
    id?: string
    onBlur?: FocusEventHandler<HTMLInputElement>
    "aria-describedby"?: string
    /** 金额仍以元录入；显示保留全部精度的中文辅助读数。 */
    money?: boolean
}

/**
 * 绑定 TanStack Form field 的文本输入（shadcn Field + Input）。
 * 通过 `form.AppField` → `field.TextField` 使用。
 */
export function TextField({
    label,
    hideLabel = false,
    description,
    placeholder,
    type = "text",
    size,
    disabled,
    required,
    className,
    inputClassName,
    autoComplete,
    inputMode,
    min,
    max,
    step,
    testId,
    id,
    onBlur,
    "aria-describedby": ariaDescribedBy,
    money = false,
}: TextFieldProps) {
    const { field, isInvalid, errors, resolvedId, descriptionId, errorId } =
        useFieldA11y<string>(description, id)
    const isNumericInput = money || type === "number"
    const moneyHintId = `${resolvedId}-money-hint`
    const moneyHint = money ? describeMoney(field.state.value)?.readout : null
    const describedBy = [
        ariaDescribedBy,
        hasFieldDescription(description) ? descriptionId : undefined,
        isInvalid ? errorId : undefined,
        moneyHint ? moneyHintId : undefined,
    ]
        .filter(Boolean)
        .join(" ")

    return (
        <FieldFrame
            className={cn(className)}
            label={label}
            labelClassName={hideLabel ? "sr-only" : undefined}
            htmlFor={resolvedId}
            required={required}
            description={description}
            descriptionId={descriptionId}
            isInvalid={isInvalid}
            disabled={disabled}
            errorId={errorId}
            errors={errors}
            afterDescription={
                moneyHint ? (
                    <FieldDescription id={moneyHintId}>
                        {moneyHint}
                    </FieldDescription>
                ) : null
            }
        >
            <Input
                id={resolvedId}
                name={field.name}
                type={type}
                size={size}
                value={field.state.value ?? ""}
                placeholder={placeholder}
                disabled={disabled}
                autoComplete={autoComplete}
                inputMode={
                    inputMode ?? (isNumericInput ? "decimal" : undefined)
                }
                min={min}
                max={max}
                step={step}
                data-testid={testId}
                aria-invalid={isInvalid || undefined}
                aria-describedby={describedBy || undefined}
                className={cn(isNumericInput && "num", inputClassName)}
                onBlur={(event) => {
                    field.handleBlur()
                    onBlur?.(event)
                }}
                onChange={(e) => field.handleChange(e.target.value)}
            />
        </FieldFrame>
    )
}
