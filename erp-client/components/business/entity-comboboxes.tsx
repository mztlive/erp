"use client"

import * as React from "react"

import {
  BusinessObjectCombobox,
  type BusinessObjectComboboxProps,
  type BusinessObjectOption,
} from "@/components/business/selectors"
import {
  OptionCombobox,
  type ComboboxOption,
  type OptionComboboxProps,
} from "@/components/business/option-combobox"
import type { StatusTone } from "@/components/ui/status-badge"

type EntityComboboxBaseProps = Omit<
  BusinessObjectComboboxProps,
  "items" | "label" | "placeholder" | "emptyLabel"
> & {
  placeholder?: string
  emptyLabel?: string
  className?: string
}

function mapToOptions(
  rows: readonly BusinessObjectOption[]
): BusinessObjectOption[] {
  return [...rows]
}

// ---------------------------------------------------------------------------
// 合同
// ---------------------------------------------------------------------------

export type ContractComboboxItem = Readonly<{
  contractId: string
  contractNo: string
  customerName: string
  statusLabel: string
  statusTone: StatusTone
  revisionNo?: number
  validTo?: string
  settlementPartyName?: string
}>

export type ContractComboboxProps = EntityComboboxBaseProps & {
  contracts: readonly ContractComboboxItem[]
}

/** 合同选择：搜索编号、客户名；展示状态与有效期。 */
export function ContractCombobox({
  contracts,
  placeholder = "搜索合同编号或客户",
  emptyLabel = "没有符合条件的合同",
  ...props
}: ContractComboboxProps) {
  const items = React.useMemo(
    () =>
      mapToOptions(
        contracts.map((c) => ({
          id: c.contractId,
          code: c.contractNo,
          label: c.customerName,
          status: { label: c.statusLabel, tone: c.statusTone },
          validUntil: c.validTo,
          description: [
            c.revisionNo != null ? `v${c.revisionNo}` : null,
            c.settlementPartyName,
          ]
            .filter(Boolean)
            .join(" · "),
        }))
      ),
    [contracts]
  )

  return (
    <BusinessObjectCombobox
      {...props}
      items={items}
      label="合同"
      placeholder={placeholder}
      emptyLabel={emptyLabel}
    />
  )
}

// ---------------------------------------------------------------------------
// 销售单
// ---------------------------------------------------------------------------

export type SalesOrderComboboxItem = Readonly<{
  id: string
  documentNumber: string
  customerName: string
  statusLabel: string
  statusTone: StatusTone
  amountGross?: string
  natureLabel?: string
}>

export type SalesOrderComboboxProps = EntityComboboxBaseProps & {
  orders: readonly SalesOrderComboboxItem[]
}

/** 销售单选择：搜索单号、客户名。 */
export function SalesOrderCombobox({
  orders,
  placeholder = "搜索销售单号或客户",
  emptyLabel = "没有符合条件的销售单",
  ...props
}: SalesOrderComboboxProps) {
  const items = React.useMemo(
    () =>
      mapToOptions(
        orders.map((o) => ({
          id: o.id,
          code: o.documentNumber,
          label: o.customerName,
          status: { label: o.statusLabel, tone: o.statusTone },
          description: [o.natureLabel, o.amountGross ? `¥${o.amountGross}` : null]
            .filter(Boolean)
            .join(" · "),
        }))
      ),
    [orders]
  )

  return (
    <BusinessObjectCombobox
      {...props}
      items={items}
      label="销售单"
      placeholder={placeholder}
      emptyLabel={emptyLabel}
    />
  )
}

// ---------------------------------------------------------------------------
// 客户
// ---------------------------------------------------------------------------

export type CustomerComboboxItem = Readonly<{
  id: string
  customerNo: string
  legalName: string
  shortName?: string
  statusLabel: string
  statusTone: StatusTone
  ownerName?: string
}>

export type CustomerComboboxProps = EntityComboboxBaseProps & {
  customers: readonly CustomerComboboxItem[]
}

/** 客户选择：搜索编号、全称、简称。 */
export function CustomerCombobox({
  customers,
  placeholder = "搜索客户编号或名称",
  emptyLabel = "没有符合条件的客户",
  ...props
}: CustomerComboboxProps) {
  const items = React.useMemo(
    () =>
      mapToOptions(
        customers.map((c) => ({
          id: c.id,
          code: c.customerNo,
          label: c.shortName ? `${c.legalName}（${c.shortName}）` : c.legalName,
          status: { label: c.statusLabel, tone: c.statusTone },
          description: c.ownerName ? `主责 ${c.ownerName}` : undefined,
        }))
      ),
    [customers]
  )

  return (
    <BusinessObjectCombobox
      {...props}
      items={items}
      label="客户"
      placeholder={placeholder}
      emptyLabel={emptyLabel}
    />
  )
}

