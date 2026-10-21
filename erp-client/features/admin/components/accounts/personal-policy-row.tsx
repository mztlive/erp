import { SlidersHorizontalIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { TableCell, TableRow } from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import {
    AUTHORIZATION_POLICY_LABEL,
    personEffectiveDescription,
    type PersonScopeBusiness,
    type PersonScopeList,
} from "../../api/person-data-scopes"

const summaryClassName =
    "cursor-pointer rounded-sm leading-6 focus-visible:outline-2 focus-visible:outline-ring"

export function PersonalPolicyRow({
    business,
    data,
    labels,
    showSettings,
    canEdit,
    editing,
    onEdit,
}: {
    business: PersonScopeBusiness
    data: PersonScopeList
    labels: Map<string, string>
    showSettings: boolean
    canEdit: boolean
    editing: string | null
    onEdit: (resource: string) => void
}) {
    const actions = business.configurable_actions
    const scopes = actions.map((action) =>
        data.items.find(
            (item) =>
                item.resource === business.resource && item.action === action,
        ),
    )
    const mixed =
        new Set(scopes.map((scope) => JSON.stringify(scope?.expression))).size >
        1
    const otherActions = business.actions.filter(
        (action) => !actions.includes(action),
    )
    const segment = toAutomationIdSegment(business.resource)
    const actionLabels = business.actions.map(actionLabel)
    const describe = (index: number) => {
        const description = personEffectiveDescription(
            scopes[index],
            business,
            labels,
        )
        if (description === "本人负责的数据") return "本人负责"
        if (description === "无数据访问范围，请添加授权")
            return business.authorization_policy === "directory"
                ? "未配置可见范围"
                : "未配置数据范围"
        return description
    }
    return (
        <TableRow
            data-state={editing === business.resource ? "selected" : undefined}
        >
            <TableCell className="h-14 whitespace-normal font-medium">
                {resourceLabel(business.resource)}
            </TableCell>
            <TableCell className="h-14 space-y-1 whitespace-normal py-3 text-sm leading-6">
                {!actions.length ? (
                    <>
                        <p>
                            {
                                AUTHORIZATION_POLICY_LABEL[
                                    business.authorization_policy
                                ]
                            }
                        </p>
                        <p className="text-xs leading-5 text-muted-foreground">
                            {business.policy_description}
                        </p>
                    </>
                ) : (
                    <>
                        <details>
                            <summary
                                id={`person-scope-${segment}-details`}
                                className={summaryClassName}
                            >
                                {mixed ? "各操作范围不同 · 查看" : describe(0)}
                            </summary>
                            <div className="mt-2 space-y-2 border-l border-grid pl-3 text-xs leading-5">
                                {mixed && (
                                    <ul className="space-y-1">
                                        {actions.map((action, index) => (
                                            <li key={action}>
                                                {actionLabel(action)}：
                                                {describe(index)}
                                            </li>
                                        ))}
                                    </ul>
                                )}
                                <p className="text-muted-foreground">
                                    {business.policy_description}
                                </p>
                            </div>
                        </details>
                        {!!otherActions.length && (
                            <p className="text-xs leading-5 text-muted-foreground">
                                {otherActions.map(actionLabel).join("、")}
                                ：按业务规则校验
                            </p>
                        )}
                    </>
                )}
            </TableCell>
            <TableCell className="h-14 whitespace-normal py-3 text-xs leading-5">
                {actionLabels.length > 4 ? (
                    <details>
                        <summary
                            id={`person-scope-${segment}-actions`}
                            className={summaryClassName}
                        >
                            {actionLabels.slice(0, 3).join("、")}{" "}
                            <span className="text-muted-foreground">
                                等 {actionLabels.length} 项
                            </span>
                        </summary>
                        <p className="mt-2 leading-6">
                            {actionLabels.join("、")}
                        </p>
                    </details>
                ) : (
                    actionLabels.join("、")
                )}
            </TableCell>
            {showSettings && (
                <TableCell className="h-14 text-right">
                    {actions.length > 0 && canEdit && (
                        <Button
                            id={`person-scope-${segment}-edit`}
                            variant="outline"
                            size="sm"
                            aria-label={`配置${resourceLabel(business.resource)}范围`}
                            disabled={Boolean(editing)}
                            onClick={() => onEdit(business.resource)}
                        >
                            <SlidersHorizontalIcon
                                data-icon="inline-start"
                                aria-hidden
                            />
                            配置范围
                        </Button>
                    )}
                </TableCell>
            )}
        </TableRow>
    )
}
