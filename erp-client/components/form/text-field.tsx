"use client"

import { Input } from "@/components/ui/input"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { useFieldA11y } from "@/components/form/field-a11y"
import { cn } from "@/lib/utils"
import { describeMoney } from "@/lib/money-display"

type TextFieldProps = {
    label: string
    hideLabel?: boolean
    description?: string
    placeholder?: string
    type?: React.ComponentProps<"input">["type"]
    disabled?: boolean
    required?: boolean
    className?: string
    inputClassName?: string
    autoComplete?: string
    inputMode?: React.ComponentProps<"input">["inputMode"]
    min?: React.ComponentProps<"input">["min"]
    max?: React.ComponentProps<"input">["max"]
    step?: React.ComponentProps<"input">["step"]
    testId?: string
    id?: string
    onBlur?: React.FocusEventHandler<HTMLInputElement>
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
    const moneyHintId = `${resolvedId}-money-hint`
    const moneyHint = money ? describeMoney(field.state.value)?.readout : null
    const describedBy = [
        ariaDescribedBy,
        description ? descriptionId : undefined,
        isInvalid ? errorId : undefined,
        moneyHint ? moneyHintId : undefined,
    ]
        .filter(Boolean)
        .join(" ")

    return (
        <Field data-invalid={isInvalid || undefined} className={cn(className)}>
            <FieldLabel
                htmlFor={resolvedId}
                className={hideLabel ? "sr-only" : undefined}
            >
                {label}
                {required ? <span className="text-destructive">*</span> : null}
            </FieldLabel>
            <Input
                id={resolvedId}
                name={field.name}
                type={type}
                value={field.state.value ?? ""}
                placeholder={placeholder}
                disabled={disabled}
                autoComplete={autoComplete}
                inputMode={inputMode}
                min={min}
                max={max}
                step={step}
                data-testid={testId}
                aria-invalid={isInvalid || undefined}
                aria-describedby={describedBy || undefined}
                className={inputClassName}
                onBlur={(event) => {
                    field.handleBlur()
                    onBlur?.(event)
                }}
                onChange={(e) => field.handleChange(e.target.value)}
            />
            {description ? (
                <FieldDescription id={descriptionId}>
                    {description}
                </FieldDescription>
            ) : null}
            {moneyHint ? (
                <FieldDescription id={moneyHintId}>
                    {moneyHint}
                </FieldDescription>
            ) : null}
            {isInvalid ? <FieldError id={errorId} errors={errors} /> : null}
        </Field>
    )
}
