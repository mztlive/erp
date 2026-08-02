/**
 * 跨工作面复用的码表与演示实体选项。
 * 业务页用 OptionCombobox / 实体 Combobox 消费；组件本身不取数。
 */

import type { ComboboxOption } from "@/components/business/option-combobox"
import type {
  OwnerComboboxItem,
  SettlementPartyComboboxItem,
  SupplierComboboxItem,
} from "@/components/business/entity-comboboxes"

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

/** 承运方（演示目录）。 */
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

/** 采购确认等场景可选供应商（与 mock seed 对齐）。 */
export const PROCUREMENT_SUPPLIER_OPTIONS: readonly SupplierComboboxItem[] = [
  {
    supplierId: "sup_hd",
    supplierName: "华东优选供应链有限公司",
    supplierCode: "SUP-HD",
    statusLabel: "有效",
    statusTone: "success",
    description: "礼包仓发 · 华东",
  },
  {
    supplierId: "sup_hf",
    supplierName: "恒丰礼赠有限公司",
    supplierCode: "SUP-HF",
    statusLabel: "有效",
    statusTone: "success",
    description: "礼包直发 · 京津冀",
  },
  {
    supplierId: "sup_xc",
    supplierName: "新程数字科技有限公司",
    supplierCode: "SUP-XC",
    statusLabel: "有效",
    statusTone: "success",
    description: "电子权益",
  },
  {
    supplierId: "sup_1",
    supplierName: "鲜果直供供应链有限公司",
    supplierCode: "SUP-XG",
    statusLabel: "有效",
    statusTone: "success",
  },
  {
    supplierId: "sup_2",
    supplierName: "礼遇包装工坊",
    supplierCode: "SUP-LY",
    statusLabel: "有效",
    statusTone: "success",
  },
] as const

/** 演示结算主体目录（合同上传 / 销售单上传路径）。 */
export const SETTLEMENT_PARTY_OPTIONS: readonly SettlementPartyComboboxItem[] =
  [
    {
      partyId: "sp_xinghe",
      displayName: "星河福利科技有限公司",
      partyCode: "SP-XH",
      statusLabel: "可用",
      statusTone: "success",
    },
    {
      partyId: "sp_qinghe",
      displayName: "清河企业管理咨询有限公司",
      partyCode: "SP-QH",
      statusLabel: "可用",
      statusTone: "success",
    },
    {
      partyId: "sp_dongfang",
      displayName: "东方联合实业集团",
      partyCode: "SP-DF",
      statusLabel: "可用",
      statusTone: "success",
    },
    {
      partyId: "sp_beichen",
      displayName: "北辰消费服务有限公司",
      partyCode: "SP-BC",
      statusLabel: "可用",
      statusTone: "success",
    },
    {
      partyId: "sp_huaxia",
      displayName: "华夏员工关怀中心",
      partyCode: "SP-HX",
      statusLabel: "可用",
      statusTone: "success",
    },
    {
      partyId: "sp_xinghe_sub",
      displayName: "星河福利（华南）分公司",
      partyCode: "SP-XH-S",
      statusLabel: "可用",
      statusTone: "success",
      description: "分公司结算",
    },
  ] as const

/** 演示销售负责人。 */
export const DEMO_OWNER_OPTIONS: readonly OwnerComboboxItem[] = [
  {
    userId: "user_zhao",
    displayName: "赵强",
    userCode: "U-ZQ",
    description: "华东销售",
  },
  {
    userId: "user_li",
    displayName: "李敏",
    userCode: "U-LM",
    description: "华北销售",
  },
  {
    userId: "user_wang",
    displayName: "王芳",
    userCode: "U-WF",
    description: "华南销售",
  },
  {
    userId: "user_chen",
    displayName: "陈磊",
    userCode: "U-CL",
    description: "大客户",
  },
] as const

export function paymentTermLabel(code: string): string {
  return (
    PAYMENT_TERM_OPTIONS.find((o) => o.value === code)?.label ?? code
  )
}
