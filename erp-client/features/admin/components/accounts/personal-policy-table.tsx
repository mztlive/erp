import { ChevronDownIcon } from "lucide-react"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
    Table,
    TableBody,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type {
    AuthorizationPolicy,
    PersonScopeBusiness,
    PersonScopeList,
} from "../../api/person-data-scopes"
import { PersonalPolicyRow } from "./personal-policy-row"

const INVENTORY_RESOURCES = [
    "stock_adjustment",
    "stock_balance",
    "stock_movement",
    "stock_reservation",
]

const GROUPS: {
    id: string
    inventory?: boolean
    rules?: boolean
    policies: AuthorizationPolicy[]
    title: string
    description: string
}[] = [
    {
        id: "business",
        policies: ["business"],
        title: "业务数据范围",
        description:
            "按业务负责人或归属确定范围，追加授权按操作合并；仍须具备对应操作权限。",
    },
    {
        id: "inventory",
        inventory: true,
        policies: ["business"],
        title: "库存仓库政策",
        description:
            "按仓库授权，各操作保留各自的范围；与仓库目录可见权限分别校验。",
    },
    {
        id: "governance",
        policies: ["governance"],
        title: "治理委派",
        description: "限定可管理的组织、人员或他人任务；仍须具备对应操作权限。",
    },
    {
        id: "directory",
        policies: ["directory"],
        title: "目录可见",
        description:
            "控制可查询、可选择的候选项，不授予关联业务的数据或操作权限。",
    },
    {
        id: "rules",
        rules: true,
        policies: ["source_inherited", "task", "role"],
        title: "按业务规则访问",
        description:
            "按来源业务、流程任务或操作权限校验，无需单独配置数据范围。",
    },
]

export function PersonalPolicyTable({
    businesses,
    data,
    labels,
    canEdit,
    editing,
    search,
    onEdit,
}: {
    businesses: PersonScopeBusiness[]
    data: PersonScopeList
    labels: Map<string, string>
    canEdit: boolean
    editing: string | null
    search: string
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
                const headingId = `person-scope-group-${group.id}`
                const title = (
                    <>
                        <span className="font-medium">{group.title}</span>
                        <span className="text-xs font-normal text-muted-foreground">
                            {rows.length} 项{group.rules ? " · 无需配置" : ""}
                        </span>
                    </>
                )
                const content = (
                    <div className="space-y-3">
                        <p className="text-xs leading-5 text-muted-foreground">
                            {group.description}
                        </p>
                        <Table
                            className="min-w-[640px] table-fixed"
                            aria-labelledby={headingId}
                        >
                            <TableHeader>
                                <TableRow>
                                    <TableHead className="w-[16%]">
                                        业务
                                    </TableHead>
                                    <TableHead className="w-[40%]">
                                        {group.rules ? "访问规则" : "当前范围"}
                                    </TableHead>
                                    <TableHead>已有操作权限</TableHead>
                                    {!group.rules && (
                                        <TableHead className="w-36 text-right">
                                            操作
                                        </TableHead>
                                    )}
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {rows.map((business) => (
                                    <PersonalPolicyRow
                                        key={business.resource}
                                        business={business}
                                        data={data}
                                        labels={labels}
                                        showSettings={!group.rules}
                                        canEdit={canEdit}
                                        editing={editing}
                                        onEdit={onEdit}
                                    />
                                ))}
                            </TableBody>
                        </Table>
                    </div>
                )
                return group.rules ? (
                    <Collapsible
                        key={`${group.id}-${search}`}
                        defaultOpen={Boolean(search)}
                        className="border-t border-grid pt-4"
                    >
                        <h3>
                            <CollapsibleTrigger
                                id={headingId}
                                className="group flex w-full cursor-pointer items-center gap-2 rounded-sm py-2 text-left focus-visible:outline-2 focus-visible:outline-ring"
                            >
                                {title}
                                <ChevronDownIcon
                                    aria-hidden
                                    className="ml-auto size-4 text-muted-foreground transition-transform group-aria-expanded:rotate-180"
                                />
                            </CollapsibleTrigger>
                        </h3>
                        <CollapsibleContent className="pt-1">
                            {content}
                        </CollapsibleContent>
                    </Collapsible>
                ) : (
                    <section
                        key={group.id}
                        className="space-y-1"
                        aria-labelledby={headingId}
                    >
                        <h3 id={headingId} className="flex items-center gap-2">
                            {title}
                        </h3>
                        {content}
                    </section>
                )
            })}
        </div>
    )
}
