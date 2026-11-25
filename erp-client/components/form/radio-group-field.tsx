"use client"

import type { ReactNode } from "react"

import { useFieldA11y } from "@/components/form/field-a11y"
import { FieldFrame } from "@/components/form/field-frame"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export type RadioGroupFieldOption = Readonly<{
    value: string
    label: ReactNode
    description?: ReactNode
    disabled?: boolean
}>

export type RadioGroupFieldProps = {
    label: ReactNode
    hideLabel?: boolean
    description?: ReactNode
    options: readonly RadioGroupFieldOption[]
    variant?: "default" | "segment"
    orientation?: "vertical" | "horizontal"
    disabled?: boolean
    required?: boolean
    className?: string
    groupClassName?: string
    id?: string
    /** 保留既有自动化选项 ID；未传入时由字段 ID 派生。 */
    getOptionId?: (value: string) => string
    onValueChange?: (value: string) => void
}

/**
 * 绑定 TanStack Form field 的单选组 / 分段单选（FieldFrame + RadioGroup）。
 * 通过 `form.AppField` → `field.RadioGroupField` 使用。
 */
export function RadioGroupField({
    label,
    hideLabel = false,
    description,
    options,
    variant = "default",
    orientation,
    disabled,
    required,
    className,
    groupClassName,
    id,
    getOptionId,
    onValueChange,
}: RadioGroupFieldProps) {
    const {
        field,
        isInvalid,
        errors,
        resolvedId,
        descriptionId,
        errorId,
        describedBy,
    } = useFieldA11y<string>(description, id)
    const labelId = `${resolvedId}-label`
    const resolvedOrientation =
        orientation ?? (variant === "segment" ? "horizontal" : "vertical")

    return (
        <FieldFrame
            className={cn(className)}
            label={label}
            labelId={labelId}
            labelClassName={hideLabel ? "sr-only" : undefined}
            required={required}
            description={description}
            descriptionId={descriptionId}
            isInvalid={isInvalid}
            disabled={disabled}
            errorId={errorId}
            errors={errors}
        >
            <RadioGroup
                id={resolvedId}
                name={field.name}
                value={field.state.value ?? ""}
                disabled={disabled}
                required={required}
                aria-labelledby={labelId}
                aria-invalid={isInvalid || undefined}
                aria-describedby={describedBy || undefined}
                onBlur={field.handleBlur}
                onValueChange={(next) => {
                    const value = String(next ?? "")
                    field.handleChange(value)
                    onValueChange?.(value)
                }}
                className={cn(
                    variant === "segment"
                        ? "inline-flex w-fit flex-wrap gap-0"
                        : resolvedOrientation === "horizontal"
                          ? "flex flex-wrap items-start gap-x-5 gap-y-2"
                          : "grid gap-2.5",
                    groupClassName,
                )}
            >
                {options.map((option) => {
                    const optionId =
                        getOptionId?.(option.value) ??
                        `${resolvedId}-option-${toAutomationIdSegment(option.value)}`
                    const optionLabelId = `${optionId}-label`
                    const optionDisabled = disabled || option.disabled

                    if (variant === "segment") {
                        return (
                            <RadioGroupItem
                                key={option.value}
                                id={optionId}
                                nativeButton
                                // oxlint-disable-next-line jsx-a11y/control-has-associated-label -- Base UI renders the option label inside this segment button.
                                render={<button type="button" />}
                                value={option.value}
                                variant="segment"
                                disabled={optionDisabled}
                                aria-invalid={isInvalid || undefined}
                            >
                                {option.label}
                            </RadioGroupItem>
                        )
                    }

                    return (
                        <div
                            key={option.value}
                            className="flex items-start gap-2"
                        >
                            <RadioGroupItem
                                id={optionId}
                                nativeButton
                                render={
                                    <button
                                        type="button"
                                        aria-labelledby={optionLabelId}
                                    />
                                }
                                aria-labelledby={optionLabelId}
                                value={option.value}
                                disabled={optionDisabled}
                                aria-invalid={isInvalid || undefined}
                                className="mt-0.5"
                            />
                            <div className="flex min-w-0 flex-col gap-0.5">
                                <Label
                                    id={optionLabelId}
                                    htmlFor={optionId}
                                    className="cursor-pointer text-body-compact font-normal text-foreground"
                                >
                                    {option.label}
                                </Label>
                                {option.description ? (
                                    <p className="text-xs text-muted-foreground">
                                        {option.description}
                                    </p>
                                ) : null}
                            </div>
                        </div>
                    )
                })}
            </RadioGroup>
        </FieldFrame>
    )
}
