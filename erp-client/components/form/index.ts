"use client";

import { createFormHook } from "@tanstack/react-form";
import {
  fieldContext,
  formContext,
  useFieldContext,
  useFormContext,
} from "@/components/form/form-context";
import { TextField } from "@/components/form/text-field";
import { TextareaField } from "@/components/form/textarea-field";
import { SubmitButton } from "@/components/form/submit-button";
import { SelectField } from "@/components/form/select-field";
import { PdfUploadField } from "@/components/form/pdf-upload-field";

/**
 * 应用统一表单入口：
 * - `useAppForm`：创建表单实例
 * - `withForm`：拆分子表单组件时复用类型
 * - `form.AppField` / `form.AppForm`：绑定下方 field/form 组件
 *
 * 所有业务表单必须通过本模块，禁止 react-hook-form / 非受控原生状态机。
 */
export const { useAppForm, withForm, withFieldGroup } = createFormHook({
  fieldContext,
  formContext,
  fieldComponents: {
    TextField,
    TextareaField,
    SelectField,
    PdfUploadField,
  },
  formComponents: {
    SubmitButton,
  },
});

export { useFieldContext, useFormContext };
export { TextField } from "@/components/form/text-field";
export { TextareaField } from "@/components/form/textarea-field";
export { SubmitButton } from "@/components/form/submit-button";
export {
  SelectField,
  ComboboxField,
  type SelectFieldOption,
  type ComboboxFieldOption,
} from "@/components/form/select-field";
export { PdfUploadField } from "@/components/form/pdf-upload-field";
export { toFieldErrors } from "@/components/form/utils";
