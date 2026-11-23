import type { RegionNode } from "../api"
import { isLeafRegion, type RegionIndex } from "./region-index"

const shortName = (name: string) =>
    name.replace(
        /(?:壮族自治区|回族自治区|维吾尔自治区|特别行政区|自治区|省|市|区|县)$/,
        "",
    )
function matching(nodes: readonly RegionNode[], name: string) {
    const exact = nodes.filter((node) => node.name === name)
    return exact.length
        ? exact
        : nodes.filter((node) => shortName(node.name) === shortName(name))
}

/** 按省、市、区逐级匹配；直辖市可填“北京市 北京市 朝阳区”。 */
export function resolveAddressRegion(
    index: RegionIndex,
    province: string,
    city: string,
    district: string,
): string {
    if (!province || !city) throw new Error("省、市必须填写完整。")
    const provinces = matching(index.provinces, province)
    if (provinces.length !== 1)
        throw new Error("省份无法唯一匹配，请填写完整名称。")
    const children = index.children.get(provinces[0].code) ?? []
    let cities = matching(children, city)
    if (
        !cities.length &&
        ["北京市", "天津市", "上海市", "重庆市"].includes(provinces[0].name) &&
        shortName(city) === shortName(provinces[0].name)
    ) {
        cities = children.filter(
            (node) => node.name === "市辖区" || node.name === "县",
        )
    }
    if (!cities.length) throw new Error("城市与省份不匹配，请核对。")
    if (cities.length === 1 && isLeafRegion(index, cities[0].code)) {
        if (
            !district ||
            ["—", "-", "无", "市辖区"].includes(district) ||
            matching(cities, district).length === 1
        )
            return cities[0].code
        throw new Error(
            "当前区划目录仅到该市，区县请填“—”，街道或镇请写入详细地址。",
        )
    }
    if (!district) throw new Error("请填写区县。")
    const districts = matching(
        cities.flatMap((node) => index.children.get(node.code) ?? []),
        district,
    )
    if (districts.length !== 1 || !isLeafRegion(index, districts[0].code))
        throw new Error("区县与省市不匹配或不唯一，请核对完整名称。")
    return districts[0].code
}
