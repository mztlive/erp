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
    className,
    textareaClassName,
}: TextareaFieldProps) {
    const field = useFieldContext<string>()
    const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid
    const errors = toFieldErrors(field.state.meta.errors)

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
                aria-invalid={isInvalid || undefined}
                className={textareaClassName}
                onBlur={field.handleBlur}
                onChange={(e) => field.handleChange(e.target.value)}
            />
            {description ? (
                <FieldDescription>{description}</FieldDescription>
            ) : null}
            {isInvalid ? <FieldError errors={errors} /> : null}
        </Field>
    )
}
