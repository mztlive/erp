"use client"

import * as React from "react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { DownloadIcon, FileUpIcon, HistoryIcon } from "lucide-react"

import { BusinessFailureState } from "@/components/business"
import { ListWorkspacePage } from "@/components/business/list-workspace"
import { ContractListResults } from "@/features/contracts/components/contract-list-results"
import { ContractPaperDialog } from "@/features/contracts/components/contract-paper-dialog"
import { ContractPreviewSheet } from "@/features/contracts/components/contract-preview-sheet"
import { ContractUploadDialog } from "@/features/contracts/components/contract-upload-dialog"
import { ContractsTablePanel } from "@/features/contracts/components/contracts-table-panel"
import { useContractListActions } from "@/features/contracts/hooks/use-contract-list-actions"
import { useContractListColumns } from "@/features/contracts/hooks/use-contract-list-columns"
import { useContractsList } from "@/features/contracts/hooks/use-contracts-list"
import { useContractCenterQuery } from "@/features/contracts/hooks/queries"
import { useContractFilePicker } from "@/features/contracts/hooks/use-contract-file-picker"

export function ContractsListPage() {
    const list = useContractsList()
    const account = useAccountProfileQuery()
    const canReadDetail = hasPermission(
        account.data?.permissions,
        "contract:detail",
    )
    const canImport = hasPermission(
        account.data?.permissions,
        "contract:create",
    )
    const { contractsQuery, customerId } = list

    const lastFocusedRowId = React.useRef<string | null>(null)
    const [previewId, setPreviewId] = React.useState<string | null>(null)
    const [paperId, setPaperId] = React.useState<string | null>(null)
    const [uploadOpen, setUploadOpen] = React.useState(list.upload === "1")
    const [uploadFile, setUploadFile] = React.useState<File | null>(null)
    const picker = useContractFilePicker((file) => {
        setUploadFile(file)
        setUploadOpen(true)
    })

    const previewRow =
        list.pageRows.find((item) => item.contractId === previewId) ?? null
    const previewDetailQuery = useContractCenterQuery(
        list.canReadList && canReadDetail ? (previewId ?? "") : "",
    )
    const paperDetailQuery = useContractCenterQuery(
        list.canReadList && canReadDetail ? (paperId ?? "") : "",
    )

    const actions = useContractListActions({
        query: list.url,
        filteredCount: list.total,
        filterSnapshotLabel: list.filterSnapshotLabel,
    })
    const columns = useContractListColumns(canReadDetail)

    return (
        <>
            {canImport && (
                <input
                    id="contracts-list-upload-input"
                    type="file"
                    {...picker.inputProps}
                />
            )}
            <ListWorkspacePage
                title="合同"
                loading={
                    account.isPending ||
                    (list.canReadList &&
                        contractsQuery.isPending &&
                        contractsQuery.isFetching)
                }
                actions={[
                    {
                        hidden: !list.canReadList,
                        actionKey: "export",
                        label: actions.exportPending ? "导出中…" : "导出",
                        icon: DownloadIcon,
                        tier: "quiet",
                        loading: actions.exportPending,
                        disabled:
                            list.total === 0 ||
                            contractsQuery.isError ||
                            actions.exportPending,
                        title: contractsQuery.isError
                            ? "请重新查询后导出"
                            : list.total === 0
                              ? "当前没有可导出的合同"
                              : undefined,
                        onClick: () => {
                            void actions.handleExport()
                        },
                    },
                    {
                        hidden: !canImport,
                        actionKey: "import-history",
                        label: "导入记录",
                        icon: HistoryIcon,
                        tier: "quiet",
                        onClick: () => setUploadOpen(true),
                    },
                    {
                        hidden: !canImport,
                        actionKey: "upload",
                        label: "上传合同 PDF",
                        icon: FileUpIcon,
                        tier: "primary",
                        onClick: picker.selectFile,
                    },
                ]}
                alerts={
                    list.canReadList &&
                    (actions.actionResult || actions.exportJob) ? (
                        <ContractListResults
                            actionResult={actions.actionResult}
                            exportJob={actions.exportJob}
                        />
                    ) : null
                }
            >
                {account.isError && !account.data ? (
                    <BusinessFailureState
                        id="card-contracts-list-failure"
                        title="无法读取账号权限"
                        error={account.error}
                        onRetry={() => void account.refetch()}
                    />
                ) : (
                    <ContractsTablePanel
                        list={list}
                        columns={columns}
                        canReadDetail={canReadDetail}
                        onOpenUpload={canImport ? picker.selectFile : undefined}
                        onPreview={(contractId) => {
                            lastFocusedRowId.current = contractId
                            setPreviewId(contractId)
                        }}
                        highlightedContractId={
                            previewId ??
                            actions.highlightedContractId ??
                            undefined
                        }
                    />
                )}
            </ListWorkspacePage>

            <ContractPreviewSheet
                row={list.canReadList ? previewRow : null}
                detail={canReadDetail ? previewDetailQuery.data : undefined}
                detailLoading={canReadDetail && previewDetailQuery.isPending}
                onOpenChange={(open) => {
                    if (!open) setPreviewId(null)
                }}
                onShowPaper={setPaperId}
                finalFocus={() => {
                    const id = lastFocusedRowId.current
                    const row = id
                        ? document.querySelector<HTMLElement>(
                              `#card-contracts-list-table [data-row-id="${CSS.escape(id)}"]`,
                          )
                        : null
                    return row ?? list.searchInputRef.current
                }}
            />

            {list.canReadList && canReadDetail && (
                <ContractPaperDialog
                    contract={paperDetailQuery.data ?? null}
                    open={paperId != null && paperDetailQuery.data != null}
                    onOpenChange={(open) => {
                        if (!open) setPaperId(null)
                    }}
                />
            )}

            {canImport && (
                <ContractUploadDialog
                    open={uploadOpen}
                    initialFile={uploadFile}
                    onOpenChange={setUploadOpen}
                    initialCustomerId={customerId ?? ""}
                    onSuccess={actions.handleUploadSuccess}
                />
            )}
        </>
    )
}
