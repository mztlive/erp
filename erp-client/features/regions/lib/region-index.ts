import type { RegionNode } from "../api"

/** 名称中不表达地理层级的市级占位，拼接完整名称时跳过。 */
const PLACEHOLDER_CITY_NAMES = new Set([
    "市辖区",
    "县",
    "省直辖县级行政区划",
    "自治区直辖县级行政区划",
])

export type RegionIndex = Readonly<{
    provinces: readonly RegionNode[]
    byCode: ReadonlyMap<string, RegionNode>
    children: ReadonlyMap<string, readonly RegionNode[]>
}>

export function buildRegionIndex(nodes: readonly RegionNode[]): RegionIndex {
    const byCode = new Map<string, RegionNode>()
    const children = new Map<string, RegionNode[]>()
    const provinces: RegionNode[] = []
    for (const node of nodes) {
        byCode.set(node.code, node)
        if (!node.parent_code) {
            provinces.push(node)
            continue
        }
        const siblings = children.get(node.parent_code) ?? []
        siblings.push(node)
        children.set(node.parent_code, siblings)
    }
    return { provinces, byCode, children }
}

/** 由省到本级的区划链；代码不存在时为空。 */
export function regionPath(index: RegionIndex, code: string): RegionNode[] {
    const path: RegionNode[] = []
    let current = index.byCode.get(code)
    while (current) {
        path.unshift(current)
        current = current.parent_code
            ? index.byCode.get(current.parent_code)
            : undefined
    }
    return path
}

/** 完整地区名称，例如「广东省深圳市南山区」；代码不存在时返回代码本身。 */
export function regionFullName(index: RegionIndex, code: string): string {
    const names = regionPath(index, code)
        .map((node) => node.name)
        .filter((name) => !PLACEHOLDER_CITY_NAMES.has(name))
    return names.length ? names.join("") : code
}

/** 是否为没有下级的最末级区划。 */
export function isLeafRegion(index: RegionIndex, code: string): boolean {
    return index.byCode.has(code) && !index.children.has(code)
}
