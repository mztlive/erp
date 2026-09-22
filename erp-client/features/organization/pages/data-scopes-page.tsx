"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { PlusIcon } from "lucide-react"

import {
    BusinessFailureState,
    OptionCombobox,
    PageScaffold,
} from "@/components/business"
import {
    ListSearchField,
    ListWorkSurface,
    ListWorkspaceFilterBar,
    ListWorkspaceHeader,
    listWorkspaceFilterStatusText,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { DataScopeFormDialog } from "@/features/organization/components/data-scope-form-dialog"
import { OrganizationEmptyByReason } from "@/features/organization/components/empty-by-reason"
import {
    useCreateDataScopeMutation,
    useDataScopesQuery,
    useDeleteDataScopeMutation,
    useOrganizationStateQuery,
} from "@/features/organization/hooks/queries"
import { organizationEmptyReason } from "@/features/organization/lib/empty-reason"
import {
    ORGANIZATION_BOUNDARY_NOTICE,
    PAGE_NARROW_CLASS,
    SCOPE_TYPE_LABEL,
} from "@/features/organization/lib/labels"
import { registeredResources } from "@/features/organization/lib/scope-payload"
import {
    mergeDataScopeSearchParams,
    parseDataScopeSearchParams,
} from "@/features/organization/lib/url-state"
import { hasPermission } from "@/lib/permissions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    personLabel,
    roleLabel,
    unitLabel,
} from "@/features/organization/lib/tree"

