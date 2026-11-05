import {
    compactFixed,
    compareDecimal,
    subtractFixed,
} from "@/lib/fixed-decimal"
import {
    packingSum,
    type DeliveryPacking,
    type PackingItem,
} from "./delivery-packing"

const precision = { maxScale: 6, outputScale: 6 } as const
export function normalizePackingQuantity(value: string): string {
    try {
        return compactFixed(value)
    } catch {
        return value
    }
}

export function packingQuantity(value: string): string {
    return normalizePackingQuantity(value) || "—"
}

/** 剩余量保留正负，超分不能截成零。 */
export function allocationBalance(
    packing: DeliveryPacking,
    address: string,
    line: string,
    exceptPackage?: string,
) {
    const required =
        packing.dispatch
            .find((row) => row.destination_key === address)
            ?.items.find((item) => item.delivery_line_id === line)?.quantity ??
        "0"
    const assigned = packingSum(
        packing.packages
            .filter(
                (row) =>
                    row.destination_key === address && row.id !== exceptPackage,
            )
            .flatMap((row) =>
                row.items
                    .filter((item) => item.delivery_line_id === line)
                    .map((item) => item.quantity),
            ),
    )
    return {
        required,
        assigned,
        remaining: subtractFixed(required, assigned, precision),
    }
}

export function dispatchItems(
    packing: DeliveryPacking,
    address: string,
): PackingItem[] {
    return (
        packing.dispatch.find((row) => row.destination_key === address)
            ?.items ?? []
    )
        .map((item) => {
            if (compareDecimal(item.quantity, "0", 6) < 0)
                throw new Error("本次发货数量不能为负")
            return { ...item, quantity: compactFixed(item.quantity) }
        })
        .filter((item) => compareDecimal(item.quantity, "0", 6) > 0)
}

/** 只有完整承接本次数量的单包裹自动跟随；删包后未分配的商品须明确重新分配。 */
export function automaticPackage(
    packing: DeliveryPacking,
    address: string,
): boolean {
    const packages = packing.packages.filter(
        (row) => row.destination_key === address,
    )
    if (packages.length !== 1) return false
    try {
        const expected = dispatchItems(packing, address)
        return (
            expected.length === packages[0].items.length &&
            expected.every((item) =>
                packages[0].items.some(
                    (value) =>
                        value.delivery_line_id === item.delivery_line_id &&
                        compareDecimal(value.quantity, item.quantity, 6) === 0,
                ),
            )
        )
    } catch {
        return false
    }
}

/** 上层数量变化只同步完整单包裹；多个包裹保留原分配，由界面提示差额。 */
export function changeDispatch(
    packing: DeliveryPacking,
    address: string,
    line: string,
    quantity: string,
    followSingle = automaticPackage(packing, address),
): DeliveryPacking {
    const next = {
        ...packing,
        dispatch: packing.dispatch.map((row) =>
            row.destination_key === address
                ? {
                      ...row,
                      items: row.items.map((item) =>
                          item.delivery_line_id === line
                              ? { ...item, quantity }
                              : item,
                      ),
                  }
                : row,
        ),
    }
    if (
        !followSingle ||
        packing.packages.filter((row) => row.destination_key === address)
            .length !== 1
    )
        return next
    try {
        const items = dispatchItems(next, address)
        return {
            ...next,
            packages: next.packages.map((row) =>
                row.destination_key === address ? { ...row, items } : row,
            ),
        }
    } catch {
        return next
    }
}

export function changePackageItem(
    items: PackingItem[],
    line: string,
    quantity: string,
): PackingItem[] {
    try {
        if (quantity.trim() && compareDecimal(quantity, "0", 6) === 0)
            return items.filter((item) => item.delivery_line_id !== line)
    } catch {
        /* 保留输入中的小数，统一校验提示。 */
    }
    return items.some((item) => item.delivery_line_id === line)
        ? items.map((item) =>
              item.delivery_line_id === line ? { ...item, quantity } : item,
          )
        : [...items, { delivery_line_id: line, quantity }]
}

/** 从指定包裹移出商品，原物流保留；一次性更新两个包裹，取消弹窗不改变草稿。 */
export function splitPackage(
    packing: DeliveryPacking,
    packageId: string,
    moved: PackingItem[],
    newId: string,
): DeliveryPacking {
    const source = packing.packages.find((row) => row.id === packageId)
    if (!source) throw new Error("原包裹已变化，请关闭后重试")
    if (packing.packages.length >= 100) throw new Error("最多登记100个包裹")
    const remaining = source.items
        .map((item) => {
            const quantity =
                moved.find(
                    (row) => row.delivery_line_id === item.delivery_line_id,
                )?.quantity ?? "0"
            if (
                compareDecimal(quantity, "0", 6) < 0 ||
                compareDecimal(quantity, item.quantity, 6) > 0
            )
                throw new Error("拆出数量须在0与原包裹数量之间")
            return {
                ...item,
                quantity: compactFixed(
                    subtractFixed(item.quantity, quantity, precision),
                ),
            }
        })
        .filter((item) => compareDecimal(item.quantity, "0", 6) > 0)
    if (
        new Set(moved.map((item) => item.delivery_line_id)).size !==
            moved.length ||
        moved.some(
            (item) =>
                !source.items.some(
                    (row) => row.delivery_line_id === item.delivery_line_id,
                ),
        )
    )
        throw new Error("商品不属于原包裹或重复")
    const items = moved
        .filter((item) => compareDecimal(item.quantity, "0", 6) > 0)
        .map((item) => ({ ...item, quantity: compactFixed(item.quantity) }))
    if (!items.length) throw new Error("请填写需要拆到新包裹的数量")
    if (!remaining.length)
        throw new Error("原包裹至少需保留一项商品；无需全部移到新包裹")
    const packages = [
        ...packing.packages.map((row) =>
            row.id === packageId ? { ...row, items: remaining } : row,
        ),
        {
            id: newId,
            destination_key: source.destination_key,
            carrier: "",
            tracking_no: "",
            items,
        },
    ]
    if (packages.flatMap((row) => row.items).length > 100)
        throw new Error("包裹商品关联超过100项，请减少拆分")
    return { ...packing, packages }
}