// ---------------------------------------------------------------------------
// 采购单
// ---------------------------------------------------------------------------

export type PurchaseOrderComboboxItem = Readonly<{
  purchaseOrderId: string
  purchaseNo: string
  supplierName: string
  statusLabel: string
  statusTone: StatusTone
  salesOrderNo?: string
  grossAmount?: string
}>

export type PurchaseOrderComboboxProps = EntityComboboxBaseProps & {
  orders: readonly PurchaseOrderComboboxItem[]
}

/** 采购单选择：搜索采购单号、供应商。 */
export function PurchaseOrderCombobox({
  orders,
  placeholder = "搜索采购单号或供应商",
  emptyLabel = "没有符合条件的采购单",
  ...props
}: PurchaseOrderComboboxProps) {
  const items = React.useMemo(
    () =>
      mapToOptions(
        orders.map((o) => ({
          id: o.purchaseOrderId,
          code: o.purchaseNo,
          label: o.supplierName,
          status: { label: o.statusLabel, tone: o.statusTone },
          description: [
            o.salesOrderNo ? `销售 ${o.salesOrderNo}` : null,
            o.grossAmount ? `¥${o.grossAmount}` : null,
          ]
            .filter(Boolean)
            .join(" · "),
        }))
      ),
    [orders]
  )

  return (
    <BusinessObjectCombobox
      {...props}
      items={items}
      label="采购单"
      placeholder={placeholder}
      emptyLabel={emptyLabel}
    />
  )
}

// ---------------------------------------------------------------------------
// 供应商
// ---------------------------------------------------------------------------

export type SupplierComboboxItem = Readonly<{
  supplierId: string
  supplierName: string
  supplierCode?: string
  statusLabel?: string
  statusTone?: StatusTone
  description?: string
}>

export type SupplierComboboxProps = EntityComboboxBaseProps & {
  suppliers: readonly SupplierComboboxItem[]
}

/** 供应商选择：搜索名称与编码。 */
export function SupplierCombobox({
  suppliers,
  placeholder = "搜索供应商名称或编码",
  emptyLabel = "没有符合条件的供应商",
  ...props
}: SupplierComboboxProps) {
  const items = React.useMemo(
    () =>
      mapToOptions(
        suppliers.map((s) => ({
          id: s.supplierId,
          code: s.supplierCode ?? s.supplierId,
          label: s.supplierName,
          status: {
            label: s.statusLabel ?? "可选",
            tone: s.statusTone ?? "neutral",
          },
          description: s.description,
        }))
      ),
    [suppliers]
  )

  return (
    <BusinessObjectCombobox
      {...props}
      items={items}
      label="供应商"
      placeholder={placeholder}
      emptyLabel={emptyLabel}
    />
  )
}

// ---------------------------------------------------------------------------
// 商品 / SKU（轻量：无状态徽章时用 OptionCombobox）
// ---------------------------------------------------------------------------

export type ProductComboboxItem = Readonly<{
  productId: string
  sku: string
  name: string
  statusLabel?: string
  statusTone?: StatusTone
  description?: string
}>

export type ProductComboboxProps = EntityComboboxBaseProps & {
  products: readonly ProductComboboxItem[]
}

/** ERP 商品/SKU 选择。 */
export function ProductCombobox({
  products,
  placeholder = "搜索 SKU 或商品名称",
  emptyLabel = "没有符合条件的商品",
  ...props
}: ProductComboboxProps) {
  const items = React.useMemo(
    () =>
      mapToOptions(
        products.map((p) => ({
          id: p.productId,
          code: p.sku,
          label: p.name,
          status: {
            label: p.statusLabel ?? "可选",
            tone: p.statusTone ?? "neutral",
          },
          description: p.description,
        }))
      ),
    [products]
  )

  return (
    <BusinessObjectCombobox
      {...props}
      items={items}
      label="商品"
      placeholder={placeholder}
      emptyLabel={emptyLabel}
    />
  )
}

// ---------------------------------------------------------------------------
// 枚举 / 筛选便捷封装（固定 options 的命名别名场景）
// ---------------------------------------------------------------------------

export type EnumComboboxProps = Omit<OptionComboboxProps, "options"> & {
  options: readonly ComboboxOption[]
}

/** 状态、环境、角色等枚举筛选的可搜索 Combobox。 */
export function EnumCombobox(props: EnumComboboxProps) {
  return <OptionCombobox {...props} />
}
