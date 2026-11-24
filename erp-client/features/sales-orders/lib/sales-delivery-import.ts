import { resolveAddressRegion } from "@/features/regions/lib/address-region"
import {
    isLeafRegion,
    regionFullName,
    type RegionIndex,
} from "@/features/regions/lib/region-index"
import { compactFixed, compareDecimal } from "@/lib/fixed-decimal"
import type {
    SalesDeliveryAllocation,
    SalesOrderDraftLineInput,
} from "../types"
import { allocationSummary } from "./sales-delivery-allocation-summary"
import { deliveryAllocationsSchema } from "./sales-order-create-validation"

export const addressColumns = [
    ["收货人", "收件人", "收货人姓名", "收件人姓名"],
    [
        "联系电话",
        "电话",
        "手机",
        "手机号",
        "手机号码",
        "收货人电话",
        "收件人电话",
    ],
    ["省", "省份"],
    ["市", "城市"],
    ["区县", "区", "区/县", "县区"],
    ["详细地址", "收货地址", "收件地址"],
    ["配送数量", "分配数量", "数量"],
] as const

export type DeliveryImportRow = {
    row: number
    lineKey: string
    label: string
    cells: string[]
    addressId?: string
    errors: string[]
}

export type DeliveryImportSource = {
    rows: DeliveryImportRow[]
    baselines?: ReadonlyMap<string, string>
}

export type DeliveryWorkbookContext = {
    scope: string
    customerId: string
    lines: SalesOrderDraftLineInput[]
    unitNames?: ReadonlyMap<string, string>
}

export type DeliveryImportIssue = {
    row: number
    label: string
    cells: string[]
    message: string
}

type DeliveryChange = {
    before?: SalesDeliveryAllocation
    after?: SalesDeliveryAllocation
}

export type DeliveryImportGroup = {
    line: SalesOrderDraftLineInput
    unitName: string
    label: string
    changes: DeliveryChange[]
    allocations: SalesDeliveryAllocation[]
    summary: ReturnType<typeof allocationSummary>
}

export function addressKey(row: SalesDeliveryAllocation) {
    return JSON.stringify(
        [row.recipient, row.phone, row.region_code, row.address].map((value) =>
            value.trim(),
        ),
    )
}

function allocationKey(row: SalesDeliveryAllocation) {
    return JSON.stringify([addressKey(row), compactFixed(row.quantity.trim())])
}

async function digest(value: unknown) {
    const buffer = await crypto.subtle.digest(
        "SHA-256",
        new TextEncoder().encode(JSON.stringify(value)),
    )
    return Array.from(new Uint8Array(buffer), (byte) =>
        byte.toString(16).padStart(2, "0"),
    ).join("")
}

export function workbookScope(context: DeliveryWorkbookContext) {
    return digest([context.scope, context.customerId])
}

export function allocationBaseline(line: SalesOrderDraftLineInput) {
    return digest((line.deliveryAllocations ?? []).map(allocationKey))
}

export function productIdentity(line: SalesOrderDraftLineInput) {
    return JSON.stringify([line.sku, line.skuRevisionId, line.unit])
}

export function workbookSnapshot(context: DeliveryWorkbookContext) {
    return JSON.stringify([
        context.scope,
        context.customerId,
        context.lines.map((line) => [
            line.rowKey,
            productIdentity(line),
            line.name,
            line.specification,
            line.quantity,
            line.deliveryAllocations ?? [],
        ]),
    ])
}

export function productLabel(line: SalesOrderDraftLineInput, index: number) {
    return [index + 1, line.name, line.specification]
        .filter(Boolean)
        .join(" · ")
}

export function isWorkbookProduct(line: SalesOrderDraftLineInput) {
    return Boolean(line.sku && line.skuRevisionId && line.name.trim())
}

function parseAllocation(
    cells: string[],
    regions: RegionIndex,
    fillOne: boolean,
) {
    const [recipient, phone, province, city, district, address, quantity] =
        cells
    if (/^\d+(?:\.\d+)?e[+-]?\d+$/i.test(phone))
        throw new Error("联系电话使用了科学计数法，请改为文本格式")
    if (!quantity && !fillOne) throw new Error("请填写配送数量")
    const provinceNode = regions.provinces.find(
        (node) => node.name === province,
    )
    const code =
        provinceNode &&
        !city &&
        !district &&
        isLeafRegion(regions, provinceNode.code)
            ? provinceNode.code
            : resolveAddressRegion(regions, province, city, district)
    const result = deliveryAllocationsSchema.safeParse([
        {
            recipient,
            phone,
            region_code: code,
            region_name: regionFullName(regions, code),
            address,
            quantity: quantity || (fillOne ? "1" : ""),
        },
    ])
    if (!result.success) {
        const labels: Record<string, string> = {
            recipient: "收货人",
            phone: "联系电话",
            address: "详细地址",
            quantity: "配送数量",
        }
        throw new Error(
            result.error.issues
                .map((issue) => {
                    const field = labels[String(issue.path[1])]
                    return issue.code === "too_big" && field
                        ? `${field}最多 ${issue.maximum} 字`
                        : issue.message
                })
                .join("，"),
        )
    }
    return result.data![0]
}

