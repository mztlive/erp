import type { AppFormWithBarApi } from "@/components/form/form-api"

import type { SalesChangeDraftValues } from "./sales-change-draft"

/** 子组件共享表单字段类型；校验泛型沿用项目表单约定。 */
export type SalesChangeDraftFormApi = AppFormWithBarApi<SalesChangeDraftValues>
