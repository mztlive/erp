import type { StatusTone } from "@/components/ui/status-badge"
import { isRelationActive, unitLabel } from "@/features/organization/lib/tree"
import type { OrgMembership, OrgUnit } from "@/features/organization/types"
import {
    personHasDefaultSelf,
    type PersonScopeList,
} from "../api/person-data-scopes"
import {
    personalPolicyView,
    type PersonalPolicyView,
} from "../components/accounts/personal-policy-view"

/** 提取用于圆形/方形头像缩写的首字符。 */
export function accountInitial(name: string, account: string): string {
    const source = name.trim() || account.trim()
    return Array.from(source)[0]?.toUpperCase() ?? "?"
}

/** 向上回溯组织层级链，带环路保护，按根到叶返回。 */
export function orgUnitPath(
    units: readonly OrgUnit[],
    unitId: string,
): OrgUnit[] {
    const byId = new Map(units.map((unit) => [unit.id, unit]))
    const chain: OrgUnit[] = []
    const visited = new Set<string>()
    let current = byId.get(unitId)
    while (current && !visited.has(current.id)) {
        visited.add(current.id)
        chain.unshift(current)
        current = current.parent_id ? byId.get(current.parent_id) : undefined
    }
    return chain
}

/** 格式化组织完整路径（如「总部 / 销售一部」）。 */
export function orgUnitPathLabel(units: OrgUnit[], unitId: string): string {
    const chain = orgUnitPath(units, unitId)
    if (chain.length === 0) return unitLabel(units, unitId)
    return chain.map((unit) => unit.name).join(" / ")
}

/** 返回由该人员担任负责人的有效组织列表。 */
export function ledOrgUnits(
    units: readonly OrgUnit[],
    userId: string,
): OrgUnit[] {
    return units.filter(
        (unit) => unit.enabled && unit.leader?.user_id === userId,
    )
}

/** 查找该人员当前生效的组织成员关系。 */
export function activeOrgMembership(
    memberships: readonly OrgMembership[],
    userId: string,
    asOf: string,
): OrgMembership | undefined {
    return memberships.find(
        (item) =>
            item.user_id === userId &&
            isRelationActive(item.valid_from, item.valid_to, asOf),
    )
}

/** 统计某个组织节点当前生效成员人数。 */
export function activeUnitMemberCount(
    memberships: readonly OrgMembership[],
    unitId: string,
    asOf: string,
): number {
    return memberships.filter(
        (item) =>
            item.org_unit_id === unitId &&
            isRelationActive(item.valid_from, item.valid_to, asOf),
    ).length
}

export type ScopePostureKind =
    | "unconfigured"
    | "company"
    | "expanded"
    | "mixed"
    | "default_self"
    | "automatic"

export type ScopePosture = {
    kind: ScopePostureKind
    label: string
    shortLabel: string
    tone: StatusTone
    /** 概览排序优先级：待配置最优先提示，其次全公司与已扩展，最后默认本人。 */
    priority: number
}

/** 按实际配置条件归类单条业务的数据范围态势，避免仅依赖展示文案猜测。 */
export function classifyPolicyView(
    view: PersonalPolicyView,
    data: PersonScopeList,
): ScopePosture {
    const { business, groups } = view
    if (!business.configurable_actions.length) {
        return {
            kind: "automatic",
            label: "业务规则",
            shortLabel: "业务规则",
            tone: "neutral",
            priority: 50,
        }
    }

    let hasUnconfigured = false
    let hasCompany = false
    let hasExtraGrant = false
    let hasSelfBaseline = false

    for (const action of business.configurable_actions) {
        const scope = data.items.find(
            (item) =>
                item.resource === business.resource && item.action === action,
        )
        const defaultSelf = personHasDefaultSelf(business, action)
        const alternatives = scope?.expression.alternatives ?? []
        const isCompany = alternatives.some((terms) =>
            terms.some((term) => term.scope_type === "company"),
        )
        const nonDefaultAlts = alternatives.filter(
            (terms) =>
                terms.length > 0 &&
                !(
                    defaultSelf &&
                    terms.length === 1 &&
                    terms[0].scope_type === "self_owned"
                ),
        )

        if (isCompany) {
            hasCompany = true
        } else if (nonDefaultAlts.length > 0) {
            hasExtraGrant = true
        } else if (defaultSelf) {
            hasSelfBaseline = true
        } else {
            hasUnconfigured = true
        }
    }

    if (hasUnconfigured && !hasCompany && !hasExtraGrant && !hasSelfBaseline) {
        return {
            kind: "unconfigured",
            label: "待配置范围",
            shortLabel: "待配置",
            tone: "warning",
            priority: 10,
        }
    }
    if (hasUnconfigured) {
        return {
            kind: "unconfigured",
            label: "部分待配置",
            shortLabel: "部分待配置",
            tone: "warning",
            priority: 15,
        }
    }
    if (hasCompany && groups.length === 1) {
        return {
            kind: "company",
            label: "全公司范围",
            shortLabel: "全公司",
            tone: "info",
            priority: 20,
        }
    }
    if (groups.length > 1) {
        return {
            kind: "mixed",
            label: "分操作配置",
            shortLabel: "分操作",
            tone: "info",
            priority: 25,
        }
    }
    if (hasExtraGrant) {
        return {
            kind: "expanded",
            label: "已扩展范围",
            shortLabel: "已扩展",
            tone: "success",
            priority: 30,
        }
    }
    return {
        kind: "default_self",
        label: "仅本人默认",
        shortLabel: "默认本人",
        tone: "neutral",
        priority: 40,
    }
}

export type EnrichedPolicyView = PersonalPolicyView & {
    posture: ScopePosture
}

export type PersonScopesSummary = {
    all: EnrichedPolicyView[]
    configurable: EnrichedPolicyView[]
    automatic: EnrichedPolicyView[]
    unconfiguredCount: number
    expandedCount: number
    companyCount: number
    defaultOnlyCount: number
}

/** 统一汇总单人全部业务的数据范围视图与统计数字。 */
export function summarizePersonScopeViews(
    data: PersonScopeList,
    labels: Map<string, string>,
): PersonScopesSummary {
    const all: EnrichedPolicyView[] = data.businesses.map((business) => {
        const view = personalPolicyView(business, data, labels)
        return {
            ...view,
            posture: classifyPolicyView(view, data),
        }
    })
    const configurable = all.filter(
        (item) => item.business.configurable_actions.length > 0,
    )
    const automatic = all.filter(
        (item) => item.business.configurable_actions.length === 0,
    )
    const unconfiguredCount = configurable.filter(
        (item) => item.posture.kind === "unconfigured",
    ).length
    const companyCount = configurable.filter(
        (item) => item.posture.kind === "company",
    ).length
    const expandedCount = configurable.filter((item) =>
        ["company", "expanded", "mixed"].includes(item.posture.kind),
    ).length
    const defaultOnlyCount = configurable.filter(
        (item) => item.posture.kind === "default_self",
    ).length

    return {
        all,
        configurable,
        automatic,
        unconfiguredCount,
        expandedCount,
        companyCount,
        defaultOnlyCount,
    }
}
