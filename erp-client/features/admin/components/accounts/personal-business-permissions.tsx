"use client"
import * as React from "react"
import { SlidersHorizontalIcon } from "lucide-react"
import { BusinessFailureState } from "@/components/business"
import { Input } from "@/components/ui/input"
import {
    Table,
    TableHeader,
    TableBody,
    TableHead,
    TableRow,
    TableCell,
} from "@/components/ui/table"
import { Button } from "@/components/ui/button"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { OrgUnit } from "@/features/organization/types"
import { usePersonScopes } from "../../hooks/use-person-data-scopes"
import type { PersonalGrantDraft } from "../../hooks/use-personal-grant-draft"
import { personScopeDescription } from "../../api/person-data-scopes"
import { PersonalGrantForm } from "./personal-grant-form"

export function PersonalBusinessPermissions({
    userId,
    name,
    units,
    ready,
    canRead,
    canCreate,
    draft,
    onDraftChange,
}: {
    userId: string
    name: string
    units: OrgUnit[]
    ready: boolean
    canRead: boolean
    canCreate: boolean
    draft: PersonalGrantDraft | null
    onDraftChange: (draft: PersonalGrantDraft | null) => void
}) {
    const query = usePersonScopes(userId, canRead)
    const [search, setSearch] = React.useState("")
    const [editing, setEditing] = React.useState<string | null>(
        draft?.values.resource ?? null,
    )
    const [notice, setNotice] = React.useState<string | null>(null)
    const previous = React.useRef(draft)
    React.useEffect(() => {
        if (previous.current && !draft) setEditing(null)
        previous.current = draft
    }, [draft])
    const labels = new Map(units.map((u) => [u.id, u.name]))
    const unavailableDraft =
        editing &&
        (!canRead ||
            !canCreate ||
            !ready ||
            (query.isSuccess &&
                !query.data.businesses.some(
                    (business) => business.resource === editing,
                )))

    const businesses =
        query.data?.businesses.filter(
            (business) =>
                business.resource === editing ||
                resourceLabel(business.resource).includes(search.trim()),
        ) ?? []
    return (
        <section className="min-w-0 space-y-3 text-sm">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="space-y-1">
                    <h2 className="font-semibold">
                        业务数据范围
                        {query.isSuccess && (
                            <span className="ml-2 text-xs font-normal text-muted-foreground">
                                {query.data.businesses.length} 项业务
                            </span>
                        )}
                    </h2>
                    <p className="text-xs text-muted-foreground">
                        仅列出{name}
                        通过有效角色获得的业务操作。未设置范围的操作暂不能访问数据。
                    </p>
                </div>
                <Input
                    id="person-scope-search"
                    type="search"
                    aria-label="搜索业务"
                    placeholder="搜索业务"
                    value={search}
                    onChange={(event) => setSearch(event.target.value)}
                    className="h-8 w-full sm:w-56"
                />
            </div>
            {notice && (
                <p role="status" className="rounded-md bg-muted p-3">
                    {notice}
                </p>
            )}
            {unavailableDraft && (
                <div role="status" className="space-y-2 rounded-md border p-3">
                    <p>
                        {resourceLabel(editing)}
                        的草稿已保留，当前权限或部门信息不足，暂不能继续编辑。恢复后可继续核对，也可以放弃草稿。
                    </p>
                    <Button
                        id="person-scope-unavailable-discard"
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => {
                            if (
                                window.confirm("确定放弃当前未保存的数据范围？")
                            ) {
                                onDraftChange(null)
                                setEditing(null)
                            }
                        }}
                    >
                        放弃草稿
                    </Button>
                </div>
            )}
            {!canRead ? (
                <p>没有查看数据范围的权限，请联系管理员。</p>
            ) : query.isError ? (
                <BusinessFailureState
                    id="person-scopes-retry"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : !query.data ? (
                <p role="status">正在读取数据范围…</p>
            ) : !query.data.businesses.length ? (
                <p className="text-muted-foreground">
                    尚无可配置的业务操作，请先为此人分配有效角色。
                </p>
            ) : (
                <>
                    {!canCreate && (
                        <p className="text-xs text-muted-foreground">
                            当前只能查看范围，修改需要人员范围配置权限。
                        </p>
                    )}
                    {canCreate && !ready && (
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            部门信息尚未就绪，读取成功后即可设置范围。
                        </p>
                    )}
                    {editing && (
                        <p className="text-xs text-muted-foreground">
                            正在设置{resourceLabel(editing)}
                            ，请先保存或取消，再调整其他业务。
                        </p>
                    )}
                    <Table className="min-w-[640px] table-fixed">
                        <TableHeader>
                            <TableRow>
                                <TableHead className="w-[16%]">业务</TableHead>
                                <TableHead className="w-[36%]">
                                    已有操作权限
                                </TableHead>
                                <TableHead>当前数据范围</TableHead>
                                <TableHead className="w-32 text-right">
                                    设置
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {!businesses.length && (
                                <TableRow>
                                    <TableCell
                                        colSpan={4}
                                        className="py-8 text-center text-muted-foreground"
                                    >
                                        没有匹配的业务，请修改搜索内容。
                                    </TableCell>
                                </TableRow>
                            )}
                            {businesses.map((business) => {
                                const data = query.data!
                                const scopes = business.actions.map((action) =>
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
                                            JSON.stringify(scope?.expression),
                                        ),
                                    ).size > 1
                                const saved = scopes.some(Boolean)
                                const segment = toAutomationIdSegment(
                                    business.resource,
                                )
                                return (
                                    <React.Fragment key={business.resource}>
                                        <TableRow>
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
                                            <TableCell className="whitespace-normal">
                                                {mixed ? (
                                                    <details className="text-xs leading-6">
                                                        <summary
                                                            id={`person-scope-${segment}-details`}
                                                            className="cursor-pointer font-medium"
                                                        >
                                                            按操作分别设置 ·
                                                            展开查看
                                                        </summary>
                                                        <ul className="mt-2 space-y-1 text-muted-foreground">
                                                            {business.actions.map(
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
                                                                        {personScopeDescription(
                                                                            scopes[
                                                                                index
                                                                            ],
                                                                            labels,
                                                                        )}
                                                                    </li>
                                                                ),
                                                            )}
                                                        </ul>
                                                    </details>
                                                ) : (
                                                    <p
                                                        className={`text-xs leading-6 ${saved ? "" : "text-amber-700"}`}
                                                    >
                                                        {personScopeDescription(
                                                            scopes[0],
                                                            labels,
                                                        )}
                                                    </p>
                                                )}
                                            </TableCell>
                                            <TableCell className="text-right">
                                                {canCreate && (
                                                    <Button
                                                        id={`person-scope-${segment}-edit`}
                                                        variant="ghost"
                                                        size="sm"
                                                        className="h-7"
                                                        disabled={
                                                            Boolean(editing) ||
                                                            !ready
                                                        }
                                                        onClick={() => {
                                                            setNotice(null)
                                                            setEditing(
                                                                business.resource,
                                                            )
                                                        }}
                                                    >
                                                        <SlidersHorizontalIcon data-icon="inline-start" />
                                                        {saved
                                                            ? "修改范围"
                                                            : "设置范围"}
                                                    </Button>
                                                )}
                                            </TableCell>
                                        </TableRow>
                                    </React.Fragment>
                                )
                            })}
                        </TableBody>
                    </Table>
                    {editing && ready && canCreate && !unavailableDraft && (
                        <PersonalGrantForm
                            key={editing}
                            userId={userId}
                            resource={editing}
                            name={name}
                            data={query.data}
                            units={units}
                            draft={draft}
                            onDraftChange={onDraftChange}
                            onDone={() => {
                                onDraftChange(null)
                                setEditing(null)
                            }}
                            onSaved={() =>
                                setNotice(
                                    `${name}的${resourceLabel(editing)}数据范围已保存。`,
                                )
                            }
                            onReload={() => void query.refetch()}
                        />
                    )}
                </>
            )}
        </section>
    )
}
