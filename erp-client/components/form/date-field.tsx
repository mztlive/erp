"use client"

import type { Matcher } from "react-day-picker"
import type { ComponentProps } from "react"

import { FieldFrame } from "@/components/form/field-frame"
import { useFieldA11y } from "@/components/form/field-a11y"
import { DatePicker, DateTimeLocalPicker } from "@/components/ui/date-picker"
import { cn } from "@/lib/utils"

type DateFieldProps = {
    label: string
    hideLabel?: boolean
    description?: string
    placeholder?: string
    disabled?: boolean
    required?: boolean
    clearable?: boolean
    allowLongTerm?: boolean
    disabledDates?: Matcher | Matcher[]
    className?: string
    inputClassName?: string
    size?: ComponentProps<typeof DatePicker>["size"]
    id?: string
}

/**
 * 绑定 TanStack Form field 的日期选择（`YYYY-MM-DD` 字符串，可选允许“长期”）。
 * 通过 `form.AppField` → `field.DateField` 使用。
 */
export function DateField({
    label,
    hideLabel = false,
    description,
    placeholder = "选择日期",
    disabled,
    required,
    clearable = true,
    allowLongTerm = false,
    disabledDates,
    className,
    inputClassName,
    size,
    id,
}: DateFieldProps) {
    const {
        field,
        isInvalid,
        errors,
        resolvedId,
        descriptionId,
        errorId,
        describedBy,
    } = useFieldA11y<string>(description, id)

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
            errorId={errorId}
            errors={errors}
        >
            <DatePicker
                id={resolvedId}
                size={size}
                value={field.state.value || undefined}
                onValueChange={(next) => {
                    field.handleChange(next ?? "")
                    field.handleBlur()
                }}
                placeholder={placeholder}
                disabled={disabled}
                clearable={clearable}
                allowLongTerm={allowLongTerm}
                disabledDates={disabledDates}
                className={inputClassName}
                aria-invalid={isInvalid || undefined}
                aria-describedby={describedBy || undefined}
            />
        </FieldFrame>
    )
}

type DateTimeFieldProps = {
    label: string
    hideLabel?: boolean
    description?: string
    placeholder?: string
    disabled?: boolean
    required?: boolean
    clearable?: boolean
    timeZone?: string
    showTimeZone?: boolean
    disabledDates?: Matcher | Matcher[]
    className?: string
    inputClassName?: string
    id?: string
}

/**
 * 绑定 TanStack Form field 的日期时间选择（`YYYY-MM-DDTHH:mm:ss` 本地字符串）。
 * 通过 `form.AppField` → `field.DateTimeField` 使用。
 */
export function DateTimeField({
    label,
    hideLabel = false,
    description,
    placeholder = "选择日期和时间",
    disabled,
    required,
    clearable = true,
    timeZone = "Asia/Shanghai",
    showTimeZone = true,
    disabledDates,
    className,
    inputClassName,
    id,
}: DateTimeFieldProps) {
    const {
        field,
        isInvalid,
        errors,
        resolvedId,
        descriptionId,
        errorId,
        describedBy,
    } = useFieldA11y<string>(description, id)

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
            errorId={errorId}
            errors={errors}
        >
            <DateTimeLocalPicker
                id={resolvedId}
                value={field.state.value || undefined}
                onValueChange={(next) => {
                    field.handleChange(next ?? "")
                    field.handleBlur()
                }}
                placeholder={placeholder}
                disabled={disabled}
                clearable={clearable}
                timeZone={timeZone}
                showTimeZone={showTimeZone}
                disabledDates={disabledDates}
                className={inputClassName}
                aria-invalid={isInvalid || undefined}
                aria-describedby={describedBy || undefined}
            />
        </FieldFrame>
    )
}
