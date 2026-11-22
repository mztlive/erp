"use client"

import {
    DetailWorkspaceSection,
    ImportIssueTable,
    OptionCombobox,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { useImportIssuesQuery } from "@/features/import-opening/hooks/queries"
import type { ImportOpeningUrlState } from "@/features/import-opening/lib/url-state"
import type {
    ImportBatchView,
    ImportIssueCode,
    ImportObjectCode,
    IssueRowStatus,
} from "@/features/import-opening/types"
import {
    ISSUE_CODE_LABEL,
    OBJECT_CODE_LABEL,
    ROW_STATUS_LABEL,
} from "@/features/import-opening/types"

export function TrialSection({
    batch,
    urlState,
    patchUrl,
    issueQuery,
}: {
    batch: ImportBatchView
    urlState: ImportOpeningUrlState
    patchUrl: (patch: Partial<ImportOpeningUrlState>) => void
    issueQuery: ReturnType<typeof useImportIssuesQuery>
}) {
    const issues = issueQuery.data?.rows ?? []
    const totalCount = issueQuery.data?.totalCount ?? 0
    const totalPages = Math.max(1, Math.ceil(totalCount / 20))

    return (
        <DetailWorkspaceSection
            title="预检问题明细"
            subtitle="仅列出校验失败、冲突、跳过及待匹配的数据行，正常通过的数据不在此处展示。"
        >
            <div className="flex flex-wrap items-end gap-2">
                <div className="space-y-1">
                    <Label
                        className="text-xs"
                        htmlFor="operations-import-batch-detail-trial-filter-issue-code"
                    >
                        错误码
                    </Label>
                    <OptionCombobox
                        id="operations-import-batch-detail-trial-filter-issue-code"
                        value={urlState.issueCode ?? "all"}
                        onValueChange={(v) => {
                            if (v == null) return
                            patchUrl({
                                issueCode:
                                    v === "all"
                                        ? undefined
                                        : (v as ImportIssueCode),
                                section: "trial",
                                page: 1,
                            })
                        }}
                        options={[
                            { value: "all", label: "全部错误码" },
                            ...(
                                Object.keys(
                                    ISSUE_CODE_LABEL,
                                ) as ImportIssueCode[]
                            ).map((code) => ({
                                value: code,
                                label: ISSUE_CODE_LABEL[code],
                            })),
                        ]}
                        className="w-[12rem]"
                        size="sm"
                        allowClear={false}
                    />
                </div>
                <div className="space-y-1">
                    <Label
                        className="text-xs"
                        htmlFor="operations-import-batch-detail-trial-filter-object-type"
                    >
                        数据类型
                    </Label>
                    <OptionCombobox
                        id="operations-import-batch-detail-trial-filter-object-type"
                        value={urlState.issueObjectType ?? "all"}
                        onValueChange={(v) => {
                            if (v == null) return
                            patchUrl({
                                issueObjectType:
                                    v === "all"
                                        ? undefined
                                        : (v as ImportObjectCode),
                                section: "trial",
                                page: 1,
                            })
                        }}
                        options={[
                            { value: "all", label: "全部类型" },
                            ...batch.sourceObjectSet.map((code) => ({
                                value: code,
                                label: OBJECT_CODE_LABEL[code],
                            })),
                        ]}
                        className="w-[10rem]"
                        size="sm"
                        allowClear={false}
                    />
                </div>
                <div className="space-y-1">
                    <Label
                        className="text-xs"
                        htmlFor="operations-import-batch-detail-trial-filter-row-status"
                    >
                        处理状态
                    </Label>
                    <OptionCombobox
                        id="operations-import-batch-detail-trial-filter-row-status"
                        value={urlState.rowStatus ?? "all"}
                        onValueChange={(v) => {
                            if (v == null) return
                            patchUrl({
                                rowStatus:
                                    v === "all"
                                        ? undefined
                                        : (v as IssueRowStatus),
                                section: "trial",
                                page: 1,
                            })
                        }}
                        options={[
                            { value: "all", label: "全部状态" },
                            ...(
                                Object.keys(
                                    ROW_STATUS_LABEL,
                                ) as IssueRowStatus[]
                            ).map((s) => ({
                                value: s,
                                label: ROW_STATUS_LABEL[s],
                            })),
                        ]}
                        className="w-[10rem]"
                        size="sm"
                        allowClear={false}
                    />
                </div>
                <Button
                    id="operations-import-batch-detail-trial-clear-filters"
                    type="button"
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                        patchUrl({
                            issueCode: undefined,
                            issueObjectType: undefined,
                            rowStatus: undefined,
                            section: "trial",
                            page: 1,
                        })
                    }
                >
                    清除筛选
                </Button>
            </div>

            <ImportIssueTable
                caption="导入问题明细（不含成功行）"
                emptyMessage={
                    issueQuery.isPending
                        ? "问题明细加载中…"
                        : "当前筛选下没有问题行"
                }
                repairableLabel="可在修复后重试"
                externalLabel="需外部处理后再试"
                issues={issues.map((issue) => ({
                    id: issue.issueId,
                    rowNumber: issue.sourceRowNo,
                    field: `${OBJECT_CODE_LABEL[issue.objectType]} · ${issue.sourceColumnName}`,
                    errorCode: issue.issueCode,
                    message: (
                        <span>
                            <span className="text-muted-foreground">
                                [{ROW_STATUS_LABEL[issue.rowStatus]}]{" "}
                            </span>
                            {issue.errorDetail}
                        </span>
                    ),
                    repairable: issue.repairable,
                }))}
            />
            <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                <p className="num">共 {totalCount} 条问题</p>
                <div className="flex items-center gap-2">
                    <Button
                        id="operations-import-batch-detail-trial-prev-page"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={urlState.page <= 1 || issueQuery.isFetching}
                        onClick={() =>
                            patchUrl({
                                page: urlState.page - 1,
                                section: "trial",
                            })
                        }
                    >
                        上一页
                    </Button>
                    <span className="num">
                        第 {urlState.page} / {totalPages} 页
                    </span>
                    <Button
                        id="operations-import-batch-detail-trial-next-page"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={
                            issueQuery.isFetching ||
                            urlState.page * 20 >= totalCount
                        }
                        onClick={() =>
                            patchUrl({
                                page: urlState.page + 1,
                                section: "trial",
                            })
                        }
                    >
                        下一页
                    </Button>
                </div>
            </div>
        </DetailWorkspaceSection>
    )
}
