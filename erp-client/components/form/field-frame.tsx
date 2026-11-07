"use client"

import type { ReactNode } from "react"

import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"

/**
 * 表单字段的标签、说明和错误外壳。
 * 控件本身仍由各字段组件渲染，节点顺序与原来一致。
 */
export function FieldFrame({
    className,
    label,
    labelClassName,
    htmlFor,
    required,
    description,
    descriptionId,
    isInvalid,
    errorId,
    errors,
    afterDescription,
    children,
}: {
    className?: string
    label: ReactNode
    labelClassName?: string
    htmlFor: string
    required?: boolean
    description?: ReactNode
    descriptionId: string
    isInvalid: boolean
    errorId: string
    errors: Array<{ message?: string } | undefined>
    afterDescription?: ReactNode
    children: ReactNode
}) {
    return (
        <Field data-invalid={isInvalid || undefined} className={className}>
            <FieldLabel htmlFor={htmlFor} className={labelClassName}>
                {label}
                {required ? <span className="text-destructive">*</span> : null}
            </FieldLabel>
            {children}
            {description ? (
                <FieldDescription id={descriptionId}>
                    {description}
                </FieldDescription>
            ) : null}
            {afterDescription}
            {isInvalid ? <FieldError id={errorId} errors={errors} /> : null}
        </Field>
    )
}
