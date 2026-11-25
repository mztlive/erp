"use client"

import type { ComponentProps, ReactNode } from "react"
import type { Matcher } from "react-day-picker"

import { useFieldA11y } from "@/components/form/field-a11y"
import { FieldFrame } from "@/components/form/field-frame"
import {
    DateRangePicker,
    type DateRangeValue,
} from "@/components/ui/date-picker"
import { cn } from "@/lib/utils"

export type { DateRangeValue }

export type DateRangeFieldProps = {
    label: string
    hideLabel?: boolean
    description?: ReactNode
    placeholder?: string
    filterLabel?: string
    disabled?: boolean
    required?: boolean
    clearable?: boolean
    disabledDates?: Matcher | Matcher[]
    className?: string
    inputClassName?: string
    size?: ComponentProps<typeof DateRangePicker>["size"]
    id?: string
    onValueChange?: (value?: DateRangeValue) => void
}

/**
 * 绑定 TanStack Form field 的日期范围选择（默认 32px `h-control` 与 `.num` 等宽数字）。
 * 通过 `form.AppField` → `field.DateRangeField` 使用。
 */
export function DateRangeField({
    label,
    hideLabel = false,
    description,
    placeholder = "选择日期范围",
    filterLabel,
    disabled,
    required,
    clearable = true,
    disabledDates,
    className,
    inputClassName,
    size = "default",
    id,
    onValueChange,
}: DateRangeFieldProps) {
    const {
        field,
        isInvalid,
        errors,
        resolvedId,
        descriptionId,
        errorId,
        describedBy,
    } = useFieldA11y<DateRangeValue | undefined>(description, id)

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
            <DateRangePicker
                id={resolvedId}
                name={field.name}
                size={size}
                value={field.state.value}
                onValueChange={(next) => {
                    field.handleChange(next)
                    field.handleBlur()
                    onValueChange?.(next)
                }}
                placeholder={placeholder}
                filterLabel={filterLabel}
                disabled={disabled}
                clearable={clearable}
                disabledDates={disabledDates}
                className={inputClassName}
                aria-invalid={isInvalid || undefined}
                aria-describedby={describedBy || undefined}
            />
        </FieldFrame>
    )
}
