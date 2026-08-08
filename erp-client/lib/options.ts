/**
 * 跨工作面复用的实体选项取数（真实 HTTP，供 Combobox 下拉消费）。
 *
 * - 供应商：/admin/suppliers + 主体名称解析
 * - 结算主体：/admin/parties + 生效修订法律名称
 * - 负责人 / 转交候选：/admin/admins（账号列表）
 */

import type {
  OwnerComboboxItem,
  SettlementPartyComboboxItem,
  SupplierComboboxItem,
} from "@/components/business/entity-comboboxes"
import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

/** 选项类接口的单页条数上限（后端 1–100）。 */
const OPTIONS_PAGE_SIZE = 100

type BackendPage<T> = Page<T>

type EnableStatus = "active" | "disabled"

type SupplierDto = {
  id: string
  party_id: string
  party_no: string | null
  legal_name: string | null
  short_name: string | null
  supplier_no: string
  status: EnableStatus
  created_at: number
  version: number
}

type PartyDto = {
  id: string
  party_no: string
  party_kind: string
  status: string
  current_revision_id: string | null
  created_at: number
  version: number
}

type PartyRevisionDto = {
  id: string
  revision_no: number
  legal_name: string
  short_name: string | null
  effective_from: string
  effective_to: string | null
  version: number
  created_at: number
}

type AdminItem = {
  id: string
  account: string
  name: string
  role_ids: string[]
  created_at: number
}

type UnitOfMeasureDto = {
  id: string
  unit_code: string
  name: string
  symbol: string
  quantity_scale: number
  status: EnableStatus
  created_at: number
  version: number
}

/** 拉取指定路径的全部分页数据。 */
async function fetchAllPages<T>(
  path: string,
  query: Record<string, unknown> = {}
): Promise<T[]> {
  const items: T[] = []
  let page = 1
  let total = Number.POSITIVE_INFINITY
  while (items.length < total) {
    const result = await apiGet<BackendPage<T>>(path, {
      ...query,
      page,
      page_size: OPTIONS_PAGE_SIZE,
    })
    items.push(...result.items)
    total = result.total
    if (result.items.length === 0) break
    page += 1
    if (page > 50) break
  }
  return items
}

/** 取主体当前生效修订的法律名称；取不到时回退主体编号。 */
async function resolvePartyName(
  party: PartyDto
): Promise<string> {
  if (!party.current_revision_id) return party.party_no
  try {
    const revPage = await apiGet<BackendPage<PartyRevisionDto>>(
      `/admin/parties/${party.id}/revisions`,
      { page: 1, page_size: 1, sort_by: "revision_no", sort_dir: "desc" }
    )
    const rev = revPage.items[0]
    return rev?.legal_name ?? party.party_no
  } catch {
    return party.party_no
  }
}

/**
 * 拉取启用状态的供应商选项（名称来自其主体当前生效修订）。
 *
 * @returns Combobox 供应商选项列表。
 */
export const fetchSupplierOptions = async (): Promise<SupplierComboboxItem[]> => {
  const suppliers = await fetchAllPages<SupplierDto>("/admin/suppliers", {
    status: "active",
  })
  return suppliers.map((s) => ({
    supplierId: s.id,
    supplierName: s.legal_name ?? s.short_name ?? s.party_no ?? s.supplier_no,
    supplierCode: s.supplier_no,
  }))
}

/**
 * 拉取结算主体选项（名称取当前生效修订的法律名称）。
 *
 * @returns Combobox 结算主体选项列表。
 */
export const fetchPartyOptions = async (): Promise<SettlementPartyComboboxItem[]> => {
  const parties = await fetchAllPages<PartyDto>("/admin/parties", {})
  const rows: SettlementPartyComboboxItem[] = []
  for (const party of parties) {
    rows.push({
      partyId: party.id,
      displayName: await resolvePartyName(party),
      partyCode: party.party_no,
    })
  }
  return rows
}

/**
 * 拉取负责人选项（管理后台账号列表）。
 *
 * @returns Combobox 负责人选项列表。
 */
export const fetchOwnerOptions = async (): Promise<OwnerComboboxItem[]> => {
  const admins = await apiGet<AdminItem[]>("/admin/admins", {})
  return admins.map((a) => ({
    userId: a.id,
    displayName: a.name,
    userCode: a.account,
  }))
}

/**
 * 拉取任务转交候选（管理后台账号列表，与负责人同一数据源）。
 *
 * @returns Combobox 团队人选列表。
 */
export const fetchTeamOptions = async (): Promise<OwnerComboboxItem[]> =>
  fetchOwnerOptions()

/**
 * 拉取启用状态的计量单位选项（供商品表单选择基础单位）。
 *
 * @returns 计量单位选项列表（id / code / label）。
 */
export const fetchUnitOptions = async (): Promise<
  Array<{ id: string; code: string; label: string }>
> => {
  const units = await fetchAllPages<UnitOfMeasureDto>("/admin/unit-of-measures", {
    status: "active",
  })
  return units.map((u) => ({
    id: u.id,
    code: u.unit_code,
    label: u.symbol || u.name,
  }))
}
