"use client"

import { Input } from "@/components/ui/input"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { useFieldContext } from "@/components/form/form-context"
import { toFieldErrors } from "@/components/form/utils"
import { cn } from "@/lib/utils"

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
}: TextFieldProps) {
    const field = useFieldContext<string>()
    const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid
    const errors = toFieldErrors(field.state.meta.errors)
    const resolvedId = id ?? field.name
    const descriptionId = `${resolvedId}-description`
    const errorId = `${resolvedId}-error`
    const describedBy = [
        description ? descriptionId : undefined,
        isInvalid ? errorId : undefined,
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
                onBlur={field.handleBlur}
                onChange={(e) => field.handleChange(e.target.value)}
            />
            {description ? (
                <FieldDescription id={descriptionId}>
                    {description}
                </FieldDescription>
            ) : null}
            {isInvalid ? <FieldError id={errorId} errors={errors} /> : null}
        </Field>
    )
}
