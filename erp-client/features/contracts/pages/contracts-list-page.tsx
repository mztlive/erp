"use client"

import * as React from "react"
import { DownloadIcon, FileUpIcon, LoaderCircleIcon } from "lucide-react"

import { PageActions, PageScaffold } from "@/components/business"
import {
    ListWorkspaceHeader,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { ContractListResults } from "@/features/contracts/components/contract-list-results"
import { ContractPaperDialog } from "@/features/contracts/components/contract-paper-dialog"
import { ContractPreviewSheet } from "@/features/contracts/components/contract-preview-sheet"
import { ContractUploadDialog } from "@/features/contracts/components/contract-upload-dialog"
import { ContractsTablePanel } from "@/features/contracts/components/contracts-table-panel"
import { useContractListActions } from "@/features/contracts/hooks/use-contract-list-actions"
import { useContractListColumns } from "@/features/contracts/hooks/use-contract-list-columns"
import { useContractsList } from "@/features/contracts/hooks/use-contracts-list"
import { useContractCenterQuery } from "@/features/contracts/hooks/queries"

export function ContractsListPage() {
    const list = useContractsList()
    const { contractsQuery } = list
    const { customerId } = list

    const [previewId, setPreviewId] = React.useState<string | null>(null)
    const [paperId, setPaperId] = React.useState<string | null>(null)
    const [uploadOpen, setUploadOpen] = React.useState(list.upload === "1")

    const previewRow = React.useMemo(
        () =>
            (contractsQuery.data?.items ?? []).find(
                (item) => item.contractId === previewId,
            ) ?? null,
        [contractsQuery.data, previewId],
    )

    const previewDetailQuery = useContractCenterQuery(previewId ?? "")
    const paperDetailQuery = useContractCenterQuery(paperId ?? "")

    const actions = useContractListActions({
        query: list.url,
        filteredCount: list.total,
        filterSnapshotLabel: list.filterSnapshotLabel,
    })

    const columns = useContractListColumns()
    const updatedAt = contractsQuery.data
        ? new Date(contractsQuery.dataUpdatedAt).toISOString()
        : undefined

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="销售"
                title="合同"
                description={
                    <>
                        查看合同文本、客户与结算主体。
                        <span className="ml-3 text-xs" role="status">
                            {contractsQuery.isError ? (
                                "查询失败"
                            ) : contractsQuery.isFetching ? (
                                "正在更新…"
                            ) : updatedAt ? (
                                <time dateTime={updatedAt}>
                                    更新于{" "}
                                    {new Date(updatedAt).toLocaleTimeString(
                                        "zh-CN",
                                        {
                                            hour: "2-digit",
                                            minute: "2-digit",
                                        },
                                    )}
                                </time>
                            ) : (
                                "正在查询"
                            )}
                        </span>
                    </>
                }
            >
                <PageActions
                    actions={[
                        {
                            actionKey: "export",
                            label: actions.exportPending ? "导出中…" : "导出",
                            icon: actions.exportPending
                                ? LoaderCircleIcon
                                : DownloadIcon,
                            variant: "outline",
                            disabled: list.total === 0 || actions.exportPending,
                            onClick: () => {
                                void actions.handleExport()
                            },
                        },
                        {
                            actionKey: "upload",
                            label: "上传合同 PDF",
                            icon: FileUpIcon,
                            onClick: () => setUploadOpen(true),
                        },
                    ]}
                />
            </ListWorkspaceHeader>

            <ContractListResults
                actionResult={actions.actionResult}
                exportJob={actions.exportJob}
            />

            <ContractsTablePanel
                list={list}
                columns={columns}
                isError={contractsQuery.isError}
                error={contractsQuery.error}
                isPending={contractsQuery.isPending}
                onRetry={() => {
                    void contractsQuery.refetch()
                }}
                onOpenUpload={() => setUploadOpen(true)}
                onPreview={setPreviewId}
                highlightedContractId={
                    actions.highlightedContractId ?? undefined
                }
            />

            <ContractPreviewSheet
                row={previewRow}
                detail={previewDetailQuery.data}
                detailLoading={previewDetailQuery.isPending}
                onOpenChange={(open) => {
                    if (!open) setPreviewId(null)
                }}
                onShowPaper={setPaperId}
            />

            <ContractPaperDialog
                contract={paperDetailQuery.data ?? null}
                open={paperId != null && paperDetailQuery.data != null}
                onOpenChange={(open) => {
                    if (!open) setPaperId(null)
                }}
            />

            <ContractUploadDialog
                open={uploadOpen}
                onOpenChange={setUploadOpen}
                initialCustomerId={customerId ?? ""}
                onSuccess={actions.handleUploadSuccess}
            />
        </PageScaffold>
    )
}
