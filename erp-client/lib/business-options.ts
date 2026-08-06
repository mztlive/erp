/**
 * 跨工作面复用的码表选项。
 * 业务页用 OptionCombobox 消费；组件本身不取数。
 */

import type { ComboboxOption } from "@/components/business/option-combobox"

/** 付款条件（与采购单草稿等对齐）。 */
export const PAYMENT_TERM_OPTIONS: readonly ComboboxOption[] = [
  { value: "PREPAY_100", label: "先款 100%" },
  { value: "PREPAY_50", label: "先款 50%" },
  { value: "PREPAY_30", label: "先款 30%" },
  { value: "POSTPAY_NET15", label: "货到 15 天" },
  { value: "POSTPAY_NET30", label: "货到 30 天" },
  { value: "CONTRACT", label: "按合同约定" },
] as const

/** 销售/库存常用单位。 */
export const UNIT_OPTIONS: readonly ComboboxOption[] = [
  { value: "件", label: "件" },
  { value: "箱", label: "箱" },
  { value: "套", label: "套" },
  { value: "盒", label: "盒" },
  { value: "篮", label: "篮" },
  { value: "张", label: "张" },
  { value: "份", label: "份" },
  { value: "kg", label: "kg" },
  { value: "次", label: "次" },
] as const

/** 入库质量结果。 */
export const QUALITY_RESULT_OPTIONS: readonly ComboboxOption[] = [
  { value: "合格", label: "合格" },
  { value: "部分合格", label: "部分合格" },
  { value: "不合格", label: "不合格" },
  { value: "待检", label: "待检" },
] as const

/** 承运方。 */
export const CARRIER_OPTIONS: readonly ComboboxOption[] = [
  { value: "顺丰速运", label: "顺丰速运" },
  { value: "中通快递", label: "中通快递" },
  { value: "圆通速递", label: "圆通速递" },
  { value: "京东物流", label: "京东物流" },
  { value: "德邦物流", label: "德邦物流" },
  { value: "供应商自送", label: "供应商自送" },
] as const

/** 接口错误转交角色（值为展示名，与任务转交 API 一致）。 */
export const TRANSFER_ROLE_OPTIONS: readonly ComboboxOption[] = [
  { value: "采购", label: "采购" },
  { value: "财务", label: "财务" },
  { value: "运营", label: "运营" },
  { value: "对接", label: "对接" },
  { value: "研发运维", label: "研发运维" },
  { value: "主管", label: "主管" },
] as const

export function paymentTermLabel(code: string): string {
  return (
    PAYMENT_TERM_OPTIONS.find((o) => o.value === code)?.label ?? code
  )
}
