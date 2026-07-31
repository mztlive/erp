"use client";

import { Input } from "@/components/ui/input";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@/components/ui/field";
import { useFieldContext } from "@/components/form/form-context";
import { toFieldErrors } from "@/components/form/utils";
import { cn } from "@/lib/utils";

type TextFieldProps = {
  label: string;
  description?: string;
  placeholder?: string;
  type?: React.ComponentProps<"input">["type"];
  disabled?: boolean;
  className?: string;
  inputClassName?: string;
  autoComplete?: string;
};

/**
 * 绑定 TanStack Form field 的文本输入（shadcn Field + Input）。
 * 通过 `form.AppField` → `field.TextField` 使用。
 */
export function TextField({
  label,
  description,
  placeholder,
  type = "text",
  disabled,
  className,
  inputClassName,
  autoComplete,
}: TextFieldProps) {
  const field = useFieldContext<string>();
  const isInvalid =
    field.state.meta.isTouched && !field.state.meta.isValid;
  const errors = toFieldErrors(field.state.meta.errors);

  return (
    <Field
      data-invalid={isInvalid || undefined}
      className={cn(className)}
    >
      <FieldLabel htmlFor={field.name}>{label}</FieldLabel>
      <Input
        id={field.name}
        name={field.name}
        type={type}
        value={field.state.value ?? ""}
        placeholder={placeholder}
        disabled={disabled}
        autoComplete={autoComplete}
        aria-invalid={isInvalid || undefined}
        className={inputClassName}
        onBlur={field.handleBlur}
        onChange={(e) => field.handleChange(e.target.value)}
      />
      {description ? <FieldDescription>{description}</FieldDescription> : null}
      {isInvalid ? <FieldError errors={errors} /> : null}
    </Field>
  );
}
