"use client"

import { createFormHook } from "@tanstack/react-form"

import {
    applyApiFormError,
    clearApiFieldError,
    clearApiFormError,
    getApiFormError,
    preserveApiFieldErrors,
    subscribeApiFormError,
} from "./api-errors"
import { CheckboxField } from "@/components/form/checkbox-field"
import { DateField, DateTimeField } from "@/components/form/date-field"
import { DateRangeField } from "@/components/form/date-range-field"
import { EvidenceUploadField } from "@/components/form/evidence-upload-field"
import {
    fieldContext,
    formContext,
    useFieldContext,
    useFormContext,
} from "@/components/form/form-context"
import { FormError } from "@/components/form/form-error"
import { FormValidationSummary } from "@/components/form/form-validation-summary"
import { MoneyField } from "@/components/form/money-field"
import { MultiFileField } from "@/components/form/multi-file-field"
import { MultiSelectField } from "@/components/form/multi-select-field"
import { PdfUploadField } from "@/components/form/pdf-upload-field"
import { RadioGroupField } from "@/components/form/radio-group-field"
import { ReadOnlyField } from "@/components/form/read-only-field"
import { SelectField } from "@/components/form/select-field"
import { SingleFileField } from "@/components/form/single-file-field"
import { SubmitButton } from "@/components/form/submit-button"
import { SwitchField } from "@/components/form/switch-field"
import { TextField } from "@/components/form/text-field"
import { TextareaField } from "@/components/form/textarea-field"
import { useWorkspaceFormGuard } from "@/lib/workspace-form-guard"

/**
 * 应用统一表单入口：
 * - `useAppForm`：创建表单实例
 * - `withForm`：拆分子表单组件时复用类型
 * - `form.AppField` / `form.AppForm`：绑定下方 field/form 组件
 *
 * 所有业务表单必须通过本模块，禁止 react-hook-form / 非受控原生状态机。
 */
const fieldComponents = {
    TextField,
    TextareaField,
    SelectField,
    MultiSelectField,
    SingleFileField,
    MultiFileField,
    PdfUploadField,
    EvidenceUploadField,
    DateField,
    DateTimeField,
    DateRangeField,
    CheckboxField,
    SwitchField,
    RadioGroupField,
    MoneyField,
    ReadOnlyField,
} as const

const formComponents = {
    SubmitButton,
    FormError,
    FormValidationSummary,
} as const

export type AppFieldComponents = typeof fieldComponents
export type AppFormComponents = typeof formComponents

const {
    useAppForm: useBaseAppForm,
    withForm,
    withFieldGroup,
} = createFormHook({
    fieldContext,
    formContext,
    fieldComponents,
    formComponents,
})

export { fieldContext, formContext, useFieldContext, useFormContext }
export type { AppFormApi, AppFormWithBarApi } from "@/components/form/form-api"
export {
    useFieldA11y,
    useOptionalFieldA11y,
} from "@/components/form/field-a11y"
export {
    FieldFrame,
    type FieldFrameLayout,
    type FieldFrameProps,
} from "@/components/form/field-frame"
export { TextField, type TextFieldProps } from "@/components/form/text-field"
export {
    TextareaField,
    type TextareaCounterMode,
    type TextareaFieldProps,
} from "@/components/form/textarea-field"
export {
    SubmitButton,
    type SubmitButtonErrorPlacement,
    type SubmitButtonProps,
} from "@/components/form/submit-button"
export { FormError } from "@/components/form/form-error"
export {
    FormValidationSummary,
    type FormValidationSummaryProps,
} from "@/components/form/form-validation-summary"
export {
    SelectField,
    type SelectFieldOption,
    type SelectFieldProps,
} from "@/components/form/select-field"
export {
    MultiSelectField,
    type MultiSelectFieldProps,
} from "@/components/form/multi-select-field"
export {
    SingleFileField,
    type SingleFileFieldProps,
} from "@/components/form/single-file-field"
export {
    MultiFileField,
    type MultiFileFieldProps,
} from "@/components/form/multi-file-field"
export {
    PdfUploadField,
    type PdfUploadFieldProps,
} from "@/components/form/pdf-upload-field"
export {
    EvidenceUploadField,
    EVIDENCE_FILE_MAX_BYTES,
    EVIDENCE_FILE_TYPES,
    type EvidenceUploadFieldProps,
} from "@/components/form/evidence-upload-field"
export {
    DateField,
    DateTimeField,
    type DateFieldProps,
    type DateTimeFieldProps,
} from "@/components/form/date-field"
export {
    DateRangeField,
    type DateRangeFieldProps,
    type DateRangeValue,
} from "@/components/form/date-range-field"
export {
    CheckboxField,
    type CheckboxFieldProps,
} from "@/components/form/checkbox-field"
export {
    SwitchField,
    type SwitchFieldProps,
} from "@/components/form/switch-field"
export {
    RadioGroupField,
    type RadioGroupFieldOption,
    type RadioGroupFieldProps,
} from "@/components/form/radio-group-field"
export { MoneyInput, type MoneyInputProps } from "@/components/form/money-input"
export { MoneyField, type MoneyFieldProps } from "@/components/form/money-field"
export {
    ReadOnlyField,
    type ReadOnlyFieldProps,
} from "@/components/form/read-only-field"
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
/** 即时生效的查询条件不属于待保存业务资料，不注册工作页离开保护。 */
export const useAppFilterForm: typeof useBaseAppForm = useBaseAppForm

export {
    withForm,
    withFieldGroup,
    applyApiFormError,
    clearApiFieldError,
    clearApiFormError,
    getApiFormError,
    preserveApiFieldErrors,
    subscribeApiFormError,
}
