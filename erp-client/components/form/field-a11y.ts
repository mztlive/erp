"use client"

import { useFieldContext } from "@/components/form/form-context"
import { toFieldErrors } from "@/components/form/utils"

/**
 * 绑定字段的稳定 id 与校验展示状态。
 *
 * 统一派生 label 的 `htmlFor`、说明与错误节点 id，以及 `aria-describedby`，
 * 避免各字段控件各自复制一套派生规则。
 */
export function useFieldA11y<T>(description?: string, id?: string) {
    const field = useFieldContext<T>()
    const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid
    const errors = toFieldErrors(field.state.meta.errors)
    const resolvedId = id ?? field.name
    const descriptionId = `${resolvedId}-description`
    const errorId = `${resolvedId}-error`
    const describedBy = [
        description ? descriptionId : undefined,
        isInvalid ? errorId : undefined,
    ]
        .filter(Boolean)
        .join(" ")

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
