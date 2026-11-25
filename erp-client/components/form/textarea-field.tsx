"use client"

import type { KeyboardEventHandler } from "react"
import { FieldFrame } from "@/components/form/field-frame"
import { useFieldA11y } from "@/components/form/field-a11y"
import { InputGroupTextarea } from "@/components/ui/input-group"
import { Textarea } from "@/components/ui/textarea"
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
        >
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
        </FieldFrame>
    )
}
