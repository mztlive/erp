import type { DataScopePreviewGroup } from "../lib/data-scope-preview"

function ScopeResources({ group }: { group: DataScopePreviewGroup }) {
    if (group.resources.length === 0) return null
    if (group.scopeType === "company" && group.resources.length > 8) {
        return (
            <p className="text-[13px] leading-6">
                适用于已接入的业务对象（
                <strong className="num text-foreground">
                    {group.resources.length}
                </strong>{" "}
                类）。
            </p>
        )
    }
    return (
        <div className="flex flex-wrap gap-x-3 gap-y-1 text-[13px] leading-6">
            {group.resources.map((resource) => (
                <span key={resource}>{resource}</span>
            ))}
        </div>
    )
}

function showGroupSource(
    group: DataScopePreviewGroup,
    subjectLabel: string,
    alwaysShowSource: boolean,
) {
    if (group.sources.length === 0) return false
    if (alwaysShowSource) return true
    return group.sources.length > 1 || group.sources[0] !== subjectLabel
}

/** 按范围类型展示已归并的数据范围。来源默认在与当前主体相同时省略。 */
export function DataScopeGroupList({
    groups,
    subjectLabel = "",
    alwaysShowSource = false,
}: {
    groups: readonly DataScopePreviewGroup[]
    subjectLabel?: string
    alwaysShowSource?: boolean
}) {
    return (
        <div className="space-y-5">
            {groups.map((group) => (
                <div key={group.scopeType} className="space-y-1.5">
                    <p className="font-medium">{group.label}</p>
                    <p className="text-xs leading-5 text-muted-foreground">
                        {group.explanation}
                    </p>
                    <ScopeResources group={group} />
                    {group.specifiedTargetCount > 0 ? (
                        <p className="text-xs leading-5 text-muted-foreground">
                            已指定{" "}
                            <strong className="num text-foreground">
                                {group.specifiedTargetCount}
                            </strong>{" "}
                            个组织或团队
                        </p>
                    ) : null}
                    {showGroupSource(group, subjectLabel, alwaysShowSource) ? (
                        <p className="text-xs leading-5 text-muted-foreground">
                            来自{group.sources.join("、")}
                        </p>
                    ) : null}
                </div>
            ))}
        </div>
    )
}
