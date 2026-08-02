"use client"

import { OptionCombobox } from "@/components/business/option-combobox"
import { useFieldContext } from "@/components/form/form-context"
import { toFieldErrors } from "@/components/form/utils"
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@/components/ui/field"
import { cn } from "@/lib/utils"

export type SelectFieldOption = Readonly<{
  value: string
  label: string
  disabled?: boolean
  keywords?: string
}>

/** @deprecated 使用 SelectFieldOption；保留别名兼容既有 import。 */
export type ComboboxFieldOption = SelectFieldOption

type SelectFieldProps = {
  label: string
  options: readonly SelectFieldOption[]
  placeholder?: string
  description?: string
  disabled?: boolean
  hideLabel?: boolean
  className?: string
  /** 输入框宽度等；历史 prop 名 selectClassName 仍可用 */
  selectClassName?: string
  inputClassName?: string
  allowClear?: boolean
  onValueChange?: (value: string) => void
}

/**
 * 绑定 TanStack Form field 的可搜索 Combobox。
 * 动态业务选项由 Query 结果注入；业务实体优先用 ContractCombobox 等专用组件。
 */
export function SelectField({
  label,
  options,
  placeholder,
  description,
  disabled,
  hideLabel = false,
  className,
  selectClassName,
  inputClassName,
  allowClear = true,
  onValueChange,
}: SelectFieldProps) {
  const field = useFieldContext<string>()
  const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid
  const errors = toFieldErrors(field.state.meta.errors)

  return (
    <Field data-invalid={isInvalid || undefined} className={cn(className)}>
      <FieldLabel htmlFor={field.name} className={hideLabel ? "sr-only" : undefined}>
        {label}
      </FieldLabel>
      <OptionCombobox
        id={field.name}
        options={options}
        value={field.state.value || null}
        disabled={disabled}
        placeholder={placeholder ?? "请选择"}
        allowClear={allowClear}
        aria-label={label}
        aria-invalid={isInvalid || undefined}
        inputClassName={cn("w-full", selectClassName, inputClassName)}
        onBlur={field.handleBlur}
        onValueChange={(next) => {
          const value = next ?? ""
          field.handleChange(value)
          onValueChange?.(value)
        }}
      />
      {description ? <FieldDescription>{description}</FieldDescription> : null}
      {isInvalid ? <FieldError errors={errors} /> : null}
    </Field>
  )
}

/** 与 SelectField 相同；推荐新代码使用此命名。 */
export const ComboboxField = SelectField
