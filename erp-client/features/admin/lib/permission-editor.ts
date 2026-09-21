import {
    PERMISSION_BY_CODE,
    type PermissionMatrixGroup,
} from "./permission-catalog"

export type PermissionView = "all" | "selected" | "changed" | "dangerous"

export const PERMISSION_VIEWS: Record<PermissionView, string> = {
    all: "全部权限",
    selected: "已勾选",
    changed: "本次变更",
    dangerous: "高风险权限",
}

const AREAS = [
    {
        name: "销售",
        groups: ["客户", "合同", "销售单", "销售复核", "销售选品", "开票申请"],
    },
    {
        name: "采购与供应商",
        groups: [
            "采购单",
            "供应商",
            "供应商供给",
            "采购责任管理",
            "API 供应商连接",
            "供应商订单",
            "供应商结算",
        ],
    },
    { name: "仓储与履约", groups: ["库存", "履约", "退货退款"] },
    {
        name: "财务与分析",
        groups: [
            "客户往来",
            "供应商往来",
            "财务责任管理",
            "客户经营质量",
            "实际经营盈亏",
        ],
    },
    { name: "基础资料", groups: ["公司主体", "主体", "商品与仓库"] },
    { name: "审批与治理", groups: ["审批实例", "审批流程", "导入与期初"] },
] as const

export function permissionArea(name: string): string {
    return (
        AREAS.find((area) => area.groups.some((group) => group === name))
            ?.name ?? "系统管理"
    )
}

export function orderPermissionGroups(
    groups: readonly PermissionMatrixGroup[],
) {
    const order: readonly string[] = AREAS.flatMap((area) => [...area.groups])
    return [...groups].sort((a, b) => {
        const aIndex = order.indexOf(a.name)
        const bIndex = order.indexOf(b.name)
        return (
            (aIndex < 0 ? order.length : aIndex) -
            (bIndex < 0 ? order.length : bIndex)
        )
    })
}

export function diffPermissions(
    selected: readonly string[],
    initial: readonly string[],
) {
    const before = new Set(initial)
    const after = new Set(selected)
    return {
        added: [...after].filter((code) => !before.has(code)),
        removed: [...before].filter((code) => !after.has(code)),
    }
}

/** 只改变可见项；被搜索或视图隐藏的权限仍由表单持有。 */
export function filterPermissionView(
    groups: readonly PermissionMatrixGroup[],
    view: PermissionView,
    selected: readonly string[],
    initial: readonly string[],
): readonly PermissionMatrixGroup[] {
    if (view === "all") return groups
    const selectedSet = new Set(selected)
    const initialSet = new Set(initial)
    const matches = (code: string) => {
        if (view === "selected") return selectedSet.has(code)
        if (view === "dangerous")
            return PERMISSION_BY_CODE.get(code)?.dangerous === true
        return selectedSet.has(code) !== initialSet.has(code)
    }
    return groups
        .map((group) => ({
            ...group,
            codes: group.codes.filter(matches),
            rows: group.rows
                .map((row) => ({
                    ...row,
                    codes: row.codes.filter(matches),
                    cells: row.cells.map((cell) =>
                        cell && matches(cell.code) ? cell : null,
                    ),
                }))
                .filter((row) => row.codes.length > 0),
        }))
        .filter((group) => group.codes.length > 0)
}
