import {
    isLeafRegion,
    regionFullName,
    type RegionIndex,
} from "@/features/regions"
import type { RegionNode } from "@/features/regions"

export type SmartParsedAddress = {
    recipient: string
    phone: string
    region_code: string
    region_name: string
    address: string
    matchedFields: ("recipient" | "phone" | "region" | "address")[]
}

const MUNICIPALITIES = new Set(["北京市", "天津市", "上海市", "重庆市"])

const PLACEHOLDER_CITY_NAMES = new Set([
    "市辖区",
    "县",
    "省直辖县级行政区划",
    "自治区直辖县级行政区划",
])

const ADDRESS_HINT_RE =
    /(?:路|街|道|号|栋|幢|座|层|楼|室|弄|巷|村|镇|乡|园|广场|大厦|中心|小区|公寓|工业|科技|公司|厂|馆|院|站|港|湾|城|大道|胡同|单元|门|组|队|区|县|市|省|\d)/

function shortRegionName(name: string): string {
    return name.replace(
        /(?:壮族自治区|回族自治区|维吾尔自治区|特别行政区|自治区|自治州|地区|盟|省|市|区|县)$/,
        "",
    )
}

function escapeRegExp(value: string): string {
    return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
}

type NodeMatch = {
    node: RegionNode
    index: number
    length: number
}

function findNodeInText(
    text: string,
    nodes: readonly RegionNode[],
    startFrom = 0,
): NodeMatch | null {
    const slice = text.slice(startFrom)
    if (!slice) return null

    // 优先匹配完整名称（按长度降序，避免短名误截长名）。
    const sorted = [...nodes].sort((a, b) => b.name.length - a.name.length)
    let best: NodeMatch | null = null

    for (const node of sorted) {
        const pos = slice.indexOf(node.name)
        if (pos >= 0) {
            const candidate: NodeMatch = {
                node,
                index: startFrom + pos,
                length: node.name.length,
            }
            if (
                !best ||
                candidate.index < best.index ||
                (candidate.index === best.index &&
                    candidate.length > best.length)
            ) {
                best = candidate
            }
        }
    }
    if (best) return best

    // 回退匹配简称（至少 2 个字，避免单字误命中）。
    for (const node of sorted) {
        const short = shortRegionName(node.name)
        if (short.length < 2 || short === node.name) continue
        const pos = slice.indexOf(short)
        if (pos >= 0) {
            const candidate: NodeMatch = {
                node,
                index: startFrom + pos,
                length: short.length,
            }
            if (
                !best ||
                candidate.index < best.index ||
                (candidate.index === best.index &&
                    candidate.length > best.length)
            ) {
                best = candidate
            }
        }
    }
    return best
}

type RegionExtract = {
    code: string
    name: string
    spans: { start: number; end: number }[]
}

