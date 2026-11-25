"use client"

import type { ReactNode } from "react"
import { LockIcon } from "lucide-react"

import { useOptionalFieldA11y } from "@/components/form/field-a11y"
import { FieldFrame } from "@/components/form/field-frame"
import { cn } from "@/lib/utils"

export type ReadOnlyFieldProps = {
    label: ReactNode
    hideLabel?: boolean
    /** 展示值；在 `form.AppField` 内省略时自动读取当前字段值。 */
    value?: ReactNode
    emptyText?: string
    description?: ReactNode
    /** 是否展示锁定图标与提示文案（详情页法则 5“带锁只读轻表面”）；默认 true。 */
    locked?: boolean
    /** 锁图标旁的短提示文案；传空字符串时仅展示锁图标。 */
    lockedLabel?: string
    /** 悬停说明，例如不可修改原因。 */
    title?: string
    /** 金额、编号或数量类值启用 `.num` 等宽数字。 */
    numeric?: boolean
    size?: "sm" | "default" | "lg"
    multiline?: boolean
    className?: string
    surfaceClassName?: string
    id?: string
}

function isEmptyReadOnlyValue(value: unknown): boolean {
    return (
        value === undefined ||
        value === null ||
        value === false ||
        (typeof value === "string" && value.trim().length === 0)
    )
}

/**
 * 详情页法则 5「带锁只读轻表面」标准字段：
 * 在编辑态替代沉重的禁用输入框/下拉框，支持在 `form.AppField` 内自动读值，也可独立使用。
 */
export function ReadOnlyField({
    label,
    hideLabel = false,
    value,
    emptyText = "—",
    description,
    locked = true,
    lockedLabel = "不可修改",
    title,
    numeric = false,
    size = "default",
    multiline = false,
    className,
    surfaceClassName,
    id,
}: ReadOnlyFieldProps) {
    const {
        field,
        isInvalid,
        errors,
        resolvedId,
        descriptionId,
        errorId,
        describedBy,
    } = useOptionalFieldA11y<unknown>(description, id)
    const labelId = `${resolvedId}-label`
    const resolvedRaw = value !== undefined ? value : field?.state.value
    const isEmpty = isEmptyReadOnlyValue(resolvedRaw)
    const displayValue: ReactNode = isEmpty
        ? emptyText
        : typeof resolvedRaw === "string" || typeof resolvedRaw === "number"
          ? String(resolvedRaw)
          : (resolvedRaw as ReactNode)

    return (
        <FieldFrame
            className={cn(className)}
            label={label}
            labelId={labelId}
            labelClassName={hideLabel ? "sr-only" : undefined}
            description={description}
            descriptionId={descriptionId}
            isInvalid={isInvalid}
            errorId={errorId}
            errors={errors}
        >
            <div
                id={resolvedId}
                data-slot="read-only-field"
                data-locked={locked ? "true" : undefined}
                title={title}
                aria-labelledby={labelId}
                aria-describedby={describedBy || undefined}
                className={cn(
                    "flex min-w-0 items-center justify-between gap-2 rounded-md bg-muted/45 px-2.5 text-body-compact text-foreground/85",
                    multiline
                        ? "min-h-control items-start py-1.5"
                        : size === "sm"
                          ? "h-control-sm"
                          : size === "lg"
                            ? "h-control-lg"
                            : "h-control",
                    surfaceClassName,
                )}
            >
                <span
                    className={cn(
                        "min-w-0 font-medium",
                        multiline
                            ? "whitespace-pre-wrap break-words"
                            : "truncate",
                        isEmpty && "font-normal text-muted-foreground",
                        numeric && "num",
                    )}
                >
                    {displayValue}
                </span>
                {locked ? (
                    <span className="inline-flex shrink-0 items-center gap-1 text-xs text-muted-foreground/80">
                        <LockIcon className="size-3" aria-hidden="true" />
                        {lockedLabel ? <span>{lockedLabel}</span> : null}
                    </span>
                ) : null}
            </div>
        </FieldFrame>
    )
}
