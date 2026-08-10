"use client"

import * as React from "react"

import {
  ContractCombobox,
  CustomerCombobox,
  ProductCombobox,
  SalesOrderCombobox,
  SettlementPartyCombobox,
  SupplierCombobox,
  WarehouseCombobox,
  type ContractComboboxItem,
  type ContractComboboxProps,
  type CustomerComboboxItem,
  type CustomerComboboxProps,
  type ProductComboboxItem,
  type ProductComboboxProps,
  type SalesOrderComboboxItem,
  type SalesOrderComboboxProps,
  type SettlementPartyComboboxItem,
  type SettlementPartyComboboxProps,
  type SupplierComboboxItem,
  type SupplierComboboxProps,
  type WarehouseComboboxItem,
  type WarehouseComboboxProps,
} from "@/components/business/entity-comboboxes"
import {
  OptionCombobox,
  type OptionComboboxProps,
} from "@/components/business/option-combobox"
import type {
  EntitySelectorPurpose,
  SellableSkuComboboxItem,
} from "@/features/entity-selectors/api"
import {
  useCompanySkuSelectorQuery,
  useContractSelectorQuery,
  useCustomerSelectorQuery,
  useDebouncedSearch,
  usePartySelectorQuery,
  useSalesOrderSelectorQuery,
  useSellableSkuSelectorQuery,
  useSupplierSelectorQuery,
  useWarehouseSelectorQuery,
  useMallSelectorQuery,
  useVoucherCategorySelectorQuery,
} from "@/features/entity-selectors/queries"
import { getErrorMessage } from "@/lib/api/errors"

type SmartProps<TProps, TItem> = Omit<
  TProps,
  | "loading"
  | "filterMode"
  | "onSearchChange"
  | "contracts"
  | "customers"
  | "orders"
  | "products"
  | "parties"
  | "suppliers"
  | "warehouses"
> & {
  purpose?: EntitySelectorPurpose
  selectedItem?: TItem
  onItemChange?: (item?: TItem) => void
}

function mergeSelected<T>(
  rows: readonly T[] | undefined,
  selected: T | null | undefined,
  idOf: (item: T) => string,
): readonly T[] {
  if (!selected) return rows ?? []
  return (rows ?? []).some((item) => idOf(item) === idOf(selected))
    ? rows ?? []
    : [selected, ...(rows ?? [])]
}

function useSearchInput() {
  const [input, setInput] = React.useState("")
  return { input: useDebouncedSearch(input), onSearchChange: setInput }
}

export type SupplierSearchComboboxProps = SmartProps<
  SupplierComboboxProps,
  SupplierComboboxItem
>

export function SupplierSearchCombobox({
  purpose = "form",
  selectedItem,
  onItemChange,
  emptyLabel,
  value,
  onValueChange,
  ...props
}: SupplierSearchComboboxProps) {
  const search = useSearchInput()
  const query = useSupplierSelectorQuery(
    { query: search.input, purpose },
    value,
  )
  const rows = mergeSelected(
    query.list.data,
    selectedItem ?? query.selected.data,
    (item) => item.supplierId,
  )
  return (
    <SupplierCombobox
      {...props}
      value={value}
      suppliers={rows}
      onValueChange={(id) => {
        onValueChange(id)
        onItemChange?.(rows.find((item) => item.supplierId === id))
      }}
      onSearchChange={search.onSearchChange}
      filterMode="remote"
      loading={query.list.isFetching || query.selected.isFetching}
      emptyLabel={
        query.list.isError
          ? getErrorMessage(query.list.error, "供应商加载失败，请重试")
          : emptyLabel
      }
    />
  )
}

export type CustomerSearchComboboxProps = SmartProps<
  CustomerComboboxProps,
  CustomerComboboxItem
> & {
  scope?: "mine" | "collaborating" | "assigned" | "all_authorized"
}

export function CustomerSearchCombobox({
  purpose = "form",
  scope = "assigned",
  selectedItem,
  onItemChange,
  emptyLabel,
  value,
  onValueChange,
  ...props
}: CustomerSearchComboboxProps) {
  const search = useSearchInput()
  const query = useCustomerSelectorQuery(
    { query: search.input, purpose, scope },
    value,
  )
  const rows = mergeSelected(
    query.list.data,
    selectedItem ?? query.selected.data,
    (item) => item.id,
  )
  return (
    <CustomerCombobox
      {...props}
      value={value}
      customers={rows}
      onValueChange={(id) => {
        onValueChange(id)
        onItemChange?.(rows.find((item) => item.id === id))
      }}
      onSearchChange={search.onSearchChange}
      filterMode="remote"
      loading={query.list.isFetching || query.selected.isFetching}
      emptyLabel={
        query.list.isError
          ? getErrorMessage(query.list.error, "客户加载失败，请重试")
          : emptyLabel
      }
    />
  )
}

