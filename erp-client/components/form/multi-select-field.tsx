"use client"

import type { ComponentProps, ReactNode } from "react"

import { useFieldA11y } from "@/components/form/field-a11y"
import { FieldFrame } from "@/components/form/field-frame"
import type { SelectFieldOption } from "@/components/form/select-field"
import type { ComboboxRefreshProps } from "@/components/ui/combobox-refresh"
import { MultiOptionCombobox } from "@/components/ui/multi-option-combobox"
import { cn } from "@/lib/utils"

export type MultiSelectFieldProps = ComboboxRefreshProps & {
    label: string
    hideLabel?: boolean
    options: readonly SelectFieldOption[]
    placeholder?: string
    emptyLabel?: string
    description?: ReactNode
    loading?: boolean
    disabled?: boolean
    required?: boolean
    size?: ComponentProps<typeof MultiOptionCombobox>["size"]
    className?: string
    inputClassName?: string
    id?: string
    /** 标签只显示短名称；选项列表仍保留完整描述。 */
    getChipLabel?: (option: SelectFieldOption) => string
    onValueChange?: (value: string[]) => void
}

/**
 * 绑定 TanStack Form field 的可搜索多选 Combobox。
 * 已选项以 chips 展示，值保持稳定顺序。
 */
export function MultiSelectField({
    label,
    hideLabel = false,
    options,
    placeholder,
    emptyLabel,
    description,
    loading,
    disabled,
    onRefresh,
    required,
    size,
    className,
    inputClassName,
    id,
    getChipLabel,
    onValueChange,
}: MultiSelectFieldProps) {
    const {
        field,
        isInvalid,
        errors,
        resolvedId,
        descriptionId,
        errorId,
        describedBy,
    } = useFieldA11y<string[]>(description, id)

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
            <MultiOptionCombobox
                id={resolvedId}
                name={field.name}
                size={size}
                inputClassName={cn("w-full", inputClassName)}
                options={options}
                value={field.state.value ?? []}
                loading={loading}
                disabled={disabled}
                onRefresh={onRefresh}
                required={required}
                placeholder={placeholder ?? "请选择，可多选"}
                emptyLabel={emptyLabel}
                aria-label={label}
                aria-invalid={isInvalid || undefined}
                aria-describedby={describedBy || undefined}
                onBlur={field.handleBlur}
                getChipLabel={getChipLabel}
                onValueChange={(next) => {
                    field.handleChange(next)
                    onValueChange?.(next)
                }}
            />
        </FieldFrame>
    )
}
