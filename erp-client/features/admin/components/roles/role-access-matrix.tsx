"use client"

import * as React from "react"
import { ChevronRightIcon, FileTextIcon, SearchIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { BusinessFailureState } from "@/components/business"
import { Checkbox } from "@/components/ui/checkbox"
import {
    actionLabel,
    resourceLabel,
    PERMISSION_BY_CODE,
} from "@/lib/permission-catalog"
import { hasPermission } from "@/lib/permissions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    useDataScopesQuery,
    useOrganizationStateQuery,
} from "@/features/organization/hooks/queries"
import {
    asScopeRule,
    scopeDescription,
} from "@/features/organization/lib/scope-description"
import {
    meaningfulScope,
    selectResourceActions,
    workbenchResources,
} from "../../lib/role-workbench"
import { RoleScopeEditor } from "./role-scope-editor"
import { RoleChangePreview } from "./role-change-preview"

const resources = workbenchResources()

export function RoleAccessMatrix({
    role,
    permissions,
    savedPermissions,
    onChange,
    disabled,
    onOpenAdvanced,
}: {
    role: { id: string; name: string } | null
    permissions: readonly string[]
    savedPermissions: readonly string[]
    onChange: (permissions: string[]) => void
    disabled: boolean
    onOpenAdvanced: () => void
}) {
    const [resource, setResource] = React.useState("sales_order")
    const [keyword, setKeyword] = React.useState("")
    const [allBusinesses, setAllBusinesses] = React.useState(false)
    const profile = useAccountProfileQuery()
    const canRead = hasPermission(profile.data?.permissions, "data_scope:list")
    const scopes = useDataScopesQuery(
        { subjectType: "role", subjectId: role?.id, scopeType: "all" },
        Boolean(role) && canRead,
    )
    const org = useOrganizationStateQuery(
        hasPermission(profile.data?.permissions, "org_unit:list"),
    )
    const entry = resources.find((item) => item.resource === resource)!
    const rows =
        scopes.data?.items.filter((row) => row.resource === resource) ?? []
    const selectedActions = entry.actions.filter((action) =>
        hasPermission(permissions, `${resource}:${action}`),
    )
    const effective = rows.filter(
        (row) =>
            meaningfulScope(row) &&
            selectedActions.some((action) => row.actions.includes(action)),
    )
    const descriptions = [
        ...new Set(
            effective.map((row) =>
                scopeDescription(asScopeRule(row), org.data?.units),
            ),
        ),
    ]
    const scopeSignatures = selectedActions.map((action) =>
        JSON.stringify(
            effective
                .filter((row) => row.actions.includes(action))
                .map((row) => [
                    row.targetDimension,
                    scopeDescription(asScopeRule(row), org.data?.units),
                ])
                .sort(),
        ),
    )
    const mixedScopes = new Set(scopeSignatures).size > 1
    const multipleDimensions =
        new Set(effective.map((row) => row.targetDimension)).size > 1
    const missing = selectedActions.filter(
        (action) => !effective.some((row) => row.actions.includes(action)),
    )
    const summary = !role
        ? "创建岗位后配置数据范围"
        : !canRead
          ? "没有查看数据范围的权限"
          : scopes.isError
            ? "范围读取失败，请重试"
            : scopes.isPending
              ? "正在读取范围…"
              : !selectedActions.length
                ? "尚未选择需要范围的操作"
                : descriptions.length
                  ? `${mixedScopes ? "按操作分别配置：" : multipleDimensions ? "多个维度共同限制：" : ""}${descriptions.join("；")}${missing.length ? `（${missing.map(actionLabel).join("、")}的范围待配置）` : ""}`
                  : "尚未配置角色范围"
    const all = permissions.includes(`${resource}:*`)
    const inherited = entry.codes.some((code) =>
        permissions.some(
            (grant) => grant.startsWith("*:") && hasPermission([grant], code),
        ),
    )
    const shown = resources.filter((item, index) =>
        !keyword.trim()
            ? allBusinesses || index < 6 || item.resource === resource
            : resourceLabel(item.resource).includes(keyword.trim()),
    )
    return (
        <section
            className="grid min-h-0 flex-1 grid-cols-1 overflow-y-auto md:grid-cols-[11rem_minmax(0,1fr)] xl:grid-cols-[15rem_minmax(0,1fr)_21rem]"
            aria-label="岗位权限工作台"
        >
            <nav
                className="min-w-0 border-b py-5 md:border-b-0 md:border-r md:pr-4"
                aria-label="选择业务"
            >
                <h2 className="mb-4 text-base font-semibold">选择业务</h2>
                <div className="relative mb-4">
                    <SearchIcon className="pointer-events-none absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
                    <Input
                        id="role-access-search"
                        className="pl-8"
                        aria-label="搜索业务"
                        placeholder="搜索业务"
                        value={keyword}
                        onChange={(event) => setKeyword(event.target.value)}
                    />
                </div>
                <div className="flex max-h-56 flex-col gap-1 overflow-y-auto md:max-h-none">
                    {shown.map((item) => (
                        <button
                            id={`role-access-resource-${toAutomationIdSegment(item.resource)}`}
                            type="button"
                            key={item.resource}
                            aria-current={
                                resource === item.resource ? "true" : undefined
                            }
                            onClick={() => setResource(item.resource)}
                            className={cn(
                                "flex min-h-11 items-center gap-2 rounded-md px-3 py-2 text-left text-base hover:bg-muted/50 focus-visible:outline-2 focus-visible:outline-ring",
                                resource === item.resource &&
                                    "bg-muted font-medium",
                            )}
                        >
                            <FileTextIcon className="size-4 shrink-0" />
                            <span>{resourceLabel(item.resource)}</span>
                        </button>
                    ))}
                </div>
                {!shown.length && (
                    <p className="py-4 text-sm text-muted-foreground">
                        没有匹配的业务
                    </p>
                )}
                {!keyword && (
                    <Button
                        id="role-access-more-businesses"
                        type="button"
                        variant="ghost"
                        className="mt-2 w-full justify-between text-muted-foreground"
                        onClick={() => setAllBusinesses(!allBusinesses)}
                        aria-expanded={allBusinesses}
                    >
                        {allBusinesses ? "收起其他业务" : "其他业务"}
                        <ChevronRightIcon className="size-4" />
                    </Button>
                )}
            </nav>
            <div className="min-w-0 space-y-6 py-6 md:px-6 xl:px-7">
                <h2 className="text-xl font-semibold">
                    {resourceLabel(resource)}
                </h2>
                <section>
                    <h3 className="text-base font-semibold">允许哪些操作？</h3>
                    <div
                        className="mt-4 flex flex-wrap items-center gap-x-6 gap-y-3"
                        role="radiogroup"
                        aria-label="操作授权方式"
                    >
                        {([true, false] as const).map((mode) => (
                            <label
                                key={String(mode)}
                                className="flex items-center gap-2 text-base"
                            >
                                <input
                                    id={`role-access-mode-${mode ? "all" : "custom"}`}
                                    type="radio"
                                    name="role-access-mode"
                                    className="size-4 accent-primary"
                                    checked={all === mode}
                                    disabled={disabled || inherited}
                                    onChange={() =>
                                        onChange(
                                            mode
                                                ? [
                                                      ...permissions.filter(
                                                          (code) =>
                                                              !code.startsWith(
                                                                  `${resource}:`,
                                                              ) ||
                                                              (!PERMISSION_BY_CODE.has(
                                                                  code,
                                                              ) &&
                                                                  code !==
                                                                      `${resource}:*`),
                                                      ),
                                                      `${resource}:*`,
                                                  ]
                                                : selectResourceActions(
                                                      permissions,
                                                      resource,
                                                      entry.codes,
                                                  ),
                                        )
                                    }
                                />
                                {mode ? "全部操作" : "自选操作"}
                            </label>
                        ))}
                    </div>
                    {all && (
                        <p className="mt-3 text-sm leading-5 text-muted-foreground">
                            包含该业务当前及今后新增的操作。取消某项时会切换为逐项授权。
                        </p>
                    )}
                    {inherited && (
                        <p className="mt-3 text-sm text-amber-700">
                            此业务由跨业务授权覆盖，需在高级授权中核对。
                        </p>
                    )}
                    <div className="mt-6 grid grid-cols-1 gap-x-5 gap-y-6 sm:grid-cols-2">
                        {entry.codes.map((code) => {
                            const action = code.split(":")[1]!
                            const checked = hasPermission(permissions, code)
                            const id = `role-access-${toAutomationIdSegment(code)}`
                            return (
                                <label
                                    key={code}
                                    htmlFor={id}
                                    className="flex cursor-pointer items-start gap-3 text-base"
                                >
                                    <Checkbox
                                        id={id}
                                        className="mt-0.5 size-5 rounded-sm border-input bg-background"
                                        checked={checked}
                                        disabled={disabled || inherited}
                                        onCheckedChange={(value) => {
                                            const custom =
                                                selectResourceActions(
                                                    permissions,
                                                    resource,
                                                    entry.codes,
                                                )
                                            onChange(
                                                value
                                                    ? [
                                                          ...new Set([
                                                              ...custom,
                                                              code,
                                                          ]),
                                                      ]
                                                    : custom.filter(
                                                          (item) =>
                                                              item !== code,
                                                      ),
                                            )
                                        }}
                                    />
                                    <span>
                                        <span className="font-medium">
                                            {actionLabel(action)}
                                        </span>
                                        {PERMISSION_BY_CODE.get(code)
                                            ?.dangerous && (
                                            <span className="ml-2 rounded bg-muted px-1.5 py-0.5 text-[11px] text-muted-foreground">
                                                谨慎授权
                                            </span>
                                        )}
                                        <span className="mt-1 block text-sm leading-5 text-muted-foreground">
                                            {
                                                PERMISSION_BY_CODE.get(code)
                                                    ?.description
                                            }
                                        </span>
                                    </span>
                                </label>
                            )
                        })}
                    </div>
                </section>
                {scopes.isError && canRead && role && (
                    <BusinessFailureState
                        id="role-access-retry"
                        error={scopes.error}
                        onRetry={() => void scopes.refetch()}
                    />
                )}
                <RoleScopeEditor
                    key={resource}
                    role={role}
                    resource={resource}
                    actions={entry.actions}
                    rows={rows}
                    units={org.data?.units ?? []}
                    permissions={permissions}
                    savedPermissions={savedPermissions}
                    disabled={disabled}
                    summary={summary}
                    ready={canRead && scopes.isSuccess}
                />
                <Button
                    id="role-editor-advanced"
                    type="button"
                    variant="outline"
                    className="h-12 w-full justify-start font-normal"
                    onClick={onOpenAdvanced}
                >
                    <ChevronRightIcon className="mr-2 size-4" />
                    高级授权与来源
                </Button>
            </div>
            <div className="min-w-0 md:col-span-2 xl:col-span-1">
                <RoleChangePreview
                    roleId={role?.id}
                    permissions={permissions}
                    savedPermissions={savedPermissions}
                    org={org.data}
                    scopeSummary={summary}
                />
            </div>
        </section>
    )
}
