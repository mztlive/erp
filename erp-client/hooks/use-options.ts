"use client"

import { useQuery } from "@tanstack/react-query"

import {
  fetchOwnerOptions,
  fetchSupplierOptions,
  fetchTeamOptions,
  fetchUnitOptions,
} from "@/lib/options"

/** 跨工作面共享选项查询的 queryKey。 */
export const optionKeys = {
  suppliers: ["options", "suppliers"] as const,
  parties: ["options", "parties"] as const,
  owners: ["options", "owners"] as const,
  team: ["options", "team"] as const,
  units: ["options", "units"] as const,
}

/** 选项类数据缓存 5 分钟，避免列表/表单频繁触发请求。 */
const OPTIONS_STALE_TIME = 5 * 60 * 1000

/** 供应商选项（启用状态）。 */
export function useSupplierOptionsQuery() {
  return useQuery({
    queryKey: optionKeys.suppliers,
    queryFn: fetchSupplierOptions,
    staleTime: OPTIONS_STALE_TIME,
  })
}

/** 负责人选项（账号列表）。 */
export function useOwnerOptionsQuery() {
  return useQuery({
    queryKey: optionKeys.owners,
    queryFn: fetchOwnerOptions,
    staleTime: OPTIONS_STALE_TIME,
  })
}

/** 任务转交候选（账号列表）。 */
export function useTeamOptionsQuery() {
  return useQuery({
    queryKey: optionKeys.team,
    queryFn: fetchTeamOptions,
    staleTime: OPTIONS_STALE_TIME,
  })
}

/** 计量单位选项（启用状态）。 */
export function useUnitOptionsQuery() {
  return useQuery({
    queryKey: optionKeys.units,
    queryFn: fetchUnitOptions,
    staleTime: OPTIONS_STALE_TIME,
  })
}
