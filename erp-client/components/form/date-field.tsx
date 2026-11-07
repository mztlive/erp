"use client"

import type { Matcher } from "react-day-picker"
import type { ComponentProps } from "react"

import { useFieldA11y } from "@/components/form/field-a11y"
import { DatePicker, DateTimeLocalPicker } from "@/components/ui/date-picker"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
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
        <Field data-invalid={isInvalid || undefined} className={cn(className)}>
            <FieldLabel
                htmlFor={resolvedId}
                className={hideLabel ? "sr-only" : undefined}
            >
                {label}
                {required ? <span className="text-destructive">*</span> : null}
            </FieldLabel>
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
            {description ? (
                <FieldDescription id={descriptionId}>
                    {description}
                </FieldDescription>
            ) : null}
            {isInvalid ? <FieldError id={errorId} errors={errors} /> : null}
        </Field>
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
        <Field data-invalid={isInvalid || undefined} className={cn(className)}>
            <FieldLabel
                htmlFor={resolvedId}
                className={hideLabel ? "sr-only" : undefined}
            >
                {label}
                {required ? <span className="text-destructive">*</span> : null}
            </FieldLabel>
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
            {description ? (
                <FieldDescription id={descriptionId}>
                    {description}
                </FieldDescription>
            ) : null}
            {isInvalid ? <FieldError id={errorId} errors={errors} /> : null}
        </Field>
    )
}
