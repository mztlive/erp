import type { StatusTone } from "@/components/ui/status-badge"

export type SalesOrderNature = "physical_service" | "card_voucher"
export type SalesOrderOwner = "erp" | "mall"

export type ProgressTrack = {
  label: string
  tone: StatusTone
}

export type SalesOrderLineItem = {
  id: string
  name: string
  sku?: string
  /** 数量，十进制字符串 */
  quantity: string
  unit: string
  /** 含税单价 */
  unitPriceGross: string
  /** 含税小计 */
  amountGross: string
  /** 卡券：面额 */
  faceValue?: string
  /** 卡券：配赠率展示，如 5.00 */
  giftRate?: string
  /** 卡券：电子卡 / 实体卡 */
  cardForm?: string
  /** 实物服务：履约方式 */
  fulfillmentMode?: string
  /** 明细履约期限（实物） */
  dueDate?: string
}

export type SalesOrderRelatedSummary = {
  purchaseOrders: number
  fulfillments: number
  receipts: number
  invoices: number
}

export type SalesOrderListItem = {
  id: string
  documentNumber: string
  customerName: string
  contractNumber: string
  nature: SalesOrderNature
  ownerSystem: SalesOrderOwner
  primaryStatus: { label: string; tone: StatusTone }
  fulfillment: ProgressTrack
  collection: ProgressTrack
  invoicing: ProgressTrack
  /** 含税成交金额 */
  amountGross: string
  /** 不含税金额 */
  amountNet: string
  /** 税额 */
  taxAmount: string
  /** 已回款（含税口径展示） */
  receivedAmount: string
  /** 已开票 */
  invoicedAmount: string
  ownerName: string
  submittedAt: string
  welfareScene: string
  remark?: string
  version: number
  settlementEntity: string
  sellerEntity: string
  paymentTerms: string
  /** 表头履约期限（卡券全单；实物为摘要文案） */
  fulfillmentDeadline: string
  customerContact?: string
  lineItems: readonly SalesOrderLineItem[]
  related: SalesOrderRelatedSummary
}
