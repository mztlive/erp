import type { AppFormWithBarApi } from "@/components/form/form-api"

import type { CreateSalesOrderFormValues } from "@/features/sales-orders/lib/sales-order-create-model"

export type SalesOrderEditorPurpose = "create" | "draft"

/** 建单表单实例的共享类型；拆分组件只关心字段与值。 */
export type SalesOrderCreateFormApi =
    AppFormWithBarApi<CreateSalesOrderFormValues>
