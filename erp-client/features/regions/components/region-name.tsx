"use client"

import { useRegionIndex } from "../hooks/queries"
import { regionFullName } from "../lib/region-index"

/** 按区划代码显示完整地区名称；区划未加载时显示代码。 */
export function RegionName({ code }: { code: string }) {
    const { data: index } = useRegionIndex()
    return <>{index ? regionFullName(index, code) : code}</>
}
