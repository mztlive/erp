"use client"

import { MultiOptionCombobox } from "@/components/business"
import { useFieldContext } from "@/components/form/form-context"
import { toFieldErrors } from "@/components/form/utils"
import type { SelectFieldOption } from "@/components/form/select-field"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { cn } from "@/lib/utils"

type MultiSelectFieldProps = {
    label: string
    options: readonly SelectFieldOption[]
    placeholder?: string
    emptyLabel?: string
    description?: string
    disabled?: boolean
    required?: boolean
    className?: string
    id?: string
    /** 标签只显示短名称；选项列表仍保留完整描述。 */
    getChipLabel?: (option: SelectFieldOption) => string
}

/**
 * 绑定 TanStack Form field 的可搜索多选 Combobox。
 * 已选项以 chips 展示，值保持稳定顺序。
 */
export function MultiSelectField({
    label,
    options,
    placeholder,
    emptyLabel,
    description,
    disabled,
    required,
    className,
    id,
    getChipLabel,
}: MultiSelectFieldProps) {
    const field = useFieldContext<string[]>()
    const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid
    const errors = toFieldErrors(field.state.meta.errors)
    const resolvedId = id ?? field.name
    const descriptionId = `${resolvedId}-description`
    const errorId = `${resolvedId}-error`
    const describedBy = [
        description ? descriptionId : undefined,
        isInvalid ? errorId : undefined,
    ]
        .filter(Boolean)
        .join(" ")

    return (
        <Field data-invalid={isInvalid || undefined} className={cn(className)}>
            <FieldLabel
                htmlFor={resolvedId}
                className={disabled ? "opacity-60" : undefined}
            >
                {label}
                {required ? <span className="text-destructive">*</span> : null}
            </FieldLabel>
            <MultiOptionCombobox
                id={resolvedId}
                options={options}
                value={field.state.value ?? []}
                disabled={disabled}
                required={required}
                placeholder={placeholder ?? "请选择，可多选"}
                emptyLabel={emptyLabel}
                aria-label={label}
                aria-invalid={isInvalid || undefined}
                aria-describedby={describedBy || undefined}
                onBlur={field.handleBlur}
                getChipLabel={getChipLabel}
                onValueChange={(next) => field.handleChange(next)}
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
