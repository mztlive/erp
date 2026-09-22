import { resourceLabel } from "./permission-catalog"

const SCOPE_TYPE_ORDER = [
    "company",
    "organization",
    "team",
    "self_owned",
    "collaborative",
] as const

const SCOPE_TYPE_EXPLAIN: Record<string, string> = {
    company: "可查看公司范围内的单据，不按个人责任收窄。",
    organization: "可查看指定组织范围内的单据。",
    team: "可查看所属团队范围内的单据。",
    self_owned: "可查看自己作为主责的单据。",
    collaborative: "可查看自己作为协作人的单据。",
}

/** 预览里的一条数据范围，只保留分组需要的字段。 */
export type DataScopeGroupSource = {
    scopeType?: string
    targetLabel: string
    resource?: string
    scopeTargets?: readonly string[]
    sourceLabel?: string
}

export type DataScopePreviewGroup = {
    scopeType: string
    label: string
    explanation: string
    resources: readonly string[]
    specifiedTargetCount: number
    sources: readonly string[]
}

function unique(values: readonly string[]): string[] {
    return [...new Set(values.filter(Boolean))]
}

/**
 * 把逐条数据范围配置按类型归并。
 * 展示用，不把多条配置合并成最终鉴权结果。
 */
export function groupDataScopes(
    scopes: readonly DataScopeGroupSource[],
): DataScopePreviewGroup[] {
    const buckets = new Map<
        string,
        {
            label: string
            resources: string[]
            specifiedTargetCount: number
            sources: string[]
        }
    >()
    for (const scope of scopes) {
        const scopeType = scope.scopeType || scope.targetLabel
        const bucket = buckets.get(scopeType) ?? {
            label: scope.targetLabel,
            resources: [],
            specifiedTargetCount: 0,
            sources: [],
        }
        if (scope.resource) bucket.resources.push(scope.resource)
        bucket.specifiedTargetCount += scope.scopeTargets?.length ?? 0
        if (scope.sourceLabel) bucket.sources.push(scope.sourceLabel)
        buckets.set(scopeType, bucket)
    }
    const rankOf = (type: string) => {
        const index = (SCOPE_TYPE_ORDER as readonly string[]).indexOf(type)
        return index < 0 ? Number.MAX_SAFE_INTEGER : index
    }
    return [...buckets.entries()]
        .map(([scopeType, bucket]) => ({
            scopeType,
            label: bucket.label,
            explanation:
                SCOPE_TYPE_EXPLAIN[scopeType] ?? "按配置限制可查看的单据。",
            resources: unique(
                bucket.resources.map((code) => resourceLabel(code)),
            ).sort((a, b) => a.localeCompare(b, "zh-CN")),
            specifiedTargetCount: bucket.specifiedTargetCount,
            sources: unique(bucket.sources),
        }))
        .sort((a, b) => rankOf(a.scopeType) - rankOf(b.scopeType))
}

export function formatResourceList(resources: readonly string[]): string {
    if (resources.length === 0) return ""
    if (resources.length <= 8) return resources.join("、")
    return `${resources.slice(0, 6).join("、")} 等 ${resources.length} 类对象`
}
