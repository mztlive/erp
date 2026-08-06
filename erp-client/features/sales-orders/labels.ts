import type {
  CardSalesApproval,
  SalesOrderListItem,
} from "@/features/sales-orders/types"

/** 业务性质展示文案（W05 列表/详情/纸质预览共用）。 */
export const NATURE_LABEL: Record<SalesOrderListItem["nature"], string> = {
  physical_service: "实物与服务",
  card_voucher: "卡券",
}

/** 创建来源文案（MALL = 商城入口；ERP = 本系统入口）。 */
export const ORIGIN_LABEL: Record<SalesOrderListItem["originSystem"], string> = {
  erp: "创建于 ERP",
  mall: "创建于商城",
}

/** 卡券审批任务类型文案。 */
export const CARD_APPROVAL_TYPE_LABEL: Record<
  CardSalesApproval["workItemType"],
  string
> = {
  CARD_SALES_MANAGER_APPROVAL: "卡券销售领导审批",
  CARD_SALES_OPERATION_APPROVAL: "卡券销售运营审批",
}
