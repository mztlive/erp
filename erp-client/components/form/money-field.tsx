"use client"

import type { ComponentProps, FocusEventHandler, ReactNode } from "react"

import { hasFieldDescription, useFieldA11y } from "@/components/form/field-a11y"
import { FieldFrame } from "@/components/form/field-frame"
import { FieldDescription } from "@/components/ui/field"
import { Input, type InputSize } from "@/components/ui/input"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { describeMoney } from "@/lib/money-display"
import { cn } from "@/lib/utils"

export type MoneyFieldProps = {
    label: string
    hideLabel?: boolean
    description?: ReactNode
    placeholder?: string
    size?: InputSize
    disabled?: boolean
    required?: boolean
    /** 可选货币前缀；传入 `true` 默认显示 `¥`，也可自定义符号如 `"$"`。 */
    prefix?: ReactNode | boolean
    /** 是否显示默认 `¥` 前缀（等价于 `prefix="¥"`）。 */
    showPrefix?: boolean
    /** 是否展示保留全部精度的中文辅助读数；默认 true。 */
    showReadout?: boolean
    align?: "start" | "end"
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
    onValueChange?: (value: string) => void
    "aria-describedby"?: string
}

/**
 * 绑定 TanStack Form field 的金额输入字段（整合 `MoneyInput` 与 `TextField(money)`）。
 * 默认启用 `.num` 等宽数字、`inputMode="decimal"`、中文全精度读数与可选 `¥` 前缀。
 */
export function MoneyField({
    label,
    hideLabel = false,
    description,
    placeholder,
    size = "default",
    disabled,
    required,
    prefix,
    showPrefix = false,
    showReadout = true,
    align = "start",
    className,
    inputClassName,
    autoComplete,
    inputMode = "decimal",
    min,
    max,
    step,
    testId,
    id,
    onBlur,
    onValueChange,
    "aria-describedby": ariaDescribedBy,
}: MoneyFieldProps) {
    const { field, isInvalid, errors, resolvedId, descriptionId, errorId } =
        useFieldA11y<string>(description, id)
    const value = field.state.value ?? ""
    const resolvedPrefix =
        prefix === true
            ? "¥"
            : prefix === false
              ? undefined
              : (prefix ?? (showPrefix ? "¥" : undefined))
    const moneyHintId = `${resolvedId}-money-hint`
    const moneyHint = showReadout ? describeMoney(value)?.readout : null
    const describedBy = [
        ariaDescribedBy,
        hasFieldDescription(description) ? descriptionId : undefined,
        isInvalid ? errorId : undefined,
        moneyHint ? moneyHintId : undefined,
    ]
        .filter(Boolean)
        .join(" ")

    const handleChange = (raw: string) => {
        const next = resolvedPrefix ? raw.replace(/[¥￥]/g, "") : raw
        field.handleChange(next)
        onValueChange?.(next)
    }

    const commonProps = {
        id: resolvedId,
        name: field.name,
        type: "text" as const,
        value,
        placeholder,
        disabled,
        autoComplete,
        inputMode,
        min,
        max,
        step,
        "data-testid": testId,
        "aria-invalid": isInvalid || undefined,
        "aria-describedby": describedBy || undefined,
        className: cn("num", align === "end" && "text-right", inputClassName),
        onBlur: (event: React.FocusEvent<HTMLInputElement>) => {
            field.handleBlur()
            onBlur?.(event)
        },
        onChange: (event: React.ChangeEvent<HTMLInputElement>) =>
            handleChange(event.target.value),
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
            {resolvedPrefix ? (
                <InputGroup
                    className={cn(
                        size === "sm" && "h-control-sm",
                        size === "lg" && "h-control-lg",
                    )}
                >
                    <InputGroupAddon
                        align="inline-start"
                        className="pr-0 text-muted-foreground/80"
                    >
                        {resolvedPrefix}
                    </InputGroupAddon>
                    <InputGroupInput {...commonProps} />
                </InputGroup>
            ) : (
                <Input {...commonProps} size={size} />
            )}
        </FieldFrame>
    )
}
