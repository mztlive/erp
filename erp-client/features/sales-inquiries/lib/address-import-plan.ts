import { compactFixed, compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import type { RegionIndex } from "@/features/regions/lib/region-index"
import type { InquiryAllocation, InquiryLine, SalesInquiry } from "../types"
import {
    validateAddressImportRow,
    type AddressImportRow,
} from "./address-text-import"

export type AddressImportChoice = {
    signature: string
    mode: "keep" | "replace" | "sum"
    quantity?: string
}
export type AddressImportChanges = {
    base: string
    additions: InquiryAllocation[]
    updates: { allocation_id: string; quantity: string }[]
}
export type AddressImportGroup = {
    id: string
    rows: AddressImportRow[]
    allocation: InquiryAllocation
    existing: InquiryAllocation[]
    quantities: string[]
    signature: string
    choice?: AddressImportChoice
    resultQuantity?: string
    errors: string[]
    sourceInvalid: boolean
    status: "error" | "conflict" | "skip" | "add" | "update"
    skipped: number
}

/** 收件资料仅清理字段首尾空格，不用模糊地址或数值电话推测同一收件人。 */
export function allocationKey(row: InquiryAllocation): string {
    return JSON.stringify(
        [row.recipient, row.phone, row.region_code, row.address].map((part) =>
            part.trim(),
        ),
    )
}

function equalQuantity(left: string, right: string, scale: number): boolean {
    try {
        return compareDecimal(left, right, scale) === 0
    } catch {
        return false
    }
}

function defaultChoice(
    group: AddressImportGroup,
    scale: number,
): AddressImportChoice | undefined {
    if (group.errors.length || group.quantities.length !== 1) return undefined
    const quantity = group.quantities[0]
    if (!group.existing.length)
        return { signature: group.signature, mode: "replace", quantity }
    if (
        group.existing.some((row) =>
            equalQuantity(row.quantity, quantity, scale),
        )
    )
        return { signature: group.signature, mode: "keep" }
    return undefined
}

function resolveGroup(
    group: AddressImportGroup,
    choice: AddressImportChoice | undefined,
    scale: number,
    inquiry: SalesInquiry | undefined,
    lineId: string,
) {
    group.choice =
        choice?.signature === group.signature
            ? choice
            : defaultChoice(group, scale)
    if (group.errors.length) return
    if (!group.choice) {
        group.status = "conflict"
        return
    }
    if (group.choice.mode === "keep") {
        group.status = "skip"
        group.skipped = group.rows.length
        group.resultQuantity =
            group.existing.length === 1 ? group.existing[0].quantity : undefined
        return
    }
    if (group.existing.length > 1) {
        group.errors.push(
            "已有多条相同地址，无法确定要修改哪一条；请跳过本组或先在地址列表处理。",
        )
        return
    }
    try {
        const quantity =
            group.choice.mode === "sum"
                ? compactFixed(
                      sumFixed(
                          [
                              ...group.existing.map((row) => row.quantity),
                              ...group.rows.map((row) => row.quantity),
                          ],
                          { maxScale: scale, outputScale: scale },
                      ),
                  )
                : (group.choice.quantity?.trim() ?? "")
        if (compareDecimal(quantity, "0", scale) <= 0)
            throw new Error("配送数量须大于 0")
        const occupied = sumFixed(
            group.existing.length
                ? (inquiry?.quantity_ledger ?? [])
                      .filter((entry) => entry.state !== "released")
                      .flatMap((entry) =>
                          entry.selections
                              .filter(
                                  (item) =>
                                      item.line_id === lineId &&
                                      item.allocation_id ===
                                          group.existing[0].allocation_id,
                              )
                              .map((item) => item.quantity),
                      )
                : [],
            { maxScale: scale, outputScale: scale },
        )
        if (compareDecimal(quantity, occupied, scale) < 0) {
            group.errors.push(
                `结果数量不得少于已占用的 ${compactFixed(occupied)}`,
            )
            return
        }
        group.resultQuantity = compactFixed(quantity)
        const unchanged =
            group.existing.length === 1 &&
            equalQuantity(group.existing[0].quantity, quantity, scale)
        group.status = unchanged
            ? "skip"
            : group.existing.length
              ? "update"
              : "add"
        group.skipped = unchanged
            ? group.rows.length
            : group.choice.mode === "sum"
              ? 0
              : group.rows.length - 1
    } catch {
        group.errors.push(`请填写大于 0 的有效结果数量，最多 ${scale} 位小数。`)
    }
}

/** 同一计划同时驱动预览与提交；跳过项保持原地址身份，更新只改变数量。 */
export function buildAddressImportPlan(
    rows: readonly AddressImportRow[],
    line: InquiryLine,
    regions: RegionIndex,
    choices: Readonly<Record<string, AddressImportChoice>>,
    inquiry?: SalesInquiry,
) {
    const byAddress = new Map<string, AddressImportGroup>()
    for (const row of rows) {
        const result = validateAddressImportRow(
            row,
            regions,
            line.quantity_scale,
        )
        const key = result.errors.length
            ? row.id
            : allocationKey(result.allocation)
        let group = byAddress.get(key)
        if (!group) {
            group = {
                id: row.id,
                rows: [],
                allocation: result.allocation,
                existing: result.errors.length
                    ? []
                    : line.allocations.filter(
                          (item) => allocationKey(item) === key,
                      ),
                quantities: [],
                signature: "",
                errors: result.errors,
                sourceInvalid: result.errors.length > 0,
                status: "error",
                skipped: 0,
            }
            byAddress.set(key, group)
        }
        group.rows.push(row)
        if (
            !group.quantities.some((quantity) =>
                equalQuantity(quantity, row.quantity, line.quantity_scale),
            )
        )
            group.quantities.push(row.quantity)
    }
    const groups = [...byAddress.values()]
    const changes: AddressImportChanges = {
        base: JSON.stringify(line.allocations),
        additions: [],
        updates: [],
    }
    for (const group of groups) {
        group.signature = JSON.stringify([group.rows, group.existing])
        resolveGroup(
            group,
            choices[group.id],
            line.quantity_scale,
            inquiry,
            line.line_id,
        )
        if (group.errors.length) group.status = "error"
        if (group.status === "add")
            changes.additions.push({
                ...group.allocation,
                quantity: group.resultQuantity!,
            })
        if (group.status === "update")
            changes.updates.push({
                allocation_id: group.existing[0].allocation_id,
                quantity: group.resultQuantity!,
            })
    }
    const quantities = new Map(
        changes.updates.map((item) => [item.allocation_id, item.quantity]),
    )
    const allocations = [
        ...line.allocations.map((item) =>
            quantities.has(item.allocation_id)
                ? { ...item, quantity: quantities.get(item.allocation_id)! }
                : item,
        ),
        ...changes.additions,
    ]
    return {
        groups,
        changes,
        allocations,
        skipped: groups.reduce((total, group) => total + group.skipped, 0),
        conflicts: groups.filter((group) => group.status === "conflict").length,
        hasErrors: groups.some((group) => group.errors.length > 0),
        overLimit: allocations.length > 100,
        changed: changes.additions.length + changes.updates.length,
    }
}