function sameQuantity(left: string, right: string) {
    try {
        return compareDecimal(left, right, 6) === 0
    } catch {
        return left === right
    }
}

/** 先按原收件资料找同一地址，再用导出行标识配对修改后的地址。 */
function addressChanges(
    previous: SalesDeliveryAllocation[],
    incoming: { allocation: SalesDeliveryAllocation; addressId?: string }[],
) {
    const remaining = new Set(previous.map((_, index) => index))
    const paired = incoming.map(({ allocation }) => {
        const index = previous.findIndex(
            (row, position) =>
                remaining.has(position) &&
                addressKey(row) === addressKey(allocation),
        )
        remaining.delete(index)
        return index
    })
    const changes: DeliveryChange[] = []
    incoming.forEach(({ allocation, addressId }, index) => {
        let beforeIndex = paired[index]
        if (beforeIndex < 0 && addressId) {
            beforeIndex = previous.findIndex(
                (_, position) =>
                    remaining.has(position) &&
                    String(position + 1) === addressId,
            )
            remaining.delete(beforeIndex)
        }
        const before = previous[beforeIndex]
        if (
            !before ||
            addressKey(before) !== addressKey(allocation) ||
            !sameQuantity(before.quantity, allocation.quantity)
        )
            changes.push({ before, after: allocation })
    })
    for (const index of remaining) changes.push({ before: previous[index] })
    return changes
}

export async function buildDeliveryImportPreview(
    source: DeliveryImportSource,
    lines: SalesOrderDraftLineInput[],
    regions: RegionIndex,
    fillOne: boolean,
    unitNames?: ReadonlyMap<string, string>,
) {
    const issues: DeliveryImportIssue[] = []
    const buckets = new Map<string, DeliveryImportRow[]>()
    for (const row of source.rows) {
        if (row.errors.length) {
            issues.push({ ...row, message: row.errors.join("，") })
            continue
        }
        const bucket = buckets.get(row.lineKey) ?? []
        bucket.push(row)
        buckets.set(row.lineKey, bucket)
    }
    const groups: DeliveryImportGroup[] = []
    let skipped = 0
    for (const [key, rows] of buckets) {
        const line = lines.find((item) => item.rowKey === key)
        if (!line) continue
        const label = rows[0].label
        const previous = line.deliveryAllocations ?? []
        const incoming = new Map<
            string,
            { allocation: SalesDeliveryAllocation; addressId?: string }
        >()
        const previousByKey = new Map(
            previous.map((allocation) => [addressKey(allocation), allocation]),
        )
        for (const row of rows) {
            try {
                const allocation = parseAllocation(row.cells, regions, fillOne)
                const signature = addressKey(allocation)
                const duplicate =
                    incoming.get(signature)?.allocation ??
                    (!source.baselines
                        ? previousByKey.get(signature)
                        : undefined)
                if (duplicate) {
                    if (!sameQuantity(duplicate.quantity, allocation.quantity))
                        throw new Error(
                            "相同地址的配送数量有冲突，请核对后修改",
                        )
                    skipped++
                    continue
                }
                incoming.set(signature, {
                    allocation,
                    addressId: row.addressId,
                })
            } catch (error) {
                issues.push({ ...row, message: (error as Error).message })
            }
        }
        const imported = [...incoming.values()]
        const allocations = [
            ...(source.baselines ? [] : previous),
            ...imported.map((entry) => entry.allocation),
        ]
        const changes = source.baselines
            ? addressChanges(previous, imported)
            : imported.map(({ allocation }) => ({ after: allocation }))
        if (
            source.baselines &&
            changes.length &&
            source.baselines.get(key) !== (await allocationBaseline(line))
        )
            issues.push({
                ...rows[0],
                message: "页面已修改该商品的地址，请重新导出收货安排后再编辑",
            })
        if (allocations.length > 100)
            issues.push({ ...rows[0], message: "每条商品最多分配 100 个地址" })
        const unitName = unitNames?.get(line.unit.trim()) ?? ""
        groups.push({
            line,
            unitName,
            label,
            changes,
            allocations,
            summary: allocationSummary({
                ...line,
                unit: unitName,
                deliveryAllocations: allocations,
            }),
        })
    }
    return {
        groups,
        issues: issues.sort((left, right) => left.row - right.row),
        skipped,
    }
}

export type DeliveryImportPreview = Awaited<
    ReturnType<typeof buildDeliveryImportPreview>
>
