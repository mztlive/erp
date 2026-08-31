"use client"

import * as React from "react"
import { DownloadIcon, FileUpIcon, LoaderCircleIcon } from "lucide-react"

import {
    DataFreshness,
    PageActions,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { ContractListResults } from "@/features/contracts/components/contract-list-results"
import { ContractPaperDialog } from "@/features/contracts/components/contract-paper-dialog"
import { ContractPreviewSheet } from "@/features/contracts/components/contract-preview-sheet"
import { ContractUploadDialog } from "@/features/contracts/components/contract-upload-dialog"
import { ContractsTablePanel } from "@/features/contracts/components/contracts-table-panel"
import { useContractListActions } from "@/features/contracts/hooks/use-contract-list-actions"
import { useContractsList } from "@/features/contracts/hooks/use-contracts-list"
import {
    useContractCenterQuery,
    useContractsQuery,
} from "@/features/contracts/hooks/queries"
import { useContractListColumns } from "@/features/contracts/hooks/use-contract-list-columns"

export function ContractsListPage() {
    const contractsQuery = useContractsQuery()
    const list = useContractsList(contractsQuery.data)
    const { customerId } = list

    const [previewId, setPreviewId] = React.useState<string | null>(null)
    const [paperId, setPaperId] = React.useState<string | null>(null)
    const [uploadOpen, setUploadOpen] = React.useState(list.upload === "1")

    const previewRow = React.useMemo(
        () =>
            (contractsQuery.data ?? []).find(
                (item) => item.contractId === previewId,
            ) ?? null,
        [contractsQuery.data, previewId],
    )

    const previewDetailQuery = useContractCenterQuery(previewId ?? "")
    const paperDetailQuery = useContractCenterQuery(paperId ?? "")

    const actions = useContractListActions({
        filteredCount: list.filtered.length,
        filterSnapshotLabel: list.filterSnapshotLabel,
    })

    const columns = useContractListColumns()

    return (
        <PageScaffold density="compact">
            <PageHeader
                title="合同"
                metadata={
                    <DataFreshness
                        updatedAt={
                            contractsQuery.isError
                                ? "查询失败"
                                : contractsQuery.data
                                  ? new Date(
                                        contractsQuery.dataUpdatedAt,
                                    ).toLocaleTimeString("zh-CN", {
                                        hour: "2-digit",
                                        minute: "2-digit",
                                    })
                                  : "正在查询"
                        }
                        dateTime={
                            contractsQuery.data
                                ? new Date(
                                      contractsQuery.dataUpdatedAt,
                                  ).toISOString()
                                : undefined
                        }
                        state={
                            contractsQuery.isError
                                ? "failed"
                                : contractsQuery.isFetching
                                  ? "syncing"
                                  : "fresh"
                        }
                    />
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "export",
                                label: actions.exportPending
                                    ? "导出中…"
                                    : "导出",
                                icon: actions.exportPending
                                    ? LoaderCircleIcon
                                    : DownloadIcon,
                                variant: "outline",
                                disabled:
                                    list.filtered.length === 0 ||
                                    actions.exportPending,
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
                }
            />

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
