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
// 结算主体 / Party
// ---------------------------------------------------------------------------

export type SettlementPartyComboboxItem = Readonly<{
  partyId: string
  displayName: string
  partyCode?: string
  statusLabel?: string
  statusTone?: StatusTone
  description?: string
}>

export type SettlementPartyComboboxProps = EntityComboboxBaseProps & {
  parties: readonly SettlementPartyComboboxItem[]
}

/** 结算主体选择：搜索名称与编号。 */
export function SettlementPartyCombobox({
  parties,
  placeholder = "搜索结算主体",
  emptyLabel = "没有符合条件的结算主体",
  ...props
}: SettlementPartyComboboxProps) {
  const items = React.useMemo(
    () =>
      mapToOptions(
        parties.map((p) => ({
          id: p.partyId,
          code: p.partyCode ?? p.partyId,
          label: p.displayName,
          status: {
            label: p.statusLabel ?? "可选",
            tone: p.statusTone ?? "neutral",
          },
          description: p.description,
        }))
      ),
    [parties]
  )

  return (
    <BusinessObjectCombobox
      {...props}
      items={items}
      label="结算主体"
      placeholder={placeholder}
      emptyLabel={emptyLabel}
    />
  )
}

// ---------------------------------------------------------------------------
// 仓库
// ---------------------------------------------------------------------------

export type WarehouseComboboxItem = Readonly<{
  warehouseId: string
  warehouseName: string
  warehouseCode?: string
  statusLabel?: string
  statusTone?: StatusTone
  description?: string
}>

export type WarehouseComboboxProps = EntityComboboxBaseProps & {
  warehouses: readonly WarehouseComboboxItem[]
}

/** 仓库选择：搜索名称与编码。 */
export function WarehouseCombobox({
  warehouses,
  placeholder = "搜索仓库名称或编码",
  emptyLabel = "没有符合条件的仓库",
  ...props
}: WarehouseComboboxProps) {
  const items = React.useMemo(
    () =>
      mapToOptions(
        warehouses.map((w) => ({
          id: w.warehouseId,
          code: w.warehouseCode ?? w.warehouseId,
          label: w.warehouseName,
          status: {
            label: w.statusLabel ?? "可选",
            tone: w.statusTone ?? "neutral",
          },
          description: w.description,
        }))
      ),
    [warehouses]
  )

  return (
    <BusinessObjectCombobox
      {...props}
      items={items}
      label="仓库"
      placeholder={placeholder}
      emptyLabel={emptyLabel}
    />
  )
}

// ---------------------------------------------------------------------------
// 负责人 / 用户
// ---------------------------------------------------------------------------

export type OwnerComboboxItem = Readonly<{
  userId: string
  displayName: string
  userCode?: string
  statusLabel?: string
  statusTone?: StatusTone
  description?: string
}>

export type OwnerComboboxProps = EntityComboboxBaseProps & {
  owners: readonly OwnerComboboxItem[]
}

/** 负责人选择：搜索姓名与工号。 */
export function OwnerCombobox({
  owners,
  placeholder = "搜索负责人或工号",
  emptyLabel = "没有符合条件的负责人",
  ...props
}: OwnerComboboxProps) {
  const items = React.useMemo(
    () =>
      mapToOptions(
        owners.map((o) => ({
          id: o.userId,
          code: o.userCode ?? o.userId,
          label: o.displayName,
          status: {
            label: o.statusLabel ?? "在职",
            tone: o.statusTone ?? "success",
          },
          description: o.description,
        }))
      ),
    [owners]
  )

  return (
    <BusinessObjectCombobox
      {...props}
      items={items}
      label="负责人"
      placeholder={placeholder}
      emptyLabel={emptyLabel}
    />
  )
}

// ---------------------------------------------------------------------------
// 品牌
// ---------------------------------------------------------------------------

export type BrandComboboxItem = Readonly<{
  brandId: string
  brandCode: string
  brandName: string
  statusLabel?: string
  statusTone?: StatusTone
  description?: string
}>

export type BrandComboboxProps = EntityComboboxBaseProps & {
  brands: readonly BrandComboboxItem[]
}

/** 品牌字典选择：搜索品牌代码与名称；仅应由 feature 注入当前可选项。 */
export function BrandCombobox({
  brands,
  placeholder = "搜索品牌代码或名称",
  emptyLabel = "没有符合条件的品牌",
  ...props
}: BrandComboboxProps) {
  const items = React.useMemo(
    () =>
      mapToOptions(
        brands.map((b) => ({
          id: b.brandId,
          code: b.brandCode,
          label: b.brandName,
          status: {
            label: b.statusLabel ?? "启用",
            tone: b.statusTone ?? "success",
          },
          description: b.description,
        }))
      ),
    [brands]
  )

  return (
    <BusinessObjectCombobox
      {...props}
      items={items}
      label="品牌"
      placeholder={placeholder}
      emptyLabel={emptyLabel}
    />
  )
}

// ---------------------------------------------------------------------------
// 商品分类（树形字典）
// ---------------------------------------------------------------------------

export type CategoryComboboxItem = Readonly<{
  categoryId: string
  categoryCode: string
  categoryName: string
  /** 根到当前节点的路径文案，如「礼品 / 礼盒」。 */
  pathLabel?: string
  /** 树深度，0 为根；用于缩进展示。 */
  depth?: number
  parentId?: string
  statusLabel?: string
  statusTone?: StatusTone
  description?: string
  /** 为 true 时不可选（例如选上级时排除自身与子树）。 */
  disabled?: boolean
}>

export type CategoryComboboxProps = EntityComboboxBaseProps & {
  categories: readonly CategoryComboboxItem[]
}

/** 商品分类选择：搜索代码、名称与路径；展示层级路径。 */
export function CategoryCombobox({
  categories,
  placeholder = "搜索分类代码、名称或路径",
  emptyLabel = "没有符合条件的分类",
  ...props
}: CategoryComboboxProps) {
  const items = React.useMemo(
    () =>
      mapToOptions(
        categories
          .filter((c) => !c.disabled)
          .map((c) => {
            const depth = c.depth ?? 0
            const indent = depth > 0 ? `${"— ".repeat(depth)}` : ""
            return {
              id: c.categoryId,
              code: c.categoryCode,
              label: `${indent}${c.categoryName}`,
              status: {
                label: c.statusLabel ?? "启用",
                tone: c.statusTone ?? "success",
              },
              description: [
                c.pathLabel && c.pathLabel !== c.categoryName
                  ? c.pathLabel
                  : null,
                c.description,
              ]
                .filter(Boolean)
                .join(" · "),
            }
          })
      ),
    [categories]
  )

  return (
    <BusinessObjectCombobox
      {...props}
      items={items}
      label="商品分类"
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
