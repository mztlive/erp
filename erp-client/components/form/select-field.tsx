"use client"

import type { ComponentProps, ReactNode } from "react"

import { useFieldA11y } from "@/components/form/field-a11y"
import { FieldFrame } from "@/components/form/field-frame"
import type { ComboboxRefreshProps } from "@/components/ui/combobox-refresh"
import { OptionCombobox } from "@/components/ui/option-combobox"
import { cn } from "@/lib/utils"

export type SelectFieldOption = Readonly<{
    value: string
    label: string
    disabled?: boolean
    keywords?: string
}>

export type SelectFieldProps = ComboboxRefreshProps & {
    label: string
    options: readonly SelectFieldOption[]
    placeholder?: string
    emptyLabel?: string
    description?: ReactNode
    disabled?: boolean
    loading?: boolean
    required?: boolean
    hideLabel?: boolean
    inInputGroup?: boolean
    className?: string
    /** 输入框宽度等；历史 prop 名 selectClassName 仍可用 */
    selectClassName?: string
    inputClassName?: string
    size?: ComponentProps<typeof OptionCombobox>["size"]
    allowClear?: boolean
    onValueChange?: (value: string) => void
    id?: string
}

/**
 * 绑定 TanStack Form field 的可搜索 Combobox。
 * 动态业务选项由 Query 结果注入；业务实体优先用 ContractCombobox 等专用组件。
 */
export function SelectField({
    label,
    options,
    placeholder,
    emptyLabel,
    description,
    disabled,
    onRefresh,
    loading,
    required,
    hideLabel = false,
    inInputGroup = false,
    className,
    selectClassName,
    inputClassName,
    size,
    allowClear = true,
    onValueChange,
    id,
}: SelectFieldProps) {
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
            disabled={disabled}
            errorId={errorId}
            errors={errors}
        >
            <OptionCombobox
                inInputGroup={inInputGroup}
                size={size}
                id={resolvedId}
                name={field.name}
                options={options}
                value={field.state.value || null}
                disabled={disabled}
                onRefresh={onRefresh}
                loading={loading}
                placeholder={placeholder ?? "请选择"}
                emptyLabel={emptyLabel}
                allowClear={allowClear}
                required={required}
                aria-label={label}
                aria-invalid={isInvalid || undefined}
                aria-describedby={describedBy || undefined}
                inputClassName={cn("w-full", selectClassName, inputClassName)}
                onBlur={field.handleBlur}
                onValueChange={(next) => {
                    const value = next ?? ""
                    field.handleChange(value)
                    onValueChange?.(value)
                }}
            />
        </FieldFrame>
    )
}
