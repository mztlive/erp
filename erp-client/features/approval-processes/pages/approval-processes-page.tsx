"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "next/navigation"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    ListToolbar,
    MetricItem,
    MetricStrip,
    OptionCombobox,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
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
    React.useEffect(() => {
        setSearchDraft(urlState.q)
    }, [urlState.q])

    const replaceState = (next: CatalogUrlState) => {
        const query = buildCatalogSearchParams(next)
        router.replace(`/system/approval-processes${query}`)
    }

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
            <PageScaffold>
                <PageHeader title="审批流程配置" />
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
            <PageScaffold>
                <PageHeader title="审批流程配置" />
                <BusinessFailureState
                    kind="permission"
                    title="权限不足"
                    description="当前账号不能查看审批流程配置。"
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold density="compact">
            <PageHeader
                title="审批流程配置"
                description="按固定单据类型维护审批节点、审批人和版本。不得创建自定义单据类型。"
            />
            <MetricStrip columns={4}>
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
            <BusinessTableFrame
                title="单据类型目录"
                description="目录固定展示 20 个单据类型。配置缺失是阻断状态，不是无需审批。"
                toolbar={
                    <ListToolbar
                        search={
                            <Input
                                id="governance-approval-processes-catalog-search"
                                aria-label="搜索单据类型"
                                value={searchDraft}
                                placeholder="搜索单据类型"
                                onChange={(event) =>
                                    setSearchDraft(event.target.value)
                                }
                            />
                        }
                        filters={
                            <>
                                <OptionCombobox
                                    id="governance-approval-processes-catalog-policy"
                                    aria-label="审批政策"
                                    options={[...POLICY_OPTIONS]}
                                    value={urlState.policy}
                                    allowClear={false}
                                    onValueChange={(value) =>
                                        replaceState({
                                            ...urlState,
                                            policy:
                                                value === "PROCESS_REQUIRED" ||
                                                value === "NO_APPROVAL"
                                                    ? value
                                                    : "ALL",
                                            page: 1,
                                        })
                                    }
                                />
                                <OptionCombobox
                                    id="governance-approval-processes-catalog-status"
                                    aria-label="配置状态"
                                    options={[...STATUS_OPTIONS]}
                                    value={urlState.status}
                                    allowClear={false}
                                    onValueChange={(value) =>
                                        replaceState({
                                            ...urlState,
                                            status:
                                                value === "PUBLISHED" ||
                                                value ===
                                                    "MISSING_CONFIGURATION" ||
                                                value === "HAS_DRAFT" ||
                                                value === "NOT_APPLICABLE"
                                                    ? value
                                                    : "ALL",
                                            page: 1,
                                        })
                                    }
                                />
                            </>
                        }
                        actions={
                            <>
                                <Button
                                    id="governance-approval-processes-catalog-clear"
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
                                <Button
                                    id="governance-approval-processes-catalog-search-submit"
                                    type="button"
                                    onClick={() =>
                                        replaceState({
                                            ...urlState,
                                            q: searchDraft.trim(),
                                            page: 1,
                                        })
                                    }
                                >
                                    搜索
                                </Button>
                            </>
                        }
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
