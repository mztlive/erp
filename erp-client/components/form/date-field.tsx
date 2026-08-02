"use client";

import type { Matcher } from "react-day-picker";

import { useFieldContext } from "@/components/form/form-context";
import { toFieldErrors } from "@/components/form/utils";
import {
  DatePicker,
  DateTimeLocalPicker,
} from "@/components/ui/date-picker";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@/components/ui/field";
import { cn } from "@/lib/utils";

type DateFieldProps = {
  label: string;
  hideLabel?: boolean;
  description?: string;
  placeholder?: string;
  disabled?: boolean;
  clearable?: boolean;
  disabledDates?: Matcher | Matcher[];
  className?: string;
  inputClassName?: string;
};

/**
 * 绑定 TanStack Form field 的日期选择（`YYYY-MM-DD` 字符串）。
 * 通过 `form.AppField` → `field.DateField` 使用。
 */
export function DateField({
  label,
  hideLabel = false,
  description,
  placeholder = "选择日期",
  disabled,
  clearable = true,
  disabledDates,
  className,
  inputClassName,
}: DateFieldProps) {
  const field = useFieldContext<string>();
  const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid;
  const errors = toFieldErrors(field.state.meta.errors);

  return (
    <Field data-invalid={isInvalid || undefined} className={cn(className)}>
      <FieldLabel
        htmlFor={field.name}
        className={hideLabel ? "sr-only" : undefined}
      >
        {label}
      </FieldLabel>
      <DatePicker
        value={field.state.value || undefined}
        onValueChange={(next) => {
          field.handleChange(next ?? "");
          field.handleBlur();
        }}
        placeholder={placeholder}
        disabled={disabled}
        clearable={clearable}
        disabledDates={disabledDates}
        className={inputClassName}
        aria-invalid={isInvalid || undefined}
      />
      {description ? <FieldDescription>{description}</FieldDescription> : null}
      {isInvalid ? <FieldError errors={errors} /> : null}
    </Field>
  );
}

type DateTimeFieldProps = {
  label: string;
  hideLabel?: boolean;
  description?: string;
  placeholder?: string;
  disabled?: boolean;
  clearable?: boolean;
  timeZone?: string;
  disabledDates?: Matcher | Matcher[];
  className?: string;
  inputClassName?: string;
};

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
  clearable = true,
  timeZone = "Asia/Shanghai",
  disabledDates,
  className,
  inputClassName,
}: DateTimeFieldProps) {
  const field = useFieldContext<string>();
  const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid;
  const errors = toFieldErrors(field.state.meta.errors);

  return (
    <Field data-invalid={isInvalid || undefined} className={cn(className)}>
      <FieldLabel
        htmlFor={field.name}
        className={hideLabel ? "sr-only" : undefined}
      >
        {label}
      </FieldLabel>
      <DateTimeLocalPicker
        value={field.state.value || undefined}
        onValueChange={(next) => {
          field.handleChange(next ?? "");
          field.handleBlur();
        }}
        placeholder={placeholder}
        disabled={disabled}
        clearable={clearable}
        timeZone={timeZone}
        disabledDates={disabledDates}
        className={inputClassName}
        aria-invalid={isInvalid || undefined}
      />
      {description ? <FieldDescription>{description}</FieldDescription> : null}
      {isInvalid ? <FieldError errors={errors} /> : null}
    </Field>
  );
}
