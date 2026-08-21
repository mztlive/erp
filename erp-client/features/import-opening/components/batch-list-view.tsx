"use client"

import { SearchIcon, ShieldAlertIcon } from "lucide-react"

import {
    BusinessFailureState,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    ListToolbar,
    MetricItem,
    MetricStrip,
    OptionCombobox,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useImportBatchListQuery } from "@/features/import-opening/hooks/queries"
import { useBatchListColumns } from "@/features/import-opening/hooks/use-batch-list-columns"
import { useBatchPagination } from "@/features/import-opening/hooks/use-batch-pagination"
import { useBatchSearch } from "@/features/import-opening/hooks/use-batch-search"
import type { ImportOpeningUrlState } from "@/features/import-opening/lib/url-state"
import type {
    ImportBatchStatus,
    ImportEnvironment,
    ImportObjectCode,
} from "@/features/import-opening/types"
import {
    BATCH_STATUS_LABEL,
    ENVIRONMENT_LABEL,
    OBJECT_CODE_LABEL,
} from "@/features/import-opening/types"
import { formatDateTime } from "@/lib/datetime"

export function BatchListView({
    urlState,
    patchUrl,
}: {
    urlState: ImportOpeningUrlState
    patchUrl: (patch: Partial<ImportOpeningUrlState>) => void
}) {
    const { qDraft, setQDraft, searchInputRef } = useBatchSearch({
        q: urlState.q,
        patchUrl,
    })
    const listQuery = useImportBatchListQuery({
        environment: urlState.environment,
        status: urlState.status,
        objectType: urlState.objectType ?? "all",
        q: urlState.q,
        page: urlState.page,
        pageSize: 20,
    })

    const columns = useBatchListColumns({
        onOpenBatch: (batchId) =>
            patchUrl({ batchId, section: "overview", page: 1 }),
    })

    const data = listQuery.data
    const { pagination, setPagination } = useBatchPagination(urlState.page)

    const hasListFilters = Boolean(
        urlState.q || urlState.status || urlState.objectType,
    )

    const clearListFilters = () => {
        setQDraft("")
        patchUrl({
            q: undefined,
            status: undefined,
            objectType: undefined,
            page: 1,
        })
    }

    return (
        <PageScaffold>
            <PageHeader
                title="导入与期初"
                metadata={
                    <DataFreshness
                        updatedAt={
                            data?.queriedAt
                                ? formatDateTime(
                                      data.queriedAt,
                                      "dateStyle",
                                      "passthrough",
                                  )
                                : "刚刚"
                        }
                        dateTime={data?.queriedAt}
                        state={listQuery.isFetching ? "stale" : "fresh"}
                        label="导入批次"
                    />
                }
            />

            <div className="flex flex-wrap items-center gap-3">
                <Label className="text-sm text-muted-foreground">环境</Label>
                <Tabs
                    value={urlState.environment}
                    onValueChange={(v) => {
                        if (v == null) return
                        patchUrl({
                            environment: v as ImportEnvironment,
                            page: 1,
                            batchId: undefined,
                        })
                    }}
                >
                    <TabsList>
                        <TabsTrigger value="VALIDATION">验证环境</TabsTrigger>
                        <TabsTrigger value="PRODUCTION">生产环境</TabsTrigger>
                    </TabsList>
                </Tabs>
                {urlState.environment === "PRODUCTION" ? (
                    <Badge variant="destructive">
                        生产环境 · 操作需显著确认
                    </Badge>
                ) : (
                    <Badge variant="secondary">验证环境</Badge>
                )}
            </div>

            <MetricStrip columns={4} aria-label="导入批次指标">
                <MetricItem
                    label="待校验"
                    value={data?.metrics.pendingValidate ?? "—"}
                />
                <MetricItem
                    label="待业务确认"
                    value={data?.metrics.pendingConfirm ?? "—"}
                />
                <MetricItem
                    label="执行中"
                    value={data?.metrics.applying ?? "—"}
                />
                <MetricItem
                    label="失败/部分失败"
                    value={data?.metrics.failedOrPartial ?? "—"}
                />
            </MetricStrip>

            <Alert>
                <ShieldAlertIcon />
                <AlertTitle>安全边界</AlertTitle>
                <AlertDescription>
                    本页不展示原始
                    SQL、数据库连接头、禁止字段或存储对象键。不合规导出只能在受控临时区清洗后，以白名单合规包进入安全接收。
                </AlertDescription>
            </Alert>

            <BusinessTableFrame
                title="导入批次"
                description={
                    listQuery.isError
                        ? "列表加载失败，可调整筛选后重试"
                        : `${ENVIRONMENT_LABEL[urlState.environment]} · 共 ${data?.totalCount ?? 0} 批`
                }
                toolbar={
                    <ListToolbar
                        search={
                            <form
                                className="flex gap-1"
                                onSubmit={(e) => {
                                    e.preventDefault()
                                    patchUrl({
                                        q: qDraft.trim() || undefined,
                                        page: 1,
                                    })
                                }}
                            >
                                <InputGroup>
                                    <InputGroupAddon>
                                        <SearchIcon aria-hidden="true" />
                                    </InputGroupAddon>
                                    <InputGroupInput
                                        ref={searchInputRef}
                                        value={qDraft}
                                        onChange={(e) =>
                                            setQDraft(e.target.value)
                                        }
                                        placeholder="批次号（精确/前缀匹配）"
                                        aria-label="搜索批次"
                                    />
                                </InputGroup>
                            </form>
                        }
                        filters={
                            <>
                                <OptionCombobox
                                    value={urlState.objectType ?? "all"}
                                    onValueChange={(v) => {
                                        if (v == null) return
                                        patchUrl({
                                            objectType:
                                                v === "all"
                                                    ? undefined
                                                    : (v as ImportObjectCode),
                                            page: 1,
                                        })
                                    }}
                                    options={[
                                        { value: "all", label: "全部对象" },
                                        ...(
                                            Object.keys(
                                                OBJECT_CODE_LABEL,
                                            ) as ImportObjectCode[]
                                        ).map((code) => ({
                                            value: code,
                                            label: OBJECT_CODE_LABEL[code],
                                        })),
                                    ]}
                                    inputClassName="w-[10rem]"
                                    size="sm"
                                    placeholder="对象：全部"
                                    aria-label="对象集合"
                                    allowClear={false}
                                />
                                <OptionCombobox
                                    value={urlState.status ?? "all"}
                                    onValueChange={(v) => {
                                        if (v == null) return
                                        patchUrl({
                                            status: v === "all" ? undefined : v,
                                            page: 1,
                                        })
                                    }}
                                    options={[
                                        { value: "all", label: "全部状态" },
                                        ...(
                                            Object.keys(
                                                BATCH_STATUS_LABEL,
                                            ) as ImportBatchStatus[]
                                        ).map((s) => ({
                                            value: s,
                                            label: BATCH_STATUS_LABEL[s],
                                        })),
                                    ]}
                                    inputClassName="w-[11rem]"
                                    size="sm"
                                    placeholder="状态：全部"
                                    aria-label="批次状态"
                                    allowClear={false}
                                />
                            </>
                        }
                        actions={
                            <>
                                <span
                                    className="text-xs text-muted-foreground"
                                    aria-live="polite"
                                >
                                    共{" "}
                                    {(data?.totalCount ?? 0).toLocaleString(
                                        "zh-CN",
                                    )}{" "}
                                    批
                                </span>
                                {hasListFilters ? (
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="ghost"
                                        onClick={clearListFilters}
                                    >
                                        清除筛选
                                    </Button>
                                ) : null}
                            </>
                        }
                    />
                }
                table={
                    listQuery.isError ? (
                        <BusinessFailureState
                            title="批次列表加载失败"
                            error={listQuery.error}
                            className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                            action={
                                <Button
                                    type="button"
                                    variant="secondary"
                                    className="rounded-lg shadow-none"
                                    onClick={() => void listQuery.refetch()}
                                >
                                    重试
                                </Button>
                            }
                        />
                    ) : (
                        <DataTable
                            data={[...(data?.rows ?? [])]}
                            columns={columns}
                            getRowId={(row) => row.batchId}
                            rowCount={data?.totalCount ?? 0}
                            pagination={pagination}
                            onPaginationChange={(next) => {
                                setPagination(next)
                                patchUrl({ page: next.pageIndex + 1 })
                            }}
                            layout="flush"
                            loading={listQuery.isPending}
                        />
                    )
                }
            />
        </PageScaffold>
    )
}