export function DataScopesPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const url = React.useMemo(
        () => parseDataScopeSearchParams(searchParams),
        [searchParams],
    )
    const pushUrl = React.useCallback(
        (patch: Partial<typeof url>) => {
            router.replace(
                `${pathname}${mergeDataScopeSearchParams(searchParams, {
                    ...url,
                    ...patch,
                })}`,
                { scroll: false },
            )
        },
        [pathname, router, searchParams, url],
    )
    const profileQuery = useAccountProfileQuery()
    const canCreate = hasPermission(
        profileQuery.data?.permissions,
        "data_scope:create",
    )
    const canDelete = hasPermission(
        profileQuery.data?.permissions,
        "data_scope:delete",
    )
    const scopesQuery = useDataScopesQuery(url)
    const orgQuery = useOrganizationStateQuery()
    const createMutation = useCreateDataScopeMutation()
    const deleteMutation = useDeleteDataScopeMutation()
    const [searchDraft, setSearchDraft] = React.useState(url.q ?? "")
    const [createOpen, setCreateOpen] = React.useState(false)
    const resources = React.useMemo(() => registeredResources(), [])

    React.useEffect(() => {
        setSearchDraft(url.q ?? "")
    }, [url.q])

    const people = orgQuery.data?.people ?? []
    const roles = orgQuery.data?.roles ?? []
    const units = orgQuery.data?.units ?? []
    const items = scopesQuery.data?.items ?? []
    const emptyReason = organizationEmptyReason({
        error: scopesQuery.error,
        noScope: scopesQuery.data?.emptyReason === "no_scope",
        filtered: Boolean(
            url.q ||
            url.resource ||
            url.action ||
            url.subjectType !== "all" ||
            url.subjectId ||
            url.scopeType !== "all",
        ),
        empty: items.length === 0,
        permissionPending: profileQuery.isPending,
    })
    const subjectLabel = (type: string, id: string) =>
        type === "user" ? personLabel(people, id) : roleLabel(roles, id)

    if (profileQuery.isPending || scopesQuery.isPending) {
        return (
            <PageScaffold density="compact" className={styles.page}>
                <div className="h-9 w-40 animate-pulse rounded-lg bg-muted" />
                <div className="h-[32rem] animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold
            density="compact"
            className={`${styles.page} ${PAGE_NARROW_CLASS}`}
        >
            <ListWorkspaceHeader
                eyebrow="系统"
                title="高级范围配置"
                description="跨角色核对范围与个人限制。日常授权请在「角色与权限」中选择角色后配置。"
            >
                <Button
                    id="organization-scopes-back-roles"
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => router.push("/system/access-audit")}
                >
                    返回角色与权限
                </Button>
                {canCreate ? (
                    <Button
                        id="organization-scope-create"
                        type="button"
                        size="sm"
                        onClick={() => setCreateOpen(true)}
                    >
                        <PlusIcon className="size-3.5" aria-hidden="true" />
                        新建范围
                    </Button>
                ) : null}
            </ListWorkspaceHeader>

            <Alert>
                <AlertTitle>配置边界</AlertTitle>
                <AlertDescription>
                    {ORGANIZATION_BOUNDARY_NOTICE}
                </AlertDescription>
            </Alert>

            <ListWorkSurface
                ariaLabel="数据范围列表"
                toolbar={
                    <ListWorkspaceFilterBar
                        idPrefix="organization-scope"
                        formAriaLabel="范围配置查询"
                        onSubmit={() =>
                            pushUrl({ q: searchDraft.trim() || undefined })
                        }
                        search={
                            <ListSearchField
                                id="organization-scope-search"
                                value={searchDraft}
                                onChange={setSearchDraft}
                                placeholder="按资源、动作或主体筛选"
                                aria-label="搜索范围配置"
                            />
                        }
                        primaryFilters={
                            <div className="flex min-w-0 flex-wrap gap-3">
                                <OptionCombobox
                                    id="organization-scope-filter-resource"
                                    aria-label="资源"
                                    value={url.resource ?? "all"}
                                    options={[
                                        { value: "all", label: "全部资源" },
                                        ...resources.map((item) => ({
                                            value: item.resource,
                                            label: resourceLabel(item.resource),
                                        })),
                                    ]}
                                    onValueChange={(value) =>
                                        pushUrl({
                                            resource:
                                                !value || value === "all"
                                                    ? undefined
                                                    : value,
                                            action: undefined,
                                        })
                                    }
                                    allowClear={false}
                                />
                                <OptionCombobox
                                    id="organization-scope-filter-action"
                                    aria-label="动作"
                                    value={url.action ?? "all"}
                                    options={[
                                        { value: "all", label: "全部动作" },
                                        ...(
                                            resources.find(
                                                (item) =>
                                                    item.resource ===
                                                    url.resource,
                                            )?.actions ?? []
                                        ).map((action) => ({
                                            value: action,
                                            label: actionLabel(action),
                                        })),
                                    ]}
                                    onValueChange={(value) =>
                                        pushUrl({
                                            action:
                                                !value || value === "all"
                                                    ? undefined
                                                    : value,
                                        })
                                    }
                                    allowClear={false}
                                />
                                <OptionCombobox
                                    id="organization-scope-filter-subject-type"
                                    aria-label="主体类型"
                                    value={url.subjectType}
                                    options={[
                                        { value: "all", label: "全部主体" },
                                        { value: "role", label: "角色" },
                                        { value: "user", label: "用户上限" },
                                    ]}
                                    onValueChange={(value) =>
                                        pushUrl({
                                            subjectType: (value ??
                                                "all") as typeof url.subjectType,
                                            subjectId: undefined,
                                        })
                                    }
                                    allowClear={false}
                                />
                                {url.subjectType !== "all" ? (
                                    <OptionCombobox
                                        id="organization-scope-filter-subject"
                                        aria-label="主体"
                                        value={url.subjectId ?? "all"}
                                        options={[
                                            { value: "all", label: "全部主体" },
                                            ...(url.subjectType === "user"
                                                ? people.map((person) => ({
                                                      value: person.id,
                                                      label: `${person.label}（${person.account}）`,
                                                  }))
                                                : roles.map((role) => ({
                                                      value: role.id,
                                                      label: role.name,
                                                  }))),
                                        ]}
                                        onValueChange={(value) =>
                                            pushUrl({
                                                subjectId:
                                                    !value || value === "all"
                                                        ? undefined
                                                        : value,
                                            })
                                        }
                                        allowClear={false}
                                    />
                                ) : null}
                                <OptionCombobox
                                    id="organization-scope-filter-scope-type"
                                    aria-label="范围类型"
                                    value={url.scopeType}
                                    options={[
                                        { value: "all", label: "全部范围类型" },
                                        ...Object.entries(SCOPE_TYPE_LABEL).map(
                                            ([value, label]) => ({
                                                value,
                                                label,
                                            }),
                                        ),
                                    ]}
                                    onValueChange={(value) =>
                                        pushUrl({
                                            scopeType: (value ??
                                                "all") as typeof url.scopeType,
                                        })
                                    }
                                    allowClear={false}
                                />
                            </div>
                        }
                        resultStatus={listWorkspaceFilterStatusText({
                            loading: scopesQuery.isFetching,
                            failed: scopesQuery.isError,
                            resultCount: scopesQuery.data
                                ? items.length
                                : undefined,
                            noun: "条范围",
                        })}
                        chips={[
                            url.q
                                ? { key: "q", label: `搜索：${url.q}` }
                                : null,
                            url.resource
                                ? {
                                      key: "resource",
                                      label: resourceLabel(url.resource),
                                  }
                                : null,
                            url.action
                                ? {
                                      key: "action",
                                      label: actionLabel(url.action),
                                  }
                                : null,
                            url.subjectType !== "all"
                                ? {
                                      key: "subjectType",
                                      label:
                                          url.subjectType === "user"
                                              ? "用户上限"
                                              : "角色",
                                  }
                                : null,
                            url.subjectId
                                ? {
                                      key: "subjectId",
                                      label: `主体：${subjectLabel(
                                          url.subjectType === "all"
                                              ? "role"
                                              : url.subjectType,
                                          url.subjectId,
                                      )}`,
                                  }
                                : null,
                            url.scopeType !== "all"
                                ? {
                                      key: "scopeType",
                                      label: SCOPE_TYPE_LABEL[url.scopeType],
                                  }
                                : null,
                        ].filter(
                            (chip): chip is { key: string; label: string } =>
                                chip != null,
                        )}
                        onClearChip={(key) => {
                            if (key === "q") pushUrl({ q: undefined })
                            if (key === "resource")
                                pushUrl({
                                    resource: undefined,
                                    action: undefined,
                                })
                            if (key === "action") pushUrl({ action: undefined })
                            if (key === "subjectType")
                                pushUrl({
                                    subjectType: "all",
                                    subjectId: undefined,
                                })
                            if (key === "subjectId")
                                pushUrl({ subjectId: undefined })
                            if (key === "scopeType")
                                pushUrl({ scopeType: "all" })
                        }}
                        onClearAll={() =>
                            pushUrl({
                                q: undefined,
                                resource: undefined,
                                action: undefined,
                                subjectType: "all",
                                subjectId: undefined,
                                scopeType: "all",
                            })
                        }
                        hasPendingChanges={
                            (searchDraft.trim() || undefined) !== url.q
                        }
                    />
                }
                table={
                    emptyReason === "NO_MODULE_PERMISSION" ? (
                        <OrganizationEmptyByReason
                            idPrefix="organization-scope"
                            reason={emptyReason}
                        />
                    ) : scopesQuery.isError && !scopesQuery.data ? (
                        <BusinessFailureState
                            id="organization-scope-retry"
                            title="范围配置加载失败"
                            error={scopesQuery.error}
                            onRetry={() => {
                                void scopesQuery.refetch()
                            }}
                        />
                    ) : emptyReason ? (
                        <OrganizationEmptyByReason
                            idPrefix="organization-scope"
                            reason={emptyReason}
                            onClearFilters={() =>
                                pushUrl({
                                    q: undefined,
                                    resource: undefined,
                                    action: undefined,
                                    subjectType: "all",
                                    subjectId: undefined,
                                    scopeType: "all",
                                })
                            }
                        />
                    ) : (
                        <ul className="min-w-0 space-y-2 overflow-x-hidden">
                            {items.map((row) => (
                                <li
                                    key={row.id}
                                    className="flex min-w-0 flex-wrap items-start justify-between gap-3 rounded-lg border px-3 py-3"
                                >
                                    <div className="min-w-0 space-y-1">
                                        <p className="font-medium wrap-anywhere">
                                            {resourceLabel(row.resource)} ·{" "}
                                            {row.actions
                                                .map((action) =>
                                                    actionLabel(action),
                                                )
                                                .join("、")}
                                        </p>
                                        <p className="text-sm text-muted-foreground wrap-anywhere">
                                            {row.subjectType === "user"
                                                ? "用户上限"
                                                : "角色"}{" "}
                                            ·{" "}
                                            {subjectLabel(
                                                row.subjectType,
                                                row.subjectId,
                                            )}{" "}
                                            · {SCOPE_TYPE_LABEL[row.scopeType]}
                                            {row.scopeTargets.length > 0
                                                ? ` · ${row.scopeTargets
                                                      .map((id) =>
                                                          unitLabel(units, id),
                                                      )
                                                      .join("、")}`
                                                : ""}
                                        </p>
                                    </div>
                                    {canDelete ? (
                                        <Button
                                            id={`organization-scope-delete-${toAutomationIdSegment(row.id)}`}
                                            type="button"
                                            size="sm"
                                            variant="ghost"
                                            onClick={() =>
                                                void deleteMutation.mutateAsync(
                                                    row.id,
                                                )
                                            }
                                        >
                                            删除
                                        </Button>
                                    ) : null}
                                </li>
                            ))}
                        </ul>
                    )
                }
            />

            {canCreate ? (
                <DataScopeFormDialog
                    open={createOpen}
                    onOpenChange={setCreateOpen}
                    roles={roles}
                    people={people}
                    units={units}
                    submitting={createMutation.isPending}
                    onSubmit={async (input) => {
                        await createMutation.mutateAsync(input)
                    }}
                />
            ) : null}
        </PageScaffold>
    )
}
