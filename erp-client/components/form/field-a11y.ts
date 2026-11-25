"use client"

import { useContext, useId, type ReactNode } from "react"
import type { AnyFieldApi } from "@tanstack/react-form"

import { fieldContext, useFieldContext } from "@/components/form/form-context"
import { toFieldErrors } from "@/components/form/utils"

export function hasFieldDescription(description?: ReactNode): boolean {
    return (
        description !== undefined &&
        description !== null &&
        description !== false &&
        description !== ""
    )
}

function buildFieldA11yIds(
    resolvedId: string,
    description: ReactNode | undefined,
    isInvalid: boolean,
) {
    const descriptionId = `${resolvedId}-description`
    const errorId = `${resolvedId}-error`
    const describedBy = [
        hasFieldDescription(description) ? descriptionId : undefined,
        isInvalid ? errorId : undefined,
    ]
        .filter(Boolean)
        .join(" ")

    return { descriptionId, errorId, describedBy }
}

/**
 * 绑定字段的稳定 id 与校验展示状态。
 *
 * 统一派生 label 的 `htmlFor`、说明与错误节点 id，以及 `aria-describedby`，
 * 避免各字段控件各自复制一套派生规则。
 * 显式传入 `id` 时原样保留以兼容自动化 ID 契约；未传入时结合 `useId()` 防止同页多表单撞 ID。
 */
export function useFieldA11y<T>(description?: ReactNode, id?: string) {
    const field = useFieldContext<T>()
    const generatedId = useId()
    const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid
    const errors = toFieldErrors(field.state.meta.errors)
    const resolvedId = id ?? `${field.name}-${generatedId}`
    const { descriptionId, errorId, describedBy } = buildFieldA11yIds(
        resolvedId,
        description,
        isInvalid,
    )

    return {
        field,
        isInvalid,
        errors,
        resolvedId,
        descriptionId,
        errorId,
        describedBy,
    }
}

/**
 * 可选绑定 TanStack Form 字段的辅助 Hook：
 * 在 `form.AppField` 内读取字段状态与校验错误，脱离表单上下文时退化为独立静态字段。
 */
export function useOptionalFieldA11y<T>(description?: ReactNode, id?: string) {
    const field = useContext(fieldContext) as
        | (AnyFieldApi & { state: { value: T } })
        | null
    const generatedId = useId()
    const isInvalid = Boolean(
        field && field.state.meta.isTouched && !field.state.meta.isValid,
    )
    const errors = field ? toFieldErrors(field.state.meta.errors) : []
    const resolvedId =
        id ?? (field ? `${field.name}-${generatedId}` : generatedId)
    const { descriptionId, errorId, describedBy } = buildFieldA11yIds(
        resolvedId,
        description,
        isInvalid,
    )

    return {
        field,
        isInvalid,
        errors,
        resolvedId,
        descriptionId,
        errorId,
        describedBy,
    }
}
