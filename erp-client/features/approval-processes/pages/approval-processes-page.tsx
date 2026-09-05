"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "next/navigation"

import {
    BusinessEmptyState,
    BusinessFailureState,
    MetricItem,
    MetricStrip,
    OptionCombobox,
    PageScaffold,
} from "@/components/business"
import {
    ListSearchField,
    ListWorkSurface,
    ListWorkspaceFilterBar,
    ListWorkspaceHeader,
    ListWorkspaceInlineFilter,
    ListWorkspaceViews,
    listWorkspaceEmptyStateClassName,
    listWorkspaceFilterStatusText,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"

import { CreateDraftDialog } from "../components/create-draft-dialog"
import { ProcessCatalog } from "../components/process-catalog"
import { definitionErrorMessage } from "../errors"
import { canReadCatalog } from "../permissions"
import { useDefinitionCatalogQuery } from "../queries"
import type { CatalogUrlState, DefinitionCatalogItem } from "../types"
import {
    buildCatalogSearchParams,
    hasUnknownCatalogParams,
    matchesCatalogFilters,
    parseCatalogSearchParams,
} from "../url-state"

const POLICY_OPTIONS = [
    { value: "ALL", label: "全部政策" },
    { value: "PROCESS_REQUIRED", label: "必须审批" },
    { value: "NO_APPROVAL", label: "无需审批" },
] as const

const STATUS_OPTIONS = [
    { value: "ALL", label: "全部状态" },
    { value: "PUBLISHED", label: "已发布" },
    { value: "MISSING_CONFIGURATION", label: "配置缺失" },
    { value: "HAS_DRAFT", label: "有草稿" },
    { value: "NOT_APPLICABLE", label: "无需审批 / 不适用" },
] as const

/**
 * W24 审批流程配置目录页。
 */
export function ApprovalProcessesPage() {
    const router = useRouter()
    const searchParams = useSearchParams()
    const profileQuery = useAccountProfileQuery()
    const catalogQuery = useDefinitionCatalogQuery()
    const [draftTarget, setDraftTarget] =
        React.useState<DefinitionCatalogItem | null>(null)
    const unknownParams = hasUnknownCatalogParams(
        new URLSearchParams(searchParams.toString()),
    )
    const urlState = React.useMemo(
        () =>
            parseCatalogSearchParams(
                new URLSearchParams(searchParams.toString()),
            ),
        [searchParams],
    )
    const [searchDraft, setSearchDraft] = React.useState(urlState.q)
    const [policyDraft, setPolicyDraft] = React.useState(urlState.policy)
    const [statusDraft, setStatusDraft] = React.useState(urlState.status)
    React.useEffect(() => {
        setSearchDraft(urlState.q)
    }, [urlState.q])
    React.useEffect(() => {
        setPolicyDraft(urlState.policy)
    }, [urlState.policy])
    React.useEffect(() => {
        setStatusDraft(urlState.status)
    }, [urlState.status])

    const replaceState = (next: CatalogUrlState) => {
        const query = buildCatalogSearchParams(next)
        router.replace(`/system/approval-processes${query}`)
    }

    const applyFilters = () => {
        replaceState({
            policy: policyDraft,
            status: statusDraft,
            q: searchDraft.trim(),
            page: 1,
        })
    }

    const clearAllFilters = () => {
        setSearchDraft("")
        setPolicyDraft("ALL")
        setStatusDraft("ALL")
        replaceState({
            policy: "ALL",
            status: "ALL",
            q: "",
            page: 1,
        })
    }

    const removeFilter = (key: string) => {
        if (key === "q") {
            setSearchDraft("")
            replaceState({ ...urlState, q: "", page: 1 })
        }
        if (key === "policy") {
            setPolicyDraft("ALL")
            replaceState({ ...urlState, policy: "ALL", page: 1 })
        }
        if (key === "status") {
            setStatusDraft("ALL")
            replaceState({ ...urlState, status: "ALL", page: 1 })
        }
    }

    const hasPendingChanges =
        searchDraft.trim() !== urlState.q.trim() ||
        policyDraft !== urlState.policy ||
        statusDraft !== urlState.status

    const appliedChips = [
        ...(urlState.q.trim()
            ? [{ key: "q", label: `搜索：${urlState.q.trim()}` }]
            : []),
        ...(urlState.policy !== "ALL"
            ? [
                  {
                      key: "policy",
                      label: `政策：${
                          POLICY_OPTIONS.find(
                              (option) => option.value === urlState.policy,
                          )?.label ?? urlState.policy
                      }`,
                  },
              ]
            : []),
        ...(urlState.status !== "ALL"
            ? [
                  {
                      key: "status",
                      label: `状态：${
                          STATUS_OPTIONS.find(
                              (option) => option.value === urlState.status,
                          )?.label ?? urlState.status
                      }`,
                  },
              ]
            : []),
    ]

    const permissions = profileQuery.data?.permissions
    const items = catalogQuery.data ?? []
    const filtered = items.filter((item) =>
        matchesCatalogFilters(item, urlState),
    )
    const required = items.filter(
        (item) => item.approval_requirement === "PROCESS_REQUIRED",
    )
    const missing = required.filter(
        (item) => item.configuration_status === "MISSING_CONFIGURATION",
    )
    const drafts = items.filter((item) => Boolean(item.draft_version))

    if (unknownParams) {
        return (
            <PageScaffold density="compact" className={styles.page}>
                <ListWorkspaceHeader
                    className="pb-6 md:pb-6"
                    eyebrow="系统"
                    title="审批流程配置"
                    description="按固定单据类型维护审批节点与审批人。"
                />
                <BusinessFailureState
                    kind="validation"
                    title="查询条件无效"
                    description="地址中的筛选参数无法识别，请清除后重新筛选。"
                    action={
                        <Button
                            id="governance-approval-processes-catalog-invalid-clear"
                            type="button"
                            onClick={() =>
                                router.replace("/system/approval-processes")
                            }
                        >
                            清除筛选
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (profileQuery.data && !canReadCatalog(permissions)) {
        return (
            <PageScaffold density="compact" className={styles.page}>
                <ListWorkspaceHeader
                    className="pb-6 md:pb-6"
                    eyebrow="系统"
                    title="审批流程配置"
                    description="按固定单据类型维护审批节点与审批人。"
                />
                <BusinessFailureState
                    kind="permission"
                    title="权限不足"
                    description="当前账号不能查看审批流程配置。"
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                className="pb-6 md:pb-6"
                eyebrow="系统"
                title="审批流程配置"
                description="按固定单据类型维护审批节点与审批人。"
            />

            <MetricStrip className="mb-6" columns={4}>
                <MetricItem
                    label="必须审批"
                    value={required.length}
                    density="compact"
                />
                <MetricItem
                    label="配置缺失"
                    value={missing.length}
                    density="compact"
                />
                <MetricItem
                    label="有草稿"
                    value={drafts.length}
                    density="compact"
                />
                <MetricItem
                    label="无需审批"
                    value={items.length - required.length}
                    density="compact"
                />
            </MetricStrip>

            <ListWorkSurface
                toolbarClassName="pt-3 pb-2"
                ariaLabel="审批流程单据类型目录"
                views={
                    <ListWorkspaceViews
                        ariaLabel="审批流程配置视图"
                        items={[
                            {
                                id: "governance-approval-processes-catalog-view-all",
                                label: "全部单据类型",
                                count: filtered.length,
                                active: true,
                                onClick: () => undefined,
                            },
                        ]}
                    />
                }
                toolbar={
                    <ListWorkspaceFilterBar
                        density="compact"
                        idPrefix="governance-approval-processes-catalog"
                        formAriaLabel="审批流程查询"
                        onSubmit={applyFilters}
                        search={
                            <ListSearchField
                                id="governance-approval-processes-catalog-search"
                                value={searchDraft}
                                onChange={setSearchDraft}
                                placeholder="搜索单据类型"
                                aria-label="搜索单据类型"
                            />
                        }
                        commonFilters={
                            <>
                                <ListWorkspaceInlineFilter
                                    htmlFor="governance-approval-processes-catalog-policy"
                                    label="审批政策"
                                >
                                    <OptionCombobox
                                        id="governance-approval-processes-catalog-policy"
                                        className="w-full sm:w-44"
                                        aria-label="审批政策"
                                        options={[...POLICY_OPTIONS]}
                                        value={policyDraft}
                                        allowClear={false}
                                        onValueChange={(value) =>
                                            setPolicyDraft(
                                                value === "PROCESS_REQUIRED" ||
                                                    value === "NO_APPROVAL"
                                                    ? value
                                                    : "ALL",
                                            )
                                        }
                                    />
                                </ListWorkspaceInlineFilter>
                                <ListWorkspaceInlineFilter
                                    htmlFor="governance-approval-processes-catalog-status"
                                    label="配置状态"
                                >
                                    <OptionCombobox
                                        id="governance-approval-processes-catalog-status"
                                        className="w-full sm:w-48"
                                        aria-label="配置状态"
                                        options={[...STATUS_OPTIONS]}
                                        value={statusDraft}
                                        allowClear={false}
                                        onValueChange={(value) =>
                                            setStatusDraft(
                                                value === "PUBLISHED" ||
                                                    value ===
                                                        "MISSING_CONFIGURATION" ||
                                                    value === "HAS_DRAFT" ||
                                                    value === "NOT_APPLICABLE"
                                                    ? value
                                                    : "ALL",
                                            )
                                        }
                                    />
                                </ListWorkspaceInlineFilter>
                            </>
                        }
                        resultStatus={listWorkspaceFilterStatusText({
                            loading: catalogQuery.isPending,
                            failed: catalogQuery.isError,
                            resultCount: catalogQuery.data
                                ? filtered.length
                                : undefined,
                            noun: "个单据类型",
                            loadingLabel: "正在加载目录…",
                        })}
                        chips={appliedChips}
                        onClearChip={removeFilter}
                        onClearAll={clearAllFilters}
                        hasPendingChanges={hasPendingChanges}
                    />
                }
                table={
                    catalogQuery.isPending && !catalogQuery.data ? (
                        <p className="p-4 text-sm text-muted-foreground">
                            正在加载目录…
                        </p>
                    ) : catalogQuery.isError ? (
                        <BusinessFailureState
                            kind="system"
                            title="目录加载失败"
                            description={definitionErrorMessage(
                                catalogQuery.error,
                            )}
                            action={
                                <Button
                                    id="governance-approval-processes-catalog-retry"
                                    type="button"
                                    onClick={() => void catalogQuery.refetch()}
                                >
                                    重试
                                </Button>
                            }
                        />
                    ) : filtered.length === 0 ? (
                        <BusinessEmptyState
                            kind="filter"
                            className={listWorkspaceEmptyStateClassName}
                            action={
                                <Button
                                    id="governance-approval-processes-catalog-empty-clear"
                                    type="button"
                                    variant="outline"
                                    onClick={() =>
                                        replaceState({
                                            policy: "ALL",
                                            status: "ALL",
                                            q: "",
                                            page: 1,
                                        })
                                    }
                                >
                                    清除筛选
                                </Button>
                            }
                        />
                    ) : (
                        <ProcessCatalog
                            id="governance-approval-processes-catalog"
                            items={filtered}
                            permissions={permissions}
                            onCreateDraft={setDraftTarget}
                            onContinueDraft={(item) =>
                                router.push(
                                    `/system/approval-processes/${item.document_type}?view=draft`,
                                )
                            }
                        />
                    )
                }
            />

            <CreateDraftDialog
                id="governance-approval-processes-catalog-create-draft-dialog"
                item={draftTarget}
                open={Boolean(draftTarget)}
                onOpenChange={(open) => {
                    if (!open) setDraftTarget(null)
                }}
                onCreated={(_definitionId, documentType) =>
                    router.push(
                        `/system/approval-processes/${documentType}?view=draft`,
                    )
                }
            />
        </PageScaffold>
    )
}
