import type { AppFormWithBarApi } from "@/components/form/form-api"

import type { PurchaseChangeDraftValues } from "./purchase-change-draft"

/** 子组件共享表单字段类型；校验泛型沿用项目表单约定。 */
export type PurchaseChangeDraftFormApi =
    AppFormWithBarApi<PurchaseChangeDraftValues>
