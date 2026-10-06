"use client"

import { useEffect, useState } from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import { DownloadIcon, PlusIcon, SettingsIcon } from "lucide-react"
import {
    BusinessEmptyState,
    DataTable,
    PageActions,
    PageScaffold,
} from "@/components/business"
import {
    ListWorkspaceHeader,
    ListWorkspaceViews,
    ListWorkSurface,
    listWorkspaceEmptyStateClassName,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
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
}

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
    const [view, setView] = useState<"templates" | "applications">("templates")
    const [pagination, setPagination] = useState({ pageIndex: 0, pageSize: 20 })
    const [uploadOpen, setUploadOpen] = useState(false)
    const [counterOpen, setCounterOpen] = useState(false)
    const [selected, setSelected] = useState<Selection | null>(null)
    const [recovery, setRecovery] = useState<PendingContractApplication | null>(
        null,
    )
    const [error, setError] = useState("")
    const templates = useContractTemplatesQuery(
        {
            page: pagination.pageIndex + 1,
            page_size: pagination.pageSize,
            include_disabled: canManage,
        },
        (canList || canManage) && view === "templates",
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

    useEffect(() => {
        if (canHistory && !(canList || canManage)) setView("applications")
    }, [canHistory, canList, canManage])

    const apply = (template: ContractTemplate) => {
        const pending = readPendingContract(accountId)
        setSelected({ template: pending?.template ?? template, pending })
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
        { accessorKey: "name", header: "模板名称" },
        { accessorKey: "company_name", header: "签约公司" },
        {
            accessorKey: "group",
            header: "编号组",
            cell: ({ row }) => (
                <span className="num">{row.original.group}-S-年度流水</span>
            ),
        },
        {
            accessorKey: "enabled",
            header: "状态",
            cell: ({ row }) => (
                <Badge variant={row.original.enabled ? "secondary" : "outline"}>
                    {row.original.enabled ? "启用" : "停用"}
                </Badge>
            ),
        },
        {
            id: "actions",
            header: "操作",
            meta: { align: "end" },
            size: canManage ? 320 : 180,
            enableHiding: false,
            cell: ({ row }) => {
                const template = row.original
                const segment = toAutomationIdSegment(template.id)
                return (
                    <div className="flex flex-wrap justify-end gap-2">
                        {canApply && (
                            <Button
                                id={`contract-template-apply-${segment}`}
                                size="sm"
                                disabled={!template.enabled}
                                onClick={() => apply(template)}
                            >
                                申请并下载
                            </Button>
                        )}
                        {canManage && (
                            <>
                                <Button
                                    id={`contract-template-sample-${segment}`}
                                    size="sm"
                                    variant="outline"
                                    disabled={mutations.download.isPending}
                                    onClick={() =>
                                        download({
                                            id: template.id,
                                            filename: `${template.group}-S-SAMPLE.docx`,
                                            sample: true,
                                        })
                                    }
                                >
                                    编号样张
                                </Button>
                                <Button
                                    id={`contract-template-status-${segment}`}
                                    size="sm"
                                    variant="ghost"
                                    disabled={mutations.status.isPending}
                                    onClick={() => {
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
                                    }}
                                >
                                    {template.enabled ? "停用" : "启用"}
                                </Button>
                            </>
                        )}
                    </div>
                )
            },
        },
    ]
    const applicationColumns: ColumnDef<ContractApplication>[] = [
        {
            accessorKey: "contract_no",
            header: "合同编号",
            cell: ({ row }) => (
                <span className="num">{row.original.contract_no}</span>
            ),
        },
        { accessorKey: "company_name", header: "签约公司" },
        { accessorKey: "template_name", header: "模板" },
        {
            accessorKey: "purpose",
            header: "申请用途",
            cell: ({ row }) => row.original.purpose || "—",
        },
        {
            accessorKey: "created_at",
            header: "申请时间",
            cell: ({ row }) =>
                new Date(row.original.created_at * 1000).toLocaleString(
                    "zh-CN",
                    { timeZone: "Asia/Shanghai" },
                ),
        },
        {
            id: "actions",
            header: "操作",
            meta: { align: "end" },
            enableHiding: false,
            cell: ({ row }) => (
                <Button
                    id={`contract-application-download-${toAutomationIdSegment(row.original.id)}`}
                    size="sm"
                    variant="outline"
                    disabled={!canDownload || mutations.download.isPending}
                    onClick={() =>
                        download({
                            id: row.original.id,
                            filename: `${row.original.contract_no}.docx`,
                        })
                    }
                >
                    <DownloadIcon data-icon="inline-start" />
                    下载 Word / 打印
                </Button>
            ),
        },
    ]
    const query = view === "templates" ? templates : applications
    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="销售"
                title="合同模板与申请"
                description="选择签约公司的 Word 模板，申请编号后下载并打印。"
            >
                <div className="flex flex-wrap items-center justify-end gap-2">
                    {hasPermission(permissions, "contract:list") && (
                        <Button
                            id="contract-templates-archives-link"
                            variant="ghost"
                            render={<Link href="/sales/contracts" />}
                        >
                            已签合同档案
                        </Button>
                    )}
                    <PageActions
                        idPrefix="contract-templates-page"
                        actions={
                            canManage
                                ? [
                                      {
                                          actionKey: "counter",
                                          label: "年度流水",
                                          icon: SettingsIcon,
                                          variant: "outline",
                                          onClick: () => setCounterOpen(true),
                                      },
                                      {
                                          actionKey: "upload",
                                          label: "上传 Word 模板",
                                          icon: PlusIcon,
                                          onClick: () => setUploadOpen(true),
                                      },
                                  ]
                                : []
                        }
                    />
                </div>
            </ListWorkspaceHeader>
            {recovery && canApply && (
                <div className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-md border p-3">
                    <p className="text-sm">
                        上次申请结果待核对：{recovery.template.name}
                        。继续核对会保留原申请编号。
                    </p>
                    <Button
                        id="contract-application-resume"
                        variant="outline"
                        onClick={() =>
                            setSelected({
                                template: recovery.template,
                                pending: recovery,
                            })
                        }
                    >
                        继续上次申请
                    </Button>
                </div>
            )}
            {(error || query.isError) && (
                <div
                    role="alert"
                    className="mb-4 flex flex-wrap items-center gap-3 text-sm text-destructive"
                >
                    <p>
                        {error ||
                            getErrorMessage(query.error, "合同资料加载失败")}
                    </p>
                    {query.isError && (
                        <Button
                            id="contract-templates-retry"
                            variant="outline"
                            onClick={() => {
                                void query.refetch()
                            }}
                        >
                            重新加载
                        </Button>
                    )}
                </div>
            )}
            {!(canList || canManage || canHistory) && !account.isPending ? (
                <p role="status" className="text-sm text-muted-foreground">
                    当前账号没有合同模板或申请记录的查看权限。
                </p>
            ) : (
                <ListWorkSurface
                    ariaLabel={
                        view === "templates" ? "合同模板目录" : "我的合同申请"
                    }
                    views={
                        <ListWorkspaceViews
                            ariaLabel="合同模板与申请视图"
                            items={[
                                ...(canList || canManage
                                    ? [
                                          {
                                              id: "contract-templates-view",
                                              label: "合同模板",
                                              active: view === "templates",
                                              onClick: () => {
                                                  setView("templates")
                                                  setPagination((current) => ({
                                                      ...current,
                                                      pageIndex: 0,
                                                  }))
                                              },
                                          },
                                      ]
                                    : []),
                                ...(canHistory
                                    ? [
                                          {
                                              id: "contract-applications-view",
                                              label: "我的合同申请",
                                              active: view === "applications",
                                              onClick: () => {
                                                  setView("applications")
                                                  setPagination((current) => ({
                                                      ...current,
                                                      pageIndex: 0,
                                                  }))
                                              },
                                          },
                                      ]
                                    : []),
                            ]}
                        />
                    }
                    toolbar={
                        <p className="text-xs leading-relaxed text-muted-foreground">
                            {view === "templates"
                                ? "选择与签约公司一致的模板；编号自动填写在首页右上角，同一编号组共用年度流水。"
                                : "重复下载保留原编号，下载后用 Word 打印；签署后请到合同档案上传签署版 PDF。"}
                        </p>
                    }
                    table={
                        view === "templates" ? (
                            <DataTable
                                emptyState={
                                    <BusinessEmptyState
                                        kind="no-data"
                                        title={
                                            canManage
                                                ? "暂无合同模板"
                                                : "暂无启用的合同模板"
                                        }
                                        description={
                                            canManage
                                                ? "请上传签约公司对应的 Word 模板，供销售申请编号并下载。"
                                                : "请联系管理员上传或启用签约公司对应的 Word 模板。"
                                        }
                                        className={
                                            listWorkspaceEmptyStateClassName
                                        }
                                    />
                                }
                                key="templates"
                                id="contract-templates-table"
                                data={templates.data?.items ?? []}
                                columns={templateColumns}
                                getRowId={(row) => row.id}
                                rowCount={templates.data?.total ?? 0}
                                pagination={pagination}
                                onPaginationChange={setPagination}
                                loading={
                                    templates.isFetching || account.isPending
                                }
                            />
                        ) : (
                            <DataTable
                                emptyState={
                                    <BusinessEmptyState
                                        kind="no-data"
                                        title="暂无合同申请"
                                        description={
                                            canList || canManage
                                                ? "在合同模板中选择签约公司对应的模板，申请编号后即可在此下载。"
                                                : "当前账号尚无合同申请记录。"
                                        }
                                        className={
                                            listWorkspaceEmptyStateClassName
                                        }
                                    />
                                }
                                key="applications"
                                id="contract-applications-table"
                                data={applications.data?.items ?? []}
                                columns={applicationColumns}
                                getRowId={(row) => row.id}
                                rowCount={applications.data?.total ?? 0}
                                pagination={pagination}
                                onPaginationChange={setPagination}
                                loading={applications.isFetching}
                            />
                        )
                    }
                />
            )}
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
                    onClose={() => {
                        setSelected(null)
                        setRecovery(readPendingContract(accountId))
                    }}
                />
            )}
        </PageScaffold>
    )
}
