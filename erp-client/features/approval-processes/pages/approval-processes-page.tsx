"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "@/lib/navigation"

import {
    BusinessEmptyState,
    BusinessFailureState,
    OptionCombobox,
} from "@/components/business"
import {
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    ListWorkspacePage,
    listWorkspaceEmptyStateClassName,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import { useSearchDraft } from "@/lib/use-search-draft"
import { useSearchHotkey } from "@/lib/use-search-hotkey"

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

const CATALOG_ID = "governance-approval-processes-catalog"
const CATALOG_TITLE = "审批流程配置"
const CATALOG_PATH = "/system/approval-processes"

const POLICY_OPTIONS = [
    { value: "ALL", label: "全部要求" },
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
    const [moreOpen, setMoreOpen] = React.useState(false)
    const searchInputRef = React.useRef<HTMLInputElement>(null)
    useSearchHotkey(searchInputRef)
    const { searchDraft, setSearchDraft } = useSearchDraft(urlState.q)
    const [policyDraft, setPolicyDraft] = React.useState(urlState.policy)
    const [statusDraft, setStatusDraft] = React.useState(urlState.status)
    React.useEffect(() => {
        setPolicyDraft(urlState.policy)
    }, [urlState.policy])
    React.useEffect(() => {
        setStatusDraft(urlState.status)
    }, [urlState.status])

    const replaceState = (next: CatalogUrlState) => {
        const query = buildCatalogSearchParams(next)
        router.replace(`${CATALOG_PATH}${query}`)
    }

    const applyFilters = () => {
        setMoreOpen(false)
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
                      label: `要求：${
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

    const hasActiveFilters = appliedChips.length > 0
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
            <ListWorkspacePage title={CATALOG_TITLE}>
                <BusinessFailureState
                    kind="validation"
                    title="查询条件无效"
                    description="地址中的筛选参数无法识别，请清除后重新筛选。"
                    action={
                        <Button
                            id={`${CATALOG_ID}-invalid-clear`}
                            type="button"
                            variant="secondary"
                            size="sm"
                            onClick={() => router.replace(CATALOG_PATH)}
                        >
                            清除筛选
                        </Button>
                    }
                />
            </ListWorkspacePage>
        )
    }

    if (profileQuery.data && !canReadCatalog(permissions)) {
        return (
            <ListWorkspacePage title={CATALOG_TITLE}>
                <BusinessFailureState
                    kind="permission"
                    title="权限不足"
                    description="当前账号不能查看审批流程配置。"
                />
            </ListWorkspacePage>
        )
    }

    const presets = [
        {
            key: "all",
            label: "全部",
            policy: "ALL",
            status: "ALL",
            count: items.length,
        },
        {
            key: "missing",
            label: "待配置",
            policy: "ALL",
            status: "MISSING_CONFIGURATION",
            count: missing.length,
        },
        {
            key: "draft",
            label: "有草稿",
            policy: "ALL",
            status: "HAS_DRAFT",
            count: drafts.length,
        },
        {
            key: "no-approval",
            label: "无需审批",
            policy: "NO_APPROVAL",
            status: "ALL",
            count: items.length - required.length,
        },
    ] as const

    return (
        <ListWorkspacePage
            title={CATALOG_TITLE}
            views={{
                ariaLabel: "审批配置快捷筛选",
                items: presets.map((view) => ({
                    id: `${CATALOG_ID}-view-${view.key}`,
                    label: view.label,
                    count: catalogQuery.isSuccess ? view.count : undefined,
                    active:
                        urlState.policy === view.policy &&
                        urlState.status === view.status,
                    onClick: () =>
                        replaceState({
                            ...urlState,
                            policy: view.policy,
                            status: view.status,
                            page: 1,
                        }),
                })),
            }}
            surface={{
                ariaLabel: "审批流程单据类型目录",
                tableClassName:
                    "overflow-y-auto px-(--scaffold-px) py-workspace-toolbar-block",
                toolbar: (
                    <ListWorkspaceFilterBar
                        morePresentation="popover"
                        idPrefix={CATALOG_ID}
                        formAriaLabel="审批流程查询"
                        onSubmit={applyFilters}
                        search={
                            <ListSearchField
                                id={`${CATALOG_ID}-search`}
                                searchInputRef={searchInputRef}
                                value={searchDraft}
                                onChange={setSearchDraft}
                                placeholder="搜索审批流程"
                                aria-label="搜索单据类型"
                            />
                        }
                        moreOpen={moreOpen}
                        onToggleMore={() => setMoreOpen((open) => !open)}
                        moreCount={
                            appliedChips.filter((chip) => chip.key !== "q")
                                .length
                        }
                        onResetMore={() => {
                            setPolicyDraft("ALL")
                            setStatusDraft("ALL")
                        }}
                        morePanel={
                            <ListFilterSections>
                                <ListFilterSection legend="审批配置">
                                    <ListWorkspaceFilterField
                                        htmlFor={`${CATALOG_ID}-policy`}
                                        label="审批要求"
                                    >
                                        <OptionCombobox
                                            id={`${CATALOG_ID}-policy`}
                                            options={POLICY_OPTIONS}
                                            value={
                                                policyDraft === "ALL"
                                                    ? null
                                                    : policyDraft
                                            }
                                            allowClear={false}
                                            placeholder="全部要求"
                                            onValueChange={(value) =>
                                                setPolicyDraft(
                                                    value ===
                                                        "PROCESS_REQUIRED" ||
                                                        value === "NO_APPROVAL"
                                                        ? value
                                                        : "ALL",
                                                )
                                            }
                                        />
                                    </ListWorkspaceFilterField>
                                    <ListWorkspaceFilterField
                                        htmlFor={`${CATALOG_ID}-status`}
                                        label="配置状态"
                                    >
                                        <OptionCombobox
                                            id={`${CATALOG_ID}-status`}
                                            options={STATUS_OPTIONS}
                                            value={
                                                statusDraft === "ALL"
                                                    ? null
                                                    : statusDraft
                                            }
                                            allowClear={false}
                                            placeholder="全部状态"
                                            onValueChange={(value) =>
                                                setStatusDraft(
                                                    value === "PUBLISHED" ||
                                                        value ===
                                                            "MISSING_CONFIGURATION" ||
                                                        value === "HAS_DRAFT" ||
                                                        value ===
                                                            "NOT_APPLICABLE"
                                                        ? value
                                                        : "ALL",
                                                )
                                            }
                                        />
                                    </ListWorkspaceFilterField>
                                </ListFilterSection>
                            </ListFilterSections>
                        }
                        resultStatus={listWorkspaceFilterStatusText({
                            loading: catalogQuery.isFetching,
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
                ),
                table: catalogQuery.isPending ? (
                    <div
                        aria-busy="true"
                        aria-label="正在加载目录"
                        className="grid gap-3 md:grid-cols-2 @[960px]/workspace:grid-cols-3"
                    >
                        <Skeleton className="h-20" />
                        <Skeleton className="h-20" />
                        <Skeleton className="h-20" />
                    </div>
                ) : catalogQuery.isError || !catalogQuery.data ? (
                    <BusinessFailureState
                        kind="system"
                        title="目录加载失败"
                        description={definitionErrorMessage(catalogQuery.error)}
                        action={
                            <Button
                                id={`${CATALOG_ID}-retry`}
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => void catalogQuery.refetch()}
                            >
                                重试
                            </Button>
                        }
                    />
                ) : filtered.length === 0 ? (
                    <BusinessEmptyState
                        kind={hasActiveFilters ? "filter" : "no-data"}
                        title={
                            hasActiveFilters
                                ? undefined
                                : "暂无审批流程单据类型"
                        }
                        className={listWorkspaceEmptyStateClassName}
                        action={
                            hasActiveFilters ? (
                                <Button
                                    id={`${CATALOG_ID}-empty-clear`}
                                    type="button"
                                    variant="secondary"
                                    size="sm"
                                    className="rounded-lg shadow-none"
                                    onClick={clearAllFilters}
                                >
                                    清除筛选
                                </Button>
                            ) : undefined
                        }
                    />
                ) : (
                    <ProcessCatalog
                        id={CATALOG_ID}
                        items={filtered}
                        permissions={permissions}
                        onCreateDraft={setDraftTarget}
                        onContinueDraft={(item) =>
                            router.push(
                                `${CATALOG_PATH}/${item.document_type}?view=draft`,
                            )
                        }
                    />
                ),
            }}
        >
            <CreateDraftDialog
                id={`${CATALOG_ID}-create-draft-dialog`}
                item={draftTarget}
                open={Boolean(draftTarget)}
                onOpenChange={(open) => {
                    if (!open) setDraftTarget(null)
                }}
                onCreated={(_definitionId, documentType) =>
                    router.push(`${CATALOG_PATH}/${documentType}?view=draft`)
                }
            />
        </ListWorkspacePage>
    )
}
