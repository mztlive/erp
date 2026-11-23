"use client"

import { useEffect, useState } from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import {
    BanIcon,
    CircleCheckIcon,
    DownloadIcon,
    FileTextIcon,
    PlusIcon,
    SettingsIcon,
} from "lucide-react"
import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    PageActions,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import {
    ListDataTable,
    ListWorkspacePage,
    listWorkspaceEmptyStateClassName,
    listWorkspaceStyles as styles,
    type ListWorkspaceViewItem,
} from "@/components/business/list-workspace"
import { Alert } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTimeInZone } from "@/lib/datetime"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import type { ContractApplication, ContractTemplate } from "../api/templates"
import {
    TemplateApplyDialog,
    readPendingContract,
    type PendingContractApplication,
} from "../components/template-apply-dialog"
import { TemplateCounterDialog } from "../components/template-counter-dialog"
import { TemplateUploadDialog } from "../components/template-upload-dialog"
import {
    useContractApplicationsQuery,
    useContractTemplatesQuery,
    useTemplateMutations,
} from "../hooks/template-queries"

type Selection = {
    template: ContractTemplate
    pending?: PendingContractApplication | null
    triggerId: string
}

type View = "templates" | "applications"

const TEMPLATE_IDENTITY_LAYOUT =
    "max-sm:[&_[data-column-id=name]]:min-w-table-column-default-min"
const APPLICATION_IDENTITY_LAYOUT =
    "max-sm:[&_[data-column-id='contract_no']]:min-w-table-column-default-min"
const MANAGED_ACTIONS_LAYOUT =
    "[&_[data-column-id=actions]]:w-48 [&_[data-column-id=actions]]:min-w-48"
const SCROLLABLE_TABLE_LAYOUT =
    "[&_[data-slot=data-table-surface]]:flex [&_[data-slot=data-table-surface]]:flex-col [&_[data-slot=table-container]]:min-h-0 [&_[data-slot=table-container]]:flex-1"
const RESULTS_HEADING_ID = "contract-templates-results-heading"
const RESUME_ID = "contract-application-resume"

