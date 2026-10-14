"use client"

import type { ComponentProps, ReactNode } from "react"

import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { RegionCascadeSelect } from "@/features/regions"
import { cn } from "@/lib/utils"

/** 收货地址行的省、市、区县选择；必须选到最末级。 */
export function DeliveryRegionField({
    id,
    label = "收货地区",
    value,
    onValueChange,
    onBlur,
    errors,
    invalid,
    hint,
    className,
    labelClassName,
}: {
    id: string
    label?: string
    value: string
    onValueChange: (code: string) => void
    onBlur?: () => void
    errors: ComponentProps<typeof FieldError>["errors"]
    invalid: boolean
    hint?: ReactNode
    className?: string
    labelClassName?: string
}) {
    const errorId = `${id}-error`
    return (
        <Field
            data-invalid={invalid || undefined}
            className={cn("min-w-0 gap-2", className)}
        >
            <FieldLabel htmlFor={`${id}-province`} className={labelClassName}>
                {label}
            </FieldLabel>
            <RegionCascadeSelect
                idPrefix={id}
                leafOnly
                value={value}
                onValueChange={onValueChange}
                onBlur={onBlur}
                invalid={invalid}
                ariaLabel={label}
                ariaDescribedBy={invalid ? errorId : undefined}
            />
            {hint}
            {invalid ? <FieldError id={errorId} errors={errors} /> : null}
        </Field>
    )
}
