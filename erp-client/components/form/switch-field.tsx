"use client"

import type { ComponentProps, ReactNode } from "react"

import { useFieldA11y } from "@/components/form/field-a11y"
import { FieldFrame } from "@/components/form/field-frame"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"

export type SwitchFieldProps = {
    label: ReactNode
    hideLabel?: boolean
    description?: ReactNode
    size?: ComponentProps<typeof Switch>["size"]
    disabled?: boolean
    required?: boolean
    className?: string
    switchClassName?: string
    id?: string
    onCheckedChange?: (checked: boolean) => void
}

/**
 * 绑定 TanStack Form field 的开关控件（FieldFrame + Switch）。
 * 通过 `form.AppField` → `field.SwitchField` 使用。
 */
export function SwitchField({
    label,
    hideLabel = false,
    description,
    size = "default",
    disabled,
    required,
    className,
    switchClassName,
    id,
    onCheckedChange,
}: SwitchFieldProps) {
    const {
        field,
        isInvalid,
        errors,
        resolvedId,
        descriptionId,
        errorId,
        describedBy,
    } = useFieldA11y<boolean>(description, id)

    return (
        <FieldFrame
            layout={hideLabel ? "vertical" : "inline-end"}
            className={cn(className)}
            label={label}
            labelClassName={
                hideLabel
                    ? "sr-only"
                    : "cursor-pointer text-body-compact font-medium text-foreground"
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
            <Switch
                id={resolvedId}
                name={field.name}
                size={size}
                checked={Boolean(field.state.value)}
                disabled={disabled}
                required={required}
                aria-invalid={isInvalid || undefined}
                aria-describedby={describedBy || undefined}
                className={switchClassName}
                onBlur={field.handleBlur}
                onCheckedChange={(checked) => {
                    field.handleChange(checked)
                    onCheckedChange?.(checked)
                }}
            />
        </FieldFrame>
    )
}
