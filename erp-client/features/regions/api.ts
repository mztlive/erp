import { apiGet } from "@/lib/api"

/** 一条行政区划；省级 `parent_code` 为空。 */
export type RegionNode = Readonly<{
    code: string
    name: string
    parent_code: string | null
}>

/** 读取内置行政区划全集（省、市、区县三级）。 */
export function fetchRegions(): Promise<RegionNode[]> {
    return apiGet<RegionNode[]>("/account/regions")
}
