"use client"

import type { ReactNode } from "react"

import { useFieldA11y } from "@/components/form/field-a11y"
import { FieldFrame } from "@/components/form/field-frame"
import { Checkbox } from "@/components/ui/checkbox"
import { cn } from "@/lib/utils"

export type CheckboxFieldProps = {
    label: ReactNode
    hideLabel?: boolean
    description?: ReactNode
    disabled?: boolean
    required?: boolean
    indeterminate?: boolean
    className?: string
    checkboxClassName?: string
    id?: string
    onCheckedChange?: (checked: boolean) => void
}

/**
 * 绑定 TanStack Form field 的复选框（FieldFrame + Checkbox）。
 * 通过 `form.AppField` → `field.CheckboxField` 使用。
 */
export function CheckboxField({
    label,
    hideLabel = false,
    description,
    disabled,
    required,
    indeterminate = false,
    className,
    checkboxClassName,
    id,
    onCheckedChange,
}: CheckboxFieldProps) {
    const {
        field,
        isInvalid,
        errors,
        resolvedId,
        descriptionId,
        errorId,
        describedBy,
    } = useFieldA11y<boolean>(description, id)
    const labelId = `${resolvedId}-label`

    return (
        <FieldFrame
            layout={hideLabel ? "vertical" : "inline-start"}
            className={cn(className)}
            label={label}
            labelId={labelId}
            labelClassName={
                hideLabel
                    ? "sr-only"
                    : "cursor-pointer text-body-compact font-normal text-foreground"
            }
            htmlFor={resolvedId}
            required={required}
            description={description}
            descriptionId={descriptionId}
            isInvalid={isInvalid}
            disabled={disabled}
            errorId={errorId}
            errors={errors}
        >
            <Checkbox
                id={resolvedId}
                name={field.name}
                nativeButton
                render={<button type="button" aria-labelledby={labelId} />}
                aria-labelledby={labelId}
                checked={Boolean(field.state.value)}
                indeterminate={indeterminate}
                disabled={disabled}
                required={required}
                aria-invalid={isInvalid || undefined}
                aria-describedby={describedBy || undefined}
                className={cn(!hideLabel && "mt-0.5", checkboxClassName)}
                onBlur={field.handleBlur}
                onCheckedChange={(checked) => {
                    const next = Boolean(checked)
                    field.handleChange(next)
                    onCheckedChange?.(next)
                }}
            />
        </FieldFrame>
    )
}
