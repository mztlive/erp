"use client"

import type { KeyboardEventHandler } from "react"
import { Textarea } from "@/components/ui/textarea"
import { InputGroupTextarea } from "@/components/ui/input-group"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { useFieldA11y } from "@/components/form/field-a11y"
import { cn } from "@/lib/utils"

type TextareaFieldProps = {
    label: string
    hideLabel?: boolean
    inInputGroup?: boolean
    description?: string
    placeholder?: string
    disabled?: boolean
    required?: boolean
    rows?: number
    maxLength?: number
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

    return (
        <Field data-invalid={isInvalid || undefined} className={cn(className)}>
            <FieldLabel
                htmlFor={resolvedId}
                className={hideLabel ? "sr-only" : undefined}
            >
                {label}
                {required ? <span className="text-destructive">*</span> : null}
            </FieldLabel>
            <Control
                id={resolvedId}
                name={field.name}
                value={field.state.value ?? ""}
                placeholder={placeholder}
                disabled={disabled}
                rows={rows}
                maxLength={maxLength}
                aria-invalid={isInvalid || undefined}
                aria-describedby={describedBy || undefined}
                className={cn(
                    inInputGroup && "min-h-control",
                    textareaClassName,
                )}
                onKeyDown={onKeyDown}
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