export type SettlementPartySearchComboboxProps = SmartProps<
  SettlementPartyComboboxProps,
  SettlementPartyComboboxItem
>

export function SettlementPartySearchCombobox({
  purpose = "form",
  selectedItem,
  onItemChange,
  emptyLabel,
  value,
  onValueChange,
  ...props
}: SettlementPartySearchComboboxProps) {
  const search = useSearchInput()
  const query = usePartySelectorQuery({ query: search.input, purpose }, value)
  const rows = mergeSelected(
    query.list.data,
    selectedItem ?? query.selected.data,
    (item) => item.partyId,
  )
  return (
    <SettlementPartyCombobox
      {...props}
      value={value}
      parties={rows}
      onValueChange={(id) => {
        onValueChange(id)
        onItemChange?.(rows.find((item) => item.partyId === id))
      }}
      onSearchChange={search.onSearchChange}
      filterMode="remote"
      loading={query.list.isFetching || query.selected.isFetching}
      emptyLabel={
        query.list.isError
          ? getErrorMessage(query.list.error, "结算主体加载失败，请重试")
          : emptyLabel
      }
    />
  )
}

export type WarehouseSearchComboboxProps = SmartProps<
  WarehouseComboboxProps,
  WarehouseComboboxItem
>

export function WarehouseSearchCombobox({
  purpose = "filter",
  selectedItem,
  onItemChange,
  emptyLabel,
  value,
  onValueChange,
  ...props
}: WarehouseSearchComboboxProps) {
  const search = useSearchInput()
  const query = useWarehouseSelectorQuery(
    { query: search.input, purpose },
    value,
  )
  const rows = mergeSelected(
    query.list.data,
    selectedItem ?? query.selected.data,
    (item) => item.warehouseId,
  )
  return (
    <WarehouseCombobox
      {...props}
      value={value}
      warehouses={rows}
      onValueChange={(id) => {
        onValueChange(id)
        onItemChange?.(rows.find((item) => item.warehouseId === id))
      }}
      onSearchChange={search.onSearchChange}
      filterMode="remote"
      loading={query.list.isFetching || query.selected.isFetching}
      emptyLabel={
        query.list.isError
          ? getErrorMessage(query.list.error, "仓库加载失败，请重试")
          : emptyLabel
      }
    />
  )
}

export type ContractSearchComboboxProps = SmartProps<
  ContractComboboxProps,
  ContractComboboxItem
> & {
  customerId?: string
  selectableOnly?: boolean
}

export function ContractSearchCombobox({
  purpose = "sales-order",
  customerId,
  selectableOnly = false,
  selectedItem,
  onItemChange,
  emptyLabel,
  value,
  onValueChange,
  ...props
}: ContractSearchComboboxProps) {
  const search = useSearchInput()
  const query = useContractSelectorQuery(
    {
      query: search.input,
      purpose,
      customerId: customerId || undefined,
      selectableOnly,
    },
    value,
  )
  const rows = mergeSelected(
    query.list.data,
    selectedItem ?? query.selected.data,
    (item) => item.contractId,
  )
  return (
    <ContractCombobox
      {...props}
      value={value}
      contracts={rows}
      onValueChange={(id) => {
        onValueChange(id)
        onItemChange?.(rows.find((item) => item.contractId === id))
      }}
      onSearchChange={search.onSearchChange}
      filterMode="remote"
      loading={query.list.isFetching || query.selected.isFetching}
      emptyLabel={
        query.list.isError
          ? getErrorMessage(query.list.error, "合同加载失败，请重试")
          : emptyLabel
      }
    />
  )
}

export type SalesOrderSearchComboboxProps = SmartProps<
  SalesOrderComboboxProps,
  SalesOrderComboboxItem
>

export function SalesOrderSearchCombobox({
  purpose = "filter",
  selectedItem,
  onItemChange,
  emptyLabel,
  value,
  onValueChange,
  ...props
}: SalesOrderSearchComboboxProps) {
  const search = useSearchInput()
  const query = useSalesOrderSelectorQuery(
    { query: search.input, purpose },
    value,
  )
  const rows = mergeSelected(
    query.list.data,
    selectedItem ?? query.selected.data,
    (item) => item.id,
  )
  return (
    <SalesOrderCombobox
      {...props}
      value={value}
      orders={rows}
      onValueChange={(id) => {
        onValueChange(id)
        onItemChange?.(rows.find((item) => item.id === id))
      }}
      onSearchChange={search.onSearchChange}
      filterMode="remote"
      loading={query.list.isFetching || query.selected.isFetching}
      emptyLabel={
        query.list.isError
          ? getErrorMessage(query.list.error, "销售单加载失败，请重试")
          : emptyLabel
      }
    />
  )
}

