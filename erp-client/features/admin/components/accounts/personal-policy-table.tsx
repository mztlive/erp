import { SlidersHorizontalIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import {
    AUTHORIZATION_POLICY_LABEL,
    personEffectiveDescription,
    type AuthorizationPolicy,
    type PersonScopeBusiness,
    type PersonScopeList,
} from "../../api/person-data-scopes"

const INVENTORY_RESOURCES = [
    "stock_adjustment",
    "stock_balance",
    "stock_movement",
    "stock_reservation",
]

const GROUPS: {
    inventory?: boolean
    policies: AuthorizationPolicy[]
    title: string
    description: string
}[] = [
    {
        policies: ["business"],
        title: "业务数据范围",
        description: "按业务当前负责人或归属划分访问范围，追加授权按操作合并。",
    },
    {
        inventory: true,
        policies: ["business"],
        title: "库存仓库政策",
        description:
            "库存调整、余额、流水和预占均按仓库授权；各操作保留各自的范围。此范围与仓库目录可见权限分别校验。",
    },
    {
        policies: ["governance"],
        title: "治理委派",
        description: "限定可管理的组织、人员或他人任务；操作权限仍须单独具备。",
    },
    {
        policies: ["directory"],
        title: "目录可见",
        description: "限定可查询和选择的候选，不授予候选对应的业务操作权。",
    },
    {
        policies: ["source_inherited", "task", "role"],
        title: "由业务规则确定",
        description:
            "下列业务按来源、任务指派或操作权限校验，无需追加独立数据范围。",
    },
]

export function PersonalPolicyTable({
    businesses,
    data,
    labels,
    canEdit,
    editing,
    onEdit,
}: {
    businesses: PersonScopeBusiness[]
    data: PersonScopeList
    labels: Map<string, string>
    canEdit: boolean
    editing: string | null
    onEdit: (resource: string) => void
}) {
    if (!businesses.length)
        return (
            <p className="py-6 text-center text-muted-foreground">
                没有匹配的业务，请修改搜索内容。
            </p>
        )
    return (
        <div className="space-y-6">
            {GROUPS.map((group) => {
                const rows = businesses.filter(
                    (business) =>
                        group.policies.includes(
                            business.authorization_policy,
                        ) &&
                        Boolean(group.inventory) ===
                            INVENTORY_RESOURCES.includes(business.resource),
                )
                if (!rows.length) return null
                return (
                    <section key={group.title} className="space-y-2">
                        <h3 className="font-medium">
                            {group.title}{" "}
                            <span className="ml-1 text-xs font-normal text-muted-foreground">
                                {rows.length} 项
                            </span>
                        </h3>
                        <p className="text-xs text-muted-foreground">
                            {group.description}
                        </p>
                        <Table className="min-w-[640px] table-fixed">
                            <TableHeader>
                                <TableRow>
                                    <TableHead className="w-[16%]">
                                        业务
                                    </TableHead>
                                    <TableHead className="w-[30%]">
                                        已有操作权限
                                    </TableHead>
                                    <TableHead>访问依据</TableHead>
                                    <TableHead className="w-28 text-right">
                                        设置
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {rows.map((business) => {
                                    const actions =
                                        business.configurable_actions
                                    const scopes = actions.map((action) =>
                                        data.items.find(
                                            (item) =>
                                                item.resource ===
                                                    business.resource &&
                                                item.action === action,
                                        ),
                                    )
                                    const mixed =
                                        new Set(
                                            scopes.map((scope) =>
                                                JSON.stringify(
                                                    scope?.expression,
                                                ),
                                            ),
                                        ).size > 1
                                    const otherActions =
                                        business.actions.filter(
                                            (action) =>
                                                !actions.includes(action),
                                        )
                                    const segment = toAutomationIdSegment(
                                        business.resource,
                                    )
                                    return (
                                        <TableRow key={business.resource}>
                                            <TableCell className="whitespace-normal font-medium">
                                                {resourceLabel(
                                                    business.resource,
                                                )}
                                            </TableCell>
                                            <TableCell className="whitespace-normal text-xs leading-5 text-muted-foreground">
                                                {business.actions
                                                    .map(actionLabel)
                                                    .join("、")}
                                            </TableCell>
                                            <TableCell className="space-y-1 whitespace-normal text-xs leading-6">
                                                {!actions.length ? (
                                                    <>
                                                        <p className="font-medium">
                                                            {
                                                                AUTHORIZATION_POLICY_LABEL[
                                                                    business
                                                                        .authorization_policy
                                                                ]
                                                            }
                                                        </p>
                                                        <p className="text-muted-foreground">
                                                            {
                                                                business.policy_description
                                                            }
                                                        </p>
                                                    </>
                                                ) : (
                                                    <>
                                                        {mixed ? (
                                                            <details>
                                                                <summary
                                                                    id={`person-scope-${segment}-details`}
                                                                    className="cursor-pointer font-medium"
                                                                >
                                                                    按操作合并范围
                                                                    · 展开查看
                                                                </summary>
                                                                <ul className="mt-2 space-y-1 text-muted-foreground">
                                                                    {actions.map(
                                                                        (
                                                                            action,
                                                                            index,
                                                                        ) => (
                                                                            <li
                                                                                key={
                                                                                    action
                                                                                }
                                                                            >
                                                                                {actionLabel(
                                                                                    action,
                                                                                )}
                                                                                ：
                                                                                {personEffectiveDescription(
                                                                                    scopes[
                                                                                        index
                                                                                    ],
                                                                                    business,
                                                                                    labels,
                                                                                )}
                                                                            </li>
                                                                        ),
                                                                    )}
                                                                </ul>
                                                            </details>
                                                        ) : (
                                                            <p>
                                                                {personEffectiveDescription(
                                                                    scopes[0],
                                                                    business,
                                                                    labels,
                                                                )}
                                                            </p>
                                                        )}
                                                        <p className="text-muted-foreground">
                                                            {
                                                                business.policy_description
                                                            }
                                                        </p>
                                                        {!!otherActions.length && (
                                                            <p className="text-muted-foreground">
                                                                {otherActions
                                                                    .map(
                                                                        actionLabel,
                                                                    )
                                                                    .join("、")}
                                                                ：按对应业务规则校验，不单独配置范围。
                                                            </p>
                                                        )}
                                                    </>
                                                )}
                                            </TableCell>
                                            <TableCell className="text-right">
                                                {actions.length ? (
                                                    canEdit && (
                                                        <Button
                                                            id={`person-scope-${segment}-edit`}
                                                            variant="ghost"
                                                            size="sm"
                                                            className="h-7"
                                                            disabled={Boolean(
                                                                editing,
                                                            )}
                                                            onClick={() =>
                                                                onEdit(
                                                                    business.resource,
                                                                )
                                                            }
                                                        >
                                                            <SlidersHorizontalIcon data-icon="inline-start" />
                                                            管理范围
                                                        </Button>
                                                    )
                                                ) : (
                                                    <span className="text-xs text-muted-foreground">
                                                        无需配置
                                                    </span>
                                                )}
                                            </TableCell>
                                        </TableRow>
                                    )
                                })}
                            </TableBody>
                        </Table>
                    </section>
                )
            })}
        </div>
    )
}
