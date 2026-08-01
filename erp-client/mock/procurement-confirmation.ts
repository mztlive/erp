import type { StatusTone } from "@/components/ui/status-badge"

export type ProcurementConfirmationTask = Readonly<{
  id: string
  salesOrderNumber: string
  customerName: string
  supplierName: string
  productName: string
  quantity: string
  unit: string
  salesAmountGross: string
  purchaseAmountGross: string
  grossMarginRate: string
  fulfillmentDeadline: string
  ownerName: string
  submittedAt: string
  risk: { label: string; tone: StatusTone; description: string }
}>

/** 二次确认队列样板数据，只表达连续处理交互。 */
export const PROCUREMENT_CONFIRMATION_TASKS: readonly ProcurementConfirmationTask[] = [
  {
    id: "confirm_01",
    salesOrderNumber: "XS20260328001",
    customerName: "星河制造股份有限公司",
    supplierName: "华东优选供应链有限公司",
    productName: "员工关怀礼包 A",
    quantity: "300",
    unit: "套",
    salesAmountGross: "186000.00",
    purchaseAmountGross: "142500.00",
    grossMarginRate: "23.39%",
    fulfillmentDeadline: "2026-08-08",
    ownerName: "王敏",
    submittedAt: "2026-08-01 08:42",
    risk: {
      label: "交期需确认",
      tone: "warning",
      description: "供应商承诺 8 月 7 日到仓，距离客户最晚交付仅 1 天。",
    },
  },
  {
    id: "confirm_02",
    salesOrderNumber: "XS20260327012",
    customerName: "北辰能源集团",
    supplierName: "恒丰礼赠有限公司",
    productName: "户外保障套装",
    quantity: "160",
    unit: "套",
    salesAmountGross: "268800.00",
    purchaseAmountGross: "211200.00",
    grossMarginRate: "21.43%",
    fulfillmentDeadline: "2026-08-03",
    ownerName: "周航",
    submittedAt: "2026-07-31 16:18",
    risk: {
      label: "任务已超期",
      tone: "destructive",
      description: "确认截止时间已过，客户要求首批 8 月 3 日交付。",
    },
  },
  {
    id: "confirm_03",
    salesOrderNumber: "XS20260326009",
    customerName: "海纳教育科技有限公司",
    supplierName: "新程数字科技有限公司",
    productName: "健康服务兑换权益",
    quantity: "500",
    unit: "份",
    salesAmountGross: "325000.00",
    purchaseAmountGross: "247500.00",
    grossMarginRate: "23.85%",
    fulfillmentDeadline: "2026-08-15",
    ownerName: "王敏",
    submittedAt: "2026-08-01 09:12",
    risk: {
      label: "信息完整",
      tone: "success",
      description: "供应商、成本、交付方式和履约期限均已匹配。",
    },
  },
] as const
