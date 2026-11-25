"use client"

import type { ReactNode } from "react"

import { hasFieldDescription } from "@/components/form/field-a11y"
import {
    Field,
    FieldContent,
    FieldDescription,
    FieldError,
    FieldLabel,
    FieldTitle,
} from "@/components/ui/field"
import { RequiredMark } from "@/components/ui/required-mark"
import { cn } from "@/lib/utils"

export type FieldFrameLayout = "vertical" | "inline-start" | "inline-end"

export type FieldFrameProps = {
    className?: string
    label: ReactNode
    labelId?: string
    labelClassName?: string
    htmlFor?: string
    required?: boolean
    description?: ReactNode
    descriptionId: string
    isInvalid: boolean
    disabled?: boolean
    errorId: string
    errors: Array<{ message?: string } | undefined>
    afterDescription?: ReactNode
    layout?: FieldFrameLayout
    children: ReactNode
}

/**
 * 表单字段的标签、说明和错误外壳。
 * 控件本身仍由各字段组件渲染，垂直模式节点顺序与原来一致；
 * `inline-start` / `inline-end` 供 CheckboxField / SwitchField 复用同一标签、说明与错误契约。
 */
export function FieldFrame({
    className,
    label,
    labelId,
    labelClassName,
    htmlFor,
    required,
    description,
    descriptionId,
    isInvalid,
    disabled,
    errorId,
    errors,
    afterDescription,
    layout = "vertical",
    children,
}: FieldFrameProps) {
    const showDescription = hasFieldDescription(description)
    const labelNode = htmlFor ? (
        <FieldLabel id={labelId} htmlFor={htmlFor} className={labelClassName}>
            {label}
            {required ? <RequiredMark /> : null}
        </FieldLabel>
    ) : (
        <FieldTitle
            id={labelId}
            className={cn(
                "gap-1.5 [&>[data-slot=required-mark]]:-ml-1",
                labelClassName,
            )}
        >
            {label}
            {required ? <RequiredMark /> : null}
        </FieldTitle>
    )

    if (layout === "inline-start" || layout === "inline-end") {
        const textBlock = (
            <FieldContent>
                {labelNode}
                {showDescription ? (
                    <FieldDescription id={descriptionId}>
                        {description}
                    </FieldDescription>
                ) : null}
            </FieldContent>
        )

        return (
            <Field
                data-invalid={isInvalid || undefined}
                data-disabled={disabled ? "true" : undefined}
                className={className}
            >
                <div
                    className={cn(
                        "flex gap-2.5",
                        layout === "inline-end"
                            ? "items-start justify-between gap-3"
                            : "items-start",
                    )}
                >
                    {layout === "inline-start" ? (
                        <>
                            {children}
                            {textBlock}
                        </>
                    ) : (
                        <>
                            {textBlock}
                            {children}
                        </>
                    )}
                </div>
                {afterDescription}
                {isInvalid ? <FieldError id={errorId} errors={errors} /> : null}
            </Field>
        )
    }

    return (
        <Field
            data-invalid={isInvalid || undefined}
            data-disabled={disabled ? "true" : undefined}
            className={className}
        >
            {labelNode}
            {children}
            {showDescription ? (
                <FieldDescription id={descriptionId}>
                    {description}
                </FieldDescription>
            ) : null}
            {afterDescription}
            {isInvalid ? <FieldError id={errorId} errors={errors} /> : null}
        </Field>
    )
}