function extractRegionFromText(
    text: string,
    regions: RegionIndex,
): RegionExtract | null {
    // 1. 先尝试从省级逐级向下匹配
    const provinceMatch = findNodeInText(text, regions.provinces)
    if (provinceMatch) {
        const province = provinceMatch.node
        const spans: { start: number; end: number }[] = [
            {
                start: provinceMatch.index,
                end: provinceMatch.index + provinceMatch.length,
            },
        ]
        if (isLeafRegion(regions, province.code)) {
            return {
                code: province.code,
                name: regionFullName(regions, province.code),
                spans,
            }
        }
        const afterProvince = provinceMatch.index + provinceMatch.length
        const cityCandidates = regions.children.get(province.code) ?? []

        // 直辖市（如 北京市 朝阳区 / 北京市 北京市 朝阳区）
        if (MUNICIPALITIES.has(province.name)) {
            let searchOffset = afterProvince
            // 跳过可能重复出现的直辖市名或“市辖区”
            const dupRe = new RegExp(
                `^[\\s,，、/-]*(?:${escapeRegExp(province.name)}|${escapeRegExp(shortRegionName(province.name))}|市辖区)`,
            )
            const dupMatch = text.slice(searchOffset).match(dupRe)
            if (dupMatch) {
                spans.push({
                    start: searchOffset,
                    end: searchOffset + dupMatch[0].length,
                })
                searchOffset += dupMatch[0].length
            }
            const districts = cityCandidates.flatMap(
                (city) => regions.children.get(city.code) ?? [],
            )
            const districtMatch = findNodeInText(text, districts, searchOffset)
            if (
                districtMatch &&
                isLeafRegion(regions, districtMatch.node.code)
            ) {
                spans.push({
                    start: districtMatch.index,
                    end: districtMatch.index + districtMatch.length,
                })
                return {
                    code: districtMatch.node.code,
                    name: regionFullName(regions, districtMatch.node.code),
                    spans,
                }
            }
        }

        const realCities = cityCandidates.filter(
            (node) => !PLACEHOLDER_CITY_NAMES.has(node.name),
        )
        const cityMatch = findNodeInText(text, realCities, afterProvince)
        if (cityMatch) {
            spans.push({
                start: cityMatch.index,
                end: cityMatch.index + cityMatch.length,
            })
            if (isLeafRegion(regions, cityMatch.node.code)) {
                return {
                    code: cityMatch.node.code,
                    name: regionFullName(regions, cityMatch.node.code),
                    spans,
                }
            }
            const districts = regions.children.get(cityMatch.node.code) ?? []
            const districtMatch = findNodeInText(
                text,
                districts,
                cityMatch.index + cityMatch.length,
            )
            if (
                districtMatch &&
                isLeafRegion(regions, districtMatch.node.code)
            ) {
                spans.push({
                    start: districtMatch.index,
                    end: districtMatch.index + districtMatch.length,
                })
                return {
                    code: districtMatch.node.code,
                    name: regionFullName(regions, districtMatch.node.code),
                    spans,
                }
            }
        }

        // 省直辖县级行政区划（如 湖北省仙桃市 / 海南省琼海市）或省略市级直接写区县
        const allProvinceDistricts = cityCandidates.flatMap((city) =>
            isLeafRegion(regions, city.code)
                ? [city]
                : (regions.children.get(city.code) ?? []),
        )
        const directDistrict = findNodeInText(
            text,
            allProvinceDistricts,
            afterProvince,
        )
        if (directDistrict && isLeafRegion(regions, directDistrict.node.code)) {
            spans.push({
                start: directDistrict.index,
                end: directDistrict.index + directDistrict.length,
            })
            return {
                code: directDistrict.node.code,
                name: regionFullName(regions, directDistrict.node.code),
                spans,
            }
        }
    }

    // 2. 未写省份时，尝试按“市 + 区县”或叶子市唯一匹配（如 深圳市南山区 / 东莞市）
    const allCities: RegionNode[] = []
    for (const province of regions.provinces) {
        if (MUNICIPALITIES.has(province.name)) continue
        const children = regions.children.get(province.code) ?? []
        for (const city of children) {
            if (!PLACEHOLDER_CITY_NAMES.has(city.name)) {
                allCities.push(city)
            }
        }
    }
    const cityMatch = findNodeInText(text, allCities)
    if (cityMatch) {
        const spans = [
            {
                start: cityMatch.index,
                end: cityMatch.index + cityMatch.length,
            },
        ]
        if (isLeafRegion(regions, cityMatch.node.code)) {
            return {
                code: cityMatch.node.code,
                name: regionFullName(regions, cityMatch.node.code),
                spans,
            }
        }
        const districts = regions.children.get(cityMatch.node.code) ?? []
        const districtMatch = findNodeInText(
            text,
            districts,
            cityMatch.index + cityMatch.length,
        )
        if (districtMatch && isLeafRegion(regions, districtMatch.node.code)) {
            spans.push({
                start: districtMatch.index,
                end: districtMatch.index + districtMatch.length,
            })
            return {
                code: districtMatch.node.code,
                name: regionFullName(regions, districtMatch.node.code),
                spans,
            }
        }
    }

    return null
}

function removeSpans(
    text: string,
    spans: readonly { start: number; end: number }[],
): string {
    if (!spans.length) return text
    const ordered = [...spans].sort((a, b) => a.start - b.start)
    let cursor = 0
    let result = ""
    for (const span of ordered) {
        if (span.start > cursor) {
            result += text.slice(cursor, span.start)
        }
        result += " "
        cursor = Math.max(cursor, span.end)
    }
    if (cursor < text.length) {
        result += text.slice(cursor)
    }
    return result
}

function extractLabeledValue(text: string, labelPattern: RegExp): string {
    const match = text.match(labelPattern)
    return match?.[1]?.trim() ?? ""
}

