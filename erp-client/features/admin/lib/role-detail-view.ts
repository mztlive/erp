import { ROLE_WORKBENCHES, type RoleWorkbenchId } from "@/lib/role-workbenches"
import { hasPermission } from "@/lib/permissions"
import {
    isSystemGroup,
    PERMISSION_BY_CODE,
    PERMISSION_MATRIX,
    permissionLabel,
    type PermissionPanelTab,
} from "./permission-catalog"
import { permissionArea } from "./permission-editor"
import type { AdminAccount } from "../types"

/** 通配全权编码：不在权限目录内，界面不可勾选，保存时原样保留。 */
export const WILDCARD_CODE = "*:*"

const FULFILLMENT_ENTRY_ANY_OF = [
    "purchase_receipt:list",
    "delivery:list",
    "electronic_delivery:list",
    "service_fulfillment:list",
] as const

/**
 * 各岗位工作台的最低入口权限（与后端 `RoleWorkbench::accessible` 口径一致）。
 * 配置工作台本身不授予业务权限，仅当角色具备这些入口权限时，绑定人员才可进入对应工作台。
 */
export const WORKBENCH_REQUIRED_PERMISSIONS: Record<
    RoleWorkbenchId,
    readonly string[]
> = {
    "WB-01": ["sales_order:list", "sales_inquiry:list", "work_item:list"],
    "WB-02": ["work_item:list", ...FULFILLMENT_ENTRY_ANY_OF],
    "WB-03": [
        "purchase_order:list",
        "procurement_inquiry:list",
        "work_item:list",
    ],
    "WB-04": [
        "customer_receipt:list",
        "supplier_payment:list",
        "work_item:list",
    ],
    "WB-05": ["cost_entry:list", "sales_order:list", "work_item:list"],
}

export type WorkbenchRequirementItem = {
    code: string
    label: string
    granted: boolean
}

export type WorkbenchReadiness = {
    id: RoleWorkbenchId
    ready: boolean
    requirements: readonly WorkbenchRequirementItem[]
    missingCodes: readonly string[]
}

/** 评估当前已勾选权限是否满足指定工作台的入口权限要求。 */
export function evaluateWorkbenchReadiness(
    workbenchId: RoleWorkbenchId,
    permissions: readonly string[],
): WorkbenchReadiness {
    if (workbenchId === "WB-02") {
        const hasWorkItem = hasPermission(permissions, "work_item:list")
        const hasAnyFulfillment = FULFILLMENT_ENTRY_ANY_OF.some((code) =>
            hasPermission(permissions, code),
        )
        const requirements: WorkbenchRequirementItem[] = [
            {
                code: "work_item:list",
                label: permissionLabel("work_item:list"),
                granted: hasWorkItem,
            },
            ...FULFILLMENT_ENTRY_ANY_OF.map((code) => ({
                code,
                label: `${permissionLabel(code)}（任一）`,
                granted: hasPermission(permissions, code),
            })),
        ]
        const missingCodes: string[] = []
        if (!hasWorkItem) missingCodes.push("work_item:list")
        if (!hasAnyFulfillment) {
            missingCodes.push("purchase_receipt:list", "delivery:list")
        }
        return {
            id: workbenchId,
            ready: hasWorkItem && hasAnyFulfillment,
            requirements,
            missingCodes,
        }
    }
    const required = WORKBENCH_REQUIRED_PERMISSIONS[workbenchId] ?? []
    const requirements: WorkbenchRequirementItem[] = required.map((code) => ({
        code,
        label: permissionLabel(code),
        granted: hasPermission(permissions, code),
    }))
    const missingCodes = requirements
        .filter((item) => !item.granted)
        .map((item) => item.code)
    return {
        id: workbenchId,
        ready: missingCodes.length === 0,
        requirements,
        missingCodes,
    }
}

export type CoveredPermissionGroup = {
    name: string
    tab: PermissionPanelTab
    area: string
    selected: number
    total: number
    dangerousSelected: number
}

export type RolePermissionOverview = {
    hasWildcard: boolean
    effectiveCodes: readonly string[]
    effectiveCount: number
    totalCatalogCount: number
    dangerousCodes: readonly string[]
    dangerousCount: number
    preservedCodes: readonly string[]
    coveredGroups: readonly CoveredPermissionGroup[]
    businessSelectedCount: number
    systemSelectedCount: number
    changedGroupNames: ReadonlySet<string>
}

