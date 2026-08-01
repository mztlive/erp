"use client"

import { useFieldContext } from "@/components/form/form-context"
import { toFieldErrors } from "@/components/form/utils"
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@/components/ui/field"
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select"
import { cn } from "@/lib/utils"

export type SelectFieldOption = Readonly<{
  value: string
  label: string
  disabled?: boolean
}>

type SelectFieldProps = {
  label: string
  options: readonly SelectFieldOption[]
  placeholder?: string
  description?: string
  disabled?: boolean
  hideLabel?: boolean
  className?: string
  selectClassName?: string
  onValueChange?: (value: string) => void
}

/** 绑定 TanStack Form field 的原生选择器；动态业务选项由 Query 结果注入。 */
export function SelectField({
  label,
  options,
  placeholder,
  description,
  disabled,
  hideLabel = false,
  className,
  selectClassName,
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
      <NativeSelect
        id={field.name}
        name={field.name}
        value={field.state.value ?? ""}
        disabled={disabled}
        aria-invalid={isInvalid || undefined}
        className={cn("w-full", selectClassName)}
        onBlur={field.handleBlur}
        onChange={(event) => {
          const value = event.target.value
          field.handleChange(value)
          onValueChange?.(value)
        }}
      >
        {placeholder ? (
          <NativeSelectOption value="">{placeholder}</NativeSelectOption>
        ) : null}
        {options.map((option) => (
          <NativeSelectOption
            key={option.value}
            value={option.value}
            disabled={option.disabled}
          >
            {option.label}
          </NativeSelectOption>
        ))}
      </NativeSelect>
      {description ? <FieldDescription>{description}</FieldDescription> : null}
      {isInvalid ? <FieldError errors={errors} /> : null}
    </Field>
  )
}
