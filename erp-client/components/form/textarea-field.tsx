"use client"

import { Textarea } from "@/components/ui/textarea"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { useFieldContext } from "@/components/form/form-context"
import { toFieldErrors } from "@/components/form/utils"
import { cn } from "@/lib/utils"

type TextareaFieldProps = {
    label: string
    description?: string
    placeholder?: string
    disabled?: boolean
    rows?: number
    maxLength?: number
    className?: string
    textareaClassName?: string
}

/**
 * 绑定 TanStack Form field 的多行文本（shadcn Field + Textarea）。
 * 通过 `form.AppField` → `field.TextareaField` 使用。
 */
export function TextareaField({
    label,
    description,
    placeholder,
    disabled,
    rows,
    maxLength,
    className,
    textareaClassName,
}: TextareaFieldProps) {
    const field = useFieldContext<string>()
    const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid
    const errors = toFieldErrors(field.state.meta.errors)
    const descriptionId = `${field.name}-description`
    const errorId = `${field.name}-error`
    const describedBy = [
        description ? descriptionId : undefined,
        isInvalid ? errorId : undefined,
    ]
        .filter(Boolean)
        .join(" ")

    return (
        <Field data-invalid={isInvalid || undefined} className={cn(className)}>
            <FieldLabel htmlFor={field.name}>{label}</FieldLabel>
            <Textarea
                id={field.name}
                name={field.name}
                value={field.state.value ?? ""}
                placeholder={placeholder}
                disabled={disabled}
                rows={rows}
                maxLength={maxLength}
                aria-invalid={isInvalid || undefined}
                aria-describedby={describedBy || undefined}
                className={textareaClassName}
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
