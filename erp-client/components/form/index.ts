"use client"

import {
    applyApiFormError,
    clearApiFieldError,
    clearApiFormError,
    preserveApiFieldErrors,
} from "./api-errors"
import { createFormHook } from "@tanstack/react-form"
import {
    fieldContext,
    formContext,
    useFieldContext,
    useFormContext,
} from "@/components/form/form-context"
import { TextField } from "@/components/form/text-field"
import { TextareaField } from "@/components/form/textarea-field"
import { SubmitButton } from "@/components/form/submit-button"
import { SelectField } from "@/components/form/select-field"
import { MultiSelectField } from "@/components/form/multi-select-field"
import { PdfUploadField } from "@/components/form/pdf-upload-field"
import { EvidenceUploadField } from "@/components/form/evidence-upload-field"
import { DateField, DateTimeField } from "@/components/form/date-field"
import { useWorkspaceFormGuard } from "@/lib/workspace-form-guard"

/**
 * 应用统一表单入口：
 * - `useAppForm`：创建表单实例
 * - `withForm`：拆分子表单组件时复用类型
 * - `form.AppField` / `form.AppForm`：绑定下方 field/form 组件
 *
 * 所有业务表单必须通过本模块，禁止 react-hook-form / 非受控原生状态机。
 */
const {
    useAppForm: useBaseAppForm,
    withForm,
    withFieldGroup,
} = createFormHook({
    fieldContext,
    formContext,
    fieldComponents: {
        TextField,
        TextareaField,
        SelectField,
        MultiSelectField,
        PdfUploadField,
        EvidenceUploadField,
        DateField,
        DateTimeField,
    },
    formComponents: {
        SubmitButton,
    },
})

export { useFieldContext, useFormContext }
export { TextField } from "@/components/form/text-field"
export { TextareaField } from "@/components/form/textarea-field"
export { SubmitButton } from "@/components/form/submit-button"
export {
    SelectField,
    type SelectFieldOption,
} from "@/components/form/select-field"
export { MultiSelectField } from "@/components/form/multi-select-field"
export { PdfUploadField } from "@/components/form/pdf-upload-field"
export {
    EvidenceUploadField,
    EVIDENCE_FILE_MAX_BYTES,
    EVIDENCE_FILE_TYPES,
} from "@/components/form/evidence-upload-field"
export { DateField, DateTimeField } from "@/components/form/date-field"
export { toFieldErrors } from "@/components/form/utils"

/** 保留表单泛型推导，统一承接未被业务流程处理的提交错误。 */
export const useAppForm: typeof useBaseAppForm = (options) => {
    const form = useBaseAppForm({
        ...options,
        validationLogic: preserveApiFieldErrors(options.validationLogic),
        listeners: {
            ...options.listeners,
            onChange: (context) => {
                clearApiFieldError(context.formApi, context.fieldApi.name)
                options.listeners?.onChange?.(context)
            },
        },
        onSubmit: async (context) => {
            clearApiFormError(context.formApi)
            try {
                await options.onSubmit?.(context)
            } catch (error) {
                applyApiFormError(context.formApi, error)
                throw error
            }
        },
    })
    useWorkspaceFormGuard(form)
    return form
}
export { withForm, withFieldGroup, applyApiFormError }
