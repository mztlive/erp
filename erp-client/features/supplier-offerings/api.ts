import { apiGet, apiPost } from "@/lib/api/client"

import type {
  CompanySkuOption,
  CreateSupplierOfferingInput,
  ReviseSupplierOfferingInput,
  SupplierOfferingListQuery,
  SupplierOfferingPage,
  UpdateOfferingAvailabilityInput,
} from "@/features/supplier-offerings/types"

type BackendPage<T> = Readonly<{
  items: readonly T[]
  total: number
  page: number
  page_size: number
}>

type BackendSku = Readonly<{
  id: string
  sku_no: string
  specification_signature: string
  status: string
}>

async function fetchAllPages<T>(path: string): Promise<readonly T[]> {
  const rows: T[] = []
  let page = 1
  let total = Number.POSITIVE_INFINITY
  while (rows.length < total) {
    const result = await apiGet<BackendPage<T>>(path, {
      page,
      page_size: 100,
    })
    rows.push(...result.items)
    total = result.total
    if (result.items.length === 0) break
    page += 1
  }
  return rows
}

export function fetchSupplierOfferings(
  query: SupplierOfferingListQuery
): Promise<SupplierOfferingPage> {
  return apiGet<SupplierOfferingPage>("/admin/supplier-offerings", {
    q: query.q?.trim() || undefined,
    sku_id: query.skuId || undefined,
    supplier_id: query.supplierId || undefined,
    status: query.status || undefined,
    page: query.page ?? 1,
    page_size: query.pageSize ?? 50,
    sort_by: "created_at",
    sort_dir: "desc",
  })
}

export async function fetchCompanySkuOptions(): Promise<readonly CompanySkuOption[]> {
  const rows = await fetchAllPages<BackendSku>("/admin/skus")
  return rows
    .filter((row) => row.status.toUpperCase() === "ACTIVE")
    .map((row) => ({
      id: row.id,
      skuNo: row.sku_no,
      specification: row.specification_signature,
    }))
    .sort((left, right) => left.skuNo.localeCompare(right.skuNo, "zh-CN"))
}

export function createSupplierOffering(input: CreateSupplierOfferingInput) {
  return apiPost<{
    offering_id: string
    revision_id: string
    availability_id: string
    revision_no: number
    status: string
  }>("/admin/supplier-offerings", input)
}

export function reviseSupplierOffering(input: ReviseSupplierOfferingInput) {
  const { offeringId, ...body } = input
  return apiPost<{
    offering_id: string
    revision_id: string
    revision_no: number
    status: string
    version: number
  }>(
    `/admin/supplier-offerings/${encodeURIComponent(offeringId)}/revisions`,
    body
  )
}

export function updateSupplierOfferingAvailability(
  input: UpdateOfferingAvailabilityInput
) {
  const { offeringId, ...body } = input
  return apiPost<{
    offering_id: string
    availability_status: string
    availability_version: number
    source_updated_at: number
  }>(
    `/admin/supplier-offerings/${encodeURIComponent(offeringId)}/availability`,
    body
  )
}