export function ContractTemplatesPage() {
    const account = useAccountProfileQuery()
    const permissions = account.data?.permissions
    const accountId = account.data?.userid ?? ""
    const canList = hasPermission(permissions, "contract_template:list")
    const canManage = hasPermission(permissions, "contract_template:manage")
    const canApply = hasPermission(permissions, "contract_application:create")
    const canDownload = hasPermission(
        permissions,
        "contract_application:download",
    )
    const canHistory = hasPermission(permissions, "contract_application:list")
    const [preferredView, setPreferredView] = useState<View>("templates")
    const view =
        canHistory && (!canList || preferredView === "applications")
            ? "applications"
            : "templates"
    const [pagination, setPagination] = useState({ pageIndex: 0, pageSize: 20 })
    const [uploadOpen, setUploadOpen] = useState(false)
    const [counterOpen, setCounterOpen] = useState(false)
    const [selected, setSelected] = useState<Selection | null>(null)
    const [recovery, setRecovery] = useState<PendingContractApplication | null>(
        null,
    )
    const [error, setError] = useState("")
    const canArchive = hasPermission(permissions, "contract:list")
    const canUploadArchive = hasPermission(permissions, "contract:create")
    const canUseTemplateActions = canApply || canManage
    const templates = useContractTemplatesQuery(
        {
            page: pagination.pageIndex + 1,
            page_size: pagination.pageSize,
            include_disabled: canManage,
        },
        canList && view === "templates",
    )
    const applications = useContractApplicationsQuery(
        { page: pagination.pageIndex + 1, page_size: pagination.pageSize },
        accountId,
        canHistory && view === "applications",
    )
    const mutations = useTemplateMutations()

    useEffect(() => {
        setSelected(null)
        setRecovery(accountId ? readPendingContract(accountId) : null)
    }, [accountId])

    const switchView = (next: View) => {
        setPreferredView(next)
        setPagination((current) => ({ ...current, pageIndex: 0 }))
        setError("")
    }

    const views: ListWorkspaceViewItem[] = []
    if (canList) {
        views.push({
            id: "contract-templates-view",
            label: "合同模板",
            active: view === "templates",
            onClick: () => switchView("templates"),
        })
    }
    if (canHistory) {
        views.push({
            id: "contract-applications-view",
            label: "我的合同申请",
            active: view === "applications",
            onClick: () => switchView("applications"),
        })
    }

    const apply = (template: ContractTemplate, triggerId: string) => {
        const pending = readPendingContract(accountId)
        setSelected({
            template: pending?.template ?? template,
            pending,
            triggerId,
        })
    }
    const download = (input: {
        id: string
        filename: string
        sample?: boolean
    }) => {
        setError("")
        mutations.download.mutate(input, {
            onError: (cause) =>
                setError(getErrorMessage(cause, "Word 下载失败，请重试")),
        })
    }
    const templateColumns: ColumnDef<ContractTemplate>[] = [
        {
            accessorKey: "name",
            header: "模板名称",
            meta: { width: "flex" },
            enableHiding: false,
            cell: ({ row }) => (
                <TableIdentityCell
                    primary={row.original.name}
                    primaryTitle={row.original.name}
                    className="w-0 min-w-full"
                />
            ),
        },
        {
            accessorKey: "company_name",
            header: "签约公司",
            cell: ({ row }) => (
                <TableTextCell value={row.original.company_name} clamp={2} />
            ),
        },
        {
            accessorKey: "group",
            header: "编号组",
            cell: ({ row }) => (
                <TableTextCell
                    value={`${row.original.group}-S-年度流水`}
                    numeric
                />
            ),
        },
        {
            accessorKey: "enabled",
            header: "状态",
            meta: { width: "status" },
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    tone={row.original.enabled ? "success" : "neutral"}
                    appearance={row.original.enabled ? "quiet" : "badge"}
                    label={row.original.enabled ? "启用" : "停用"}
                />
            ),
        },
    ]
    if (canUseTemplateActions) {
        templateColumns.push({
            id: "actions",
            header: "操作",
            meta: { align: "end" },
            enableHiding: false,
            cell: ({ row }) => {
                const template = row.original
                const segment = toAutomationIdSegment(template.id)
                const applyId = `contract-template-apply-${segment}`
                return (
                    <TableRowActions
                        moreId={`contract-template-more-${segment}`}
                        moreLabel={`${template.name} 更多操作`}
                        maxInline={2}
                        actions={[
                            {
                                id: applyId,
                                label: "申请并下载",
                                icon: DownloadIcon,
                                hidden: !canApply,
                                disabled: !template.enabled,
                                disabledReason: template.enabled
                                    ? undefined
                                    : "此模板已停用，不能申请编号。",
                                onClick: () => apply(template, applyId),
                            },
                            {
                                id: `contract-template-sample-${segment}`,
                                label: "编号样张",
                                icon: FileTextIcon,
                                hidden: !canManage,
                                disabled: mutations.download.isPending,
                                onClick: () =>
                                    download({
                                        id: template.id,
                                        filename: `${template.group}-S-SAMPLE.docx`,
                                        sample: true,
                                    }),
                            },
                            {
                                id: `contract-template-status-${segment}`,
                                label: template.enabled ? "停用" : "启用",
                                icon: template.enabled
                                    ? BanIcon
                                    : CircleCheckIcon,
                                hidden: !canManage,
                                placement: "menu",
                                destructive: template.enabled,
                                disabled: mutations.status.isPending,
                                onClick: () => {
                                    setError("")
                                    mutations.status.mutate(
                                        {
                                            id: template.id,
                                            version: template.version,
                                            enabled: !template.enabled,
                                        },
                                        {
                                            onError: (cause) =>
                                                setError(
                                                    getErrorMessage(
                                                        cause,
                                                        "模板状态修改失败，请刷新后重试",
                                                    ),
                                                ),
                                        },
                                    )
                                },
                            },
                        ]}
                    />
                )
            },
        })
    }
    const applicationColumns: ColumnDef<ContractApplication>[] = [
        {
            accessorKey: "contract_no",
            header: "合同编号",
            meta: { width: "flex", numeric: true },
            enableHiding: false,
            cell: ({ row }) => (
                <TableIdentityCell
                    primary={row.original.contract_no}
                    primaryTitle={row.original.contract_no}
                    className="w-0 min-w-full"
                />
            ),
        },
        {
            accessorKey: "company_name",
            header: "签约公司",
            cell: ({ row }) => (
                <TableTextCell value={row.original.company_name} clamp={2} />
            ),
        },
        {
            accessorKey: "template_name",
            header: "模板",
            cell: ({ row }) => (
                <TableTextCell value={row.original.template_name} clamp={2} />
            ),
        },
        {
            accessorKey: "purpose",
            header: "申请用途",
            meta: { width: "reference" },
            cell: ({ row }) => (
                <TableTextCell value={row.original.purpose} clamp={2} />
            ),
        },
        {
            accessorKey: "created_at",
            header: "申请时间",
            meta: { numeric: true },
            cell: ({ row }) => {
                const createdAt = new Date(row.original.created_at * 1000)
                if (!Number.isFinite(createdAt.getTime())) {
                    return <TableEmptyCell />
                }
                return (
                    <time className="num" dateTime={createdAt.toISOString()}>
                        {formatDateTimeInZone(
                            createdAt.toISOString(),
                            "Asia/Shanghai",
                        )}
                    </time>
                )
            },
        },
    ]
    if (canDownload) {
        applicationColumns.push({
            id: "actions",
            header: "操作",
            meta: { align: "end" },
            enableHiding: false,
            cell: ({ row }) => {
                const application = row.original
                const segment = toAutomationIdSegment(application.id)
                return (
                    <TableRowActions
                        moreId={`contract-application-more-${segment}`}
                        moreLabel={`${application.contract_no} 更多操作`}
                        actions={[
                            {
                                id: `contract-application-download-${segment}`,
                                label: "下载 Word / 打印",
                                disabled: mutations.download.isPending,
                                onClick: () =>
                                    download({
                                        id: application.id,
                                        filename: `${application.contract_no}.docx`,
                                    }),
                            },
                        ]}
                    />
                )
            },
        })
    }

    const showingTemplates = view === "templates"
    const query = showingTemplates ? templates : applications
    const resultsLabel = showingTemplates ? "合同模板目录" : "我的合同申请"
    const hasListAccess = canList || canHistory
    const tableId = showingTemplates
        ? "contract-templates-table"
        : "contract-applications-table"
    const usageHint = showingTemplates
        ? `${canApply ? "请选择与签约公司一致的模板。" : ""}编号自动填写在模板首页右上角，同一编号组共用年度流水。`
        : `${canDownload ? "重复下载保留原编号，下载后用 Word 打印。" : ""}${
              canArchive && canUploadArchive
                  ? "签署后请到合同档案上传签署版 PDF。"
                  : ""
          }`
    const tableState = {
        pagination,
        onPaginationChange: setPagination,
        loading: query.isPending || query.isFetching,
        scrollable: true,
        className: SCROLLABLE_TABLE_LAYOUT,
        rowCount: query.data?.total ?? 0,
        errorState:
            query.isError || (!query.isPending && !query.data) ? (
                <div role="alert">
                    <BusinessFailureState
                        title="合同资料加载失败"
                        error={query.error}
                        action={
                            <Button
                                id="contract-templates-retry"
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={query.isFetching}
                                onClick={() => void query.refetch()}
                            >
                                重新加载
                            </Button>
                        }
                    />
                </div>
            ) : undefined,
    }
    if (account.isError && !account.data) {
        return (
            <ListWorkspacePage
                title="合同模板与申请"
                alerts={
                    <BusinessFailureState
                        idPrefix="contract-templates-account"
                        error={account.error}
                        onRetry={() => void account.refetch()}
                    />
                }
            />
        )
    }

    const table = showingTemplates ? (
        <ListDataTable
            {...tableState}
            key="templates"
            id={tableId}
            caption={resultsLabel}
            data={templates.data?.items ?? []}
            columns={templateColumns}
            defaultColumnOrder={[
                "name",
                "company_name",
                "group",
                "enabled",
                ...(canUseTemplateActions ? ["actions"] : []),
            ]}
            defaultColumnPinning={{
                left: ["name"],
                right: canUseTemplateActions ? ["actions"] : [],
            }}
            getRowId={(row) => row.id}
            rowLabel={(row) => row.name}
            emptyCopy={{
                emptyTitle: canManage ? "暂无合同模板" : "暂无启用的合同模板",
                emptyDescription: canManage
                    ? "请上传签约公司对应的 Word 模板，供销售申请编号并下载。"
                    : "请联系管理员上传或启用签约公司对应的 Word 模板。",
            }}
            canCreate={canManage}
            createLabel="上传 Word 模板"
            onCreate={() => setUploadOpen(true)}
        />
    ) : (
        <ListDataTable
            {...tableState}
            key="applications"
            id={tableId}
            caption={resultsLabel}
            data={applications.data?.items ?? []}
            columns={applicationColumns}
            defaultColumnOrder={[
                "contract_no",
                "company_name",
                "template_name",
                "purpose",
                "created_at",
                ...(canDownload ? ["actions"] : []),
            ]}
            defaultColumnPinning={{
                left: ["contract_no"],
                right: canDownload ? ["actions"] : [],
            }}
            getRowId={(row) => row.id}
            rowLabel={(row) => row.contract_no}
            emptyCopy={{
                emptyTitle: "暂无合同申请",
                emptyDescription:
                    canList && canApply
                        ? "在合同模板中选择签约公司对应的模板，申请编号后即可在此查看。"
                        : "当前账号尚无合同申请记录。",
            }}
        />
    )
    return (
        <ListWorkspacePage
            title="合同模板与申请"
            className="min-h-0"
            views={{ ariaLabel: "合同模板与申请视图", items: views }}
            loading={account.isPending}
            resultsLabel={hasListAccess ? resultsLabel : undefined}
            resultsHeadingId={RESULTS_HEADING_ID}
            headerActions={
                canArchive || canManage ? (
                    <>
                        {canArchive && (
                            <Button
                                id="contract-templates-archives-link"
                                size="sm"
                                variant="ghost"
                                className={styles.quietButton}
                                render={<Link href="/sales/contracts" />}
                            >
                                已签合同档案
                            </Button>
                        )}
                        {canManage && (
                            <PageActions
                                idPrefix="contract-templates-page"
                                className="gap-1.5"
                                actions={[
                                    {
                                        actionKey: "counter",
                                        label: "年度流水",
                                        icon: SettingsIcon,
                                        variant: "ghost",
                                        className: styles.quietButton,
                                        onClick: () => setCounterOpen(true),
                                    },
                                    {
                                        actionKey: "upload",
                                        label: "上传 Word 模板",
                                        icon: PlusIcon,
                                        onClick: () => setUploadOpen(true),
                                    },
                                ]}
                            />
                        )}
                    </>
                ) : undefined
            }
            alerts={error ? <Alert variant="destructive">{error}</Alert> : null}
            notices={
                recovery && canApply ? (
                    <Alert
                        variant="warning"
                        className="flex flex-wrap items-center justify-between gap-2"
                    >
                        <p className="text-body-compact">
                            上次申请结果待核对：{recovery.template.name}
                            。继续核对会保留原申请编号。
                        </p>
                        <Button
                            id={RESUME_ID}
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() =>
                                setSelected({
                                    template: recovery.template,
                                    pending: recovery,
                                    triggerId: RESUME_ID,
                                })
                            }
                        >
                            继续上次申请
                        </Button>
                    </Alert>
                ) : null
            }
            surface={
                hasListAccess
                    ? {
                          ariaLabel: resultsLabel,
                          toolbar: usageHint ? (
                              <p className="text-xs leading-relaxed text-muted-foreground">
                                  {usageHint}
                              </p>
                          ) : undefined,
                          tableClassName: cn(
                              showingTemplates
                                  ? TEMPLATE_IDENTITY_LAYOUT
                                  : APPLICATION_IDENTITY_LAYOUT,
                              showingTemplates &&
                                  canManage &&
                                  canApply &&
                                  MANAGED_ACTIONS_LAYOUT,
                          ),
                          table,
                      }
                    : undefined
            }
        >
            {!hasListAccess ? (
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前账号没有合同模板或申请记录的查看权限。"
                    description="如需查看，请联系管理员开通权限。"
                    className={listWorkspaceEmptyStateClassName}
                />
            ) : null}
            {uploadOpen && canManage && (
                <TemplateUploadDialog onClose={() => setUploadOpen(false)} />
            )}
            {counterOpen && canManage && (
                <TemplateCounterDialog onClose={() => setCounterOpen(false)} />
            )}
            {selected && canApply && (
                <TemplateApplyDialog
                    key={
                        selected.pending?.input.command_id ??
                        selected.template.id
                    }
                    template={selected.template}
                    pending={selected.pending}
                    accountId={accountId}
                    finalFocus={() => {
                        const trigger = document.getElementById(
                            selected.triggerId,
                        )
                        if (
                            trigger instanceof HTMLButtonElement &&
                            !trigger.disabled &&
                            (selected.triggerId !== RESUME_ID ||
                                readPendingContract(accountId))
                        ) {
                            return trigger
                        }
                        return (
                            document.getElementById(
                                `${tableId}-column-visibility-trigger`,
                            ) ??
                            document.getElementById(RESULTS_HEADING_ID) ??
                            true
                        )
                    }}
                    onClose={() => {
                        setSelected(null)
                        setRecovery(readPendingContract(accountId))
                    }}
                />
            )}
        </ListWorkspacePage>
    )
}
