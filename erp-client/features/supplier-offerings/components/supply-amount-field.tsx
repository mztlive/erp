"use client"

import { useFieldContext } from "@/components/form/form-context"
import { toFieldErrors } from "@/components/form/utils"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"

export function SupplyAmountField({
    id,
    label,
    unit,
    required = false,
}: {
    id: string
    label: string
    unit?: string
    required?: boolean
}) {
    const field = useFieldContext<string>()
    const invalid = field.state.meta.isTouched && !field.state.meta.isValid
    return (
        <Field className="gap-2 min-w-0" data-invalid={invalid || undefined}>
            <FieldLabel htmlFor={id}>
                {label}
                {required && <span className="text-destructive">*</span>}
            </FieldLabel>
            <InputGroup className="h-9">
                <InputGroupAddon>¥</InputGroupAddon>
                <InputGroupInput
                    id={id}
                    name={field.name}
                    inputMode="decimal"
                    value={field.state.value}
                    aria-required={required || undefined}
                    aria-invalid={invalid || undefined}
                    aria-describedby={invalid ? `${id}-error` : undefined}
                    onChange={(event) => field.handleChange(event.target.value)}
                    onBlur={field.handleBlur}
                />
                {unit && (
                    <InputGroupAddon align="inline-end">
                        / {unit}
                    </InputGroupAddon>
                )}
            </InputGroup>
            {invalid && (
                <FieldError
                    id={`${id}-error`}
                    errors={toFieldErrors(field.state.meta.errors)}
                />
            )}
        </Field>
    )
}