function looksLikeRecipient(token: string): boolean {
    const cleaned = token.trim()
    if (!cleaned || cleaned.length < 2 || cleaned.length > 16) return false
    if (ADDRESS_HINT_RE.test(cleaned)) return false
    // 中文姓名或英文/拼音姓名
    return /^[\u4e00-\u9fa5·]{2,8}$|^[A-Za-z][A-Za-z\s.'-]{1,15}$/.test(cleaned)
}

/**
 * 解析微信/邮件/Excel 粘贴的一段收货文本，提取收货人、电话、最末级地区与详细地址。
 * 不确定时不强行覆盖，返回识别出的字段列表便于界面反馈。
 */
export function parseSmartAddressText(
    raw: string,
    regions?: RegionIndex,
): SmartParsedAddress {
    const normalized = raw
        .replace(/\r\n?/g, "\n")
        .replace(/[：]/g, ":")
        .replace(/[，、；;]/g, " ")
        .trim()

    if (!normalized) {
        return {
            recipient: "",
            phone: "",
            region_code: "",
            region_name: "",
            address: "",
            matchedFields: [],
        }
    }

    // 1. 先检查是否有显式标签指定的收货人
    const labeledRecipient = extractLabeledValue(
        normalized,
        /(?:收货人|收件人|联系人|姓名)\s*:\s*([^\n\t,，;；\s]{1,20})/,
    )

    // 2. 提取手机号或固话
    let phone = ""
    let working = normalized
    const mobileMatch = working.match(
        /(?<!\d)(1[3-9]\d[\s-]?\d{4}[\s-]?\d{4})(?!\d)/,
    )
    const landlineMatch = !mobileMatch
        ? working.match(
              /(?<!\d)(0\d{2,3}[-\s]?\d{7,8}|400[-\s]?\d{3}[-\s]?\d{4})(?!\d)/,
          )
        : null
    const phoneMatch = mobileMatch ?? landlineMatch
    if (phoneMatch) {
        phone = phoneMatch[1]!.replace(/[\s]/g, "")
        working = `${working.slice(0, phoneMatch.index)} ${working.slice((phoneMatch.index ?? 0) + phoneMatch[0].length)}`
    }

    // 3. 清理常见前缀标签
    working = working.replace(
        /(?:收货人|收件人|联系人|姓名|联系电话|手机号码|手机号|电话|手机|收货地址|收件地址|详细地址|所在地区|地址)\s*:/g,
        " ",
    )

    // 4. 匹配省市区县
    let regionCode = ""
    let regionName = ""
    if (regions) {
        const extracted = extractRegionFromText(working, regions)
        if (extracted) {
            regionCode = extracted.code
            regionName = extracted.name
            working = removeSpans(working, extracted.spans)
        }
    }

    // 5. 拆分剩余片段为收货人与详细地址
    const tokens = working
        .split(/[\n\t]+|\s{1,}/)
        .map((part) =>
            part.replace(/^[,，。:：\s-]+|[,，。:：\s-]+$/g, "").trim(),
        )
        .filter(Boolean)

    let recipient = labeledRecipient
    let remainingTokens = tokens
    if (recipient) {
        const index = remainingTokens.indexOf(recipient)
        if (index >= 0) {
            remainingTokens = remainingTokens.filter((_, i) => i !== index)
        }
    } else if (remainingTokens.length >= 2) {
        // 优先从首尾片段找姓名（最常见格式：“张三 ...地址”或“...地址 张三”）
        const first = remainingTokens[0]!
        const last = remainingTokens[remainingTokens.length - 1]!
        if (looksLikeRecipient(first)) {
            recipient = first
            remainingTokens = remainingTokens.slice(1)
        } else if (looksLikeRecipient(last)) {
            recipient = last
            remainingTokens = remainingTokens.slice(0, -1)
        } else {
            const candidateIndex = remainingTokens.findIndex(looksLikeRecipient)
            if (candidateIndex >= 0) {
                recipient = remainingTokens[candidateIndex]!
                remainingTokens = remainingTokens.filter(
                    (_, i) => i !== candidateIndex,
                )
            }
        }
    } else if (
        remainingTokens.length === 1 &&
        looksLikeRecipient(remainingTokens[0]!)
    ) {
        recipient = remainingTokens[0]!
        remainingTokens = []
    }

    const address = remainingTokens.join(" ").trim()
    const matchedFields: SmartParsedAddress["matchedFields"] = []
    if (recipient) matchedFields.push("recipient")
    if (phone) matchedFields.push("phone")
    if (regionCode) matchedFields.push("region")
    if (address) matchedFields.push("address")

    return {
        recipient,
        phone,
        region_code: regionCode,
        region_name: regionName,
        address,
        matchedFields,
    }
}