export type SellableSkuSearchComboboxProps = SmartProps<
  ProductComboboxProps,
  SellableSkuComboboxItem
> & { productKind?: string; excludeProductKind?: string }

export function SellableSkuSearchCombobox({
  purpose = "sales-order",
  productKind,
  excludeProductKind,
  selectedItem,
  onItemChange,
  emptyLabel,
  value,
  onValueChange,
  ...props
}: SellableSkuSearchComboboxProps) {
  const search = useSearchInput()
  const query = useSellableSkuSelectorQuery({
    query: search.input,
    purpose,
    productKind,
    excludeProductKind,
  })
  const rows = mergeSelected(
    query.data,
    selectedItem,
    (item) => item.productId,
  )
  return (
    <ProductCombobox
      {...props}
      value={value}
      products={rows}
      onValueChange={(id) => {
        onValueChange(id)
        onItemChange?.(rows.find((item) => item.productId === id))
      }}
      onSearchChange={search.onSearchChange}
      filterMode="remote"
      loading={query.isFetching}
      emptyLabel={
        query.isError
          ? getErrorMessage(query.error, "商品加载失败，请重试")
          : emptyLabel
      }
    />
  )
}

export type CompanySkuSearchComboboxProps = SmartProps<
  ProductComboboxProps,
  ProductComboboxItem
>

export function CompanySkuSearchCombobox({
  purpose = "supplier-offering",
  selectedItem,
  onItemChange,
  emptyLabel,
  value,
  onValueChange,
  ...props
}: CompanySkuSearchComboboxProps) {
  const search = useSearchInput()
  const query = useCompanySkuSelectorQuery({ query: search.input, purpose })
  const rows = mergeSelected(
    query.data,
    selectedItem,
    (item) => item.productId,
  )
  return (
    <ProductCombobox
      {...props}
      value={value}
      products={rows}
      onValueChange={(id) => {
        onValueChange(id)
        onItemChange?.(rows.find((item) => item.productId === id))
      }}
      onSearchChange={search.onSearchChange}
      filterMode="remote"
      loading={query.isFetching}
      emptyLabel={
        query.isError
          ? getErrorMessage(query.error, "公司 SKU 加载失败，请重试")
          : emptyLabel
      }
    />
  )
}

export type MallSearchComboboxProps = Omit<
  OptionComboboxProps,
  "options" | "loading" | "filterMode" | "onSearchChange"
> & { purpose?: "filter" | "form" }

/** 商城来源系统选项；组件自包含取数并共享 TanStack Query 缓存。 */
export function MallSearchCombobox({
  purpose = "filter",
  emptyLabel,
  ...props
}: MallSearchComboboxProps) {
  const query = useMallSelectorQuery(purpose)
  return (
    <OptionCombobox
      {...props}
      options={(query.data ?? []).map((item) => ({
        value: item.id,
        label: item.name,
        keywords: item.code,
      }))}
      loading={query.isFetching}
      emptyLabel={
        query.isError
          ? getErrorMessage(query.error, "商城加载失败，请重试")
          : emptyLabel
      }
    />
  )
}

export type VoucherCategorySearchComboboxProps = SmartProps<
  ProductComboboxProps,
  SellableSkuComboboxItem
>

/** 卡券类目优先使用正式档案；档案未启用时回退公司商品池卡券 SKU。 */
export function VoucherCategorySearchCombobox({
  purpose = "sales-order",
  selectedItem,
  onItemChange,
  emptyLabel,
  value,
  onValueChange,
  ...props
}: VoucherCategorySearchComboboxProps) {
  const query = useVoucherCategorySelectorQuery(purpose)
  const rows = mergeSelected(
    query.data,
    selectedItem,
    (item) => item.productId,
  )
  return (
    <ProductCombobox
      {...props}
      value={value}
      products={rows}
      onValueChange={(id) => {
        onValueChange(id)
        onItemChange?.(rows.find((item) => item.productId === id))
      }}
      loading={query.isFetching}
      emptyLabel={
        query.isError
          ? getErrorMessage(query.error, "卡券类目加载失败，请重试")
          : emptyLabel
      }
    />
  )
}