const ALL_KNOWN_CODES = [...PERMISSION_BY_CODE.keys()]

/** 汇总角色当前权限覆盖态势、高风险项与各模块变更标记。 */
export function summarizeRolePermissions(
    selected: readonly string[],
    initial: readonly string[],
): RolePermissionOverview {
    const hasWildcard = selected.includes(WILDCARD_CODE)
    const effectiveCodes = hasWildcard
        ? ALL_KNOWN_CODES
        : ALL_KNOWN_CODES.filter((code) => hasPermission(selected, code))
    const effectiveInitial = initial.includes(WILDCARD_CODE)
        ? ALL_KNOWN_CODES
        : ALL_KNOWN_CODES.filter((code) => hasPermission(initial, code))

    const effectiveSet = new Set(effectiveCodes)
    const initialSet = new Set(effectiveInitial)

    const dangerousCodes = hasWildcard
        ? []
        : effectiveCodes.filter(
              (code) => PERMISSION_BY_CODE.get(code)?.dangerous === true,
          )
    const preservedCodes = selected.filter(
        (code) => !PERMISSION_BY_CODE.has(code),
    )

    const coveredGroups: CoveredPermissionGroup[] = []
    const changedGroupNames = new Set<string>()
    let businessSelectedCount = 0
    let systemSelectedCount = 0

    for (const group of PERMISSION_MATRIX) {
        let groupSelected = 0
        let groupDangerous = 0
        let groupChanged = false

        for (const code of group.codes) {
            const isSelected = effectiveSet.has(code)
            if (isSelected) {
                groupSelected += 1
                if (PERMISSION_BY_CODE.get(code)?.dangerous) {
                    groupDangerous += 1
                }
            }
            if (isSelected !== initialSet.has(code)) {
                groupChanged = true
            }
        }

        if (groupChanged) {
            changedGroupNames.add(group.name)
        }
        if (groupSelected > 0) {
            if (isSystemGroup(group.name)) {
                systemSelectedCount += groupSelected
            } else {
                businessSelectedCount += groupSelected
            }
            coveredGroups.push({
                name: group.name,
                tab: group.tab,
                area: permissionArea(group.name),
                selected: groupSelected,
                total: group.codes.length,
                dangerousSelected: groupDangerous,
            })
        }
    }

    coveredGroups.sort(
        (left, right) =>
            right.selected - left.selected ||
            left.name.localeCompare(right.name, "zh-CN"),
    )

    return {
        hasWildcard,
        effectiveCodes,
        effectiveCount: effectiveCodes.length,
        totalCatalogCount: ALL_KNOWN_CODES.length,
        dangerousCodes,
        dangerousCount: dangerousCodes.length,
        preservedCodes,
        coveredGroups,
        businessSelectedCount,
        systemSelectedCount,
        changedGroupNames,
    }
}

export type WorkbenchDiff = {
    added: readonly RoleWorkbenchId[]
    removed: readonly RoleWorkbenchId[]
    count: number
}

/** 比对岗位工作台增减变更，供保存计数与变更核对弹窗复用。 */
export function diffWorkbenches(
    current: readonly string[],
    saved: readonly string[],
): WorkbenchDiff {
    const currentSet = new Set(current)
    const savedSet = new Set(saved)
    const knownIds = ROLE_WORKBENCHES.map((item) => item.id)
    const added = knownIds.filter(
        (id) => currentSet.has(id) && !savedSet.has(id),
    )
    const removed = knownIds.filter(
        (id) => savedSet.has(id) && !currentSet.has(id),
    )
    return {
        added,
        removed,
        count: added.length + removed.length,
    }
}

export type BoundAccountsSummary = {
    members: readonly AdminAccount[]
    activeCount: number
    inactiveCount: number
}

/** 筛选并按启用优先、创建时间倒序排列绑定该角色的账号列表。 */
export function summarizeBoundAccounts(
    accounts: readonly AdminAccount[],
    roleId: string,
): BoundAccountsSummary {
    const members = accounts
        .filter((account) => account.role_ids.includes(roleId))
        .sort((left, right) => {
            const leftActive = left.status === "active" ? 0 : 1
            const rightActive = right.status === "active" ? 0 : 1
            return (
                leftActive - rightActive || right.created_at - left.created_at
            )
        })
    const activeCount = members.filter(
        (account) => account.status === "active",
    ).length
    return {
        members,
        activeCount,
        inactiveCount: members.length - activeCount,
    }
}
