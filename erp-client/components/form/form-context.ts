"use client";

import { createFormHookContexts } from "@tanstack/react-form";

/**
 * 应用级 Form / Field Context。
 * 业务表单请通过 `useAppForm` + `form.AppField` 使用，不要绕开此层。
 */
export const { fieldContext, formContext, useFieldContext, useFormContext } =
  createFormHookContexts();
