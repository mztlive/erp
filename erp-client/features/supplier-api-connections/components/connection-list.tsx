"use client"

import * as React from "react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import { PlusIcon, RefreshCwIcon, SearchIcon } from "lucide-react"
import { z } from "zod"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    FormalActionResult,
    GuardedBusinessAction,
    ListToolbar,
    MetricFilterItem,
    MetricStrip,
    OptionCombobox,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import type { ResultState } from "@/components/business/feedback"
import { toFieldErrors, useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import {
    newIdempotencyKey,
    outcomeToResult,
} from "@/features/supplier-api-connections/operations"
import {
    useConnectionListQuery,
    useCreateConnectionMutation,
} from "@/features/supplier-api-connections/queries"
import type { ConnectionListItem } from "@/features/supplier-api-connections/types"
import { CAPABILITY_LABEL } from "@/features/supplier-api-connections/types"
import type { ConnectionsUrlState } from "@/features/supplier-api-connections/url-state"
import { formatDateTime } from "@/lib/datetime"
import { freshnessText } from "@/lib/ui-text"

function ConnectionList({
    urlState,
    patchUrl,
    onOpen,
}: {
    urlState: ConnectionsUrlState
    patchUrl: (patch: Partial<ConnectionsUrlState>) => void
    onOpen: (connectionId: string) => void
}) {
    const [searchDraft, setSearchDraft] = React.useState(urlState.q ?? "")
    const [createOpen, setCreateOpen] = React.useState(false)
    const [result, setResult] = React.useState<
        (ResultState & { actions?: React.ReactNode }) | null
    >(null)
    const createMutation = useCreateConnectionMutation()

    React.useEffect(() => {
        setSearchDraft(urlState.q ?? "")
    }, [urlState.q])

    const listQuery = useConnectionListQuery({
        environment: urlState.environment,
        status: urlState.status,
        health: urlState.health,
        capability: urlState.capability,
        catalogFreshness: urlState.catalogFreshness,
        supplierId: urlState.supplierId,
        q: urlState.q,
        page: urlState.page,
        pageSize: urlState.pageSize,
    })

    const data = listQuery.data

    // D7：常驻/空态清除 = 清全部筛选参数并回第 1 页；environment 属视图类参数按 P4 保留，
    // 语义通过按钮 title/aria 说明。status/health/catalogFreshness 为逗号分隔多值串
    // （codec array 语义自洽），保持不变。
    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        patchUrl({
            q: undefined,
            status: undefined,
            health: undefined,
            catalogFreshness: undefined,
            capability: undefined,
            supplierId: undefined,
            page: 1,
        })
    }, [patchUrl])

    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: Math.max(0, urlState.page - 1),
        pageSize: urlState.pageSize,
    })

    React.useEffect(() => {
        setPagination((p) => ({
            ...p,
            pageIndex: Math.max(0, urlState.page - 1),
            pageSize: urlState.pageSize,
        }))
    }, [urlState.page, urlState.pageSize])

    const columns = React.useMemo<ColumnDef<ConnectionListItem>[]>(
        () => [
            {
                id: "identity",
                accessorFn: (row) => row.connectionCode,
                header: "连接身份",
                meta: { label: "连接身份", width: "reference" },
                cell: ({ row }) => {
                    const r = row.original
                    return (
                        <div className="min-w-0 py-0.5">
                            <Button
                                type="button"
                                variant="link"
                                size="xs"
                                className="num h-auto justify-start px-0 font-medium"
                                aria-label={`打开连接 ${r.connectionCode}`}
                                onClick={() => onOpen(r.connectionId)}
                            >
                                {r.connectionCode}
                            </Button>
                            <div className="truncate text-xs text-muted-foreground">
                                {r.supplier.name}
                            </div>
                        </div>
                    )
                },
            },
            {
                id: "environment",
                accessorFn: (row) => row.environmentLabel,
                header: "环境",
                meta: { label: "环境", width: "status" },
                cell: ({ row }) => {
                    const env = row.original.environment
                    const isProd = env === "PRODUCTION"
                    return (
                        <span
                            className={
                                isProd
                                    ? "text-sm font-medium text-destructive"
                                    : "text-sm text-muted-foreground"
                            }
                            aria-label={`环境：${row.original.environmentLabel}${
                                isProd ? "（生产环境）" : ""
                            }`}
                        >
                            {row.original.environmentLabel}
                            {isProd ? (
                                <span className="sr-only">生产环境</span>
                            ) : null}
                        </span>
                    )
                },
            },
            {
                id: "status",
                accessorFn: (row) => row.statusLabel,
                header: "状态",
                meta: { label: "状态", width: "status" },
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                    />
                ),
            },
            {
                id: "capabilities",
                accessorFn: (row) => row.capabilitySummary,
                header: "能力摘要",
                meta: { label: "能力摘要" },
                cell: ({ row }) => (
                    <div className="max-w-[14rem]">
                        <div className="truncate text-sm">
                            {row.original.capabilitySummary}
                        </div>
                        <div className="text-tiny text-muted-foreground">
                            连接级 · 非商品级
                        </div>
                    </div>
                ),
            },
            {
                id: "health",
                accessorFn: (row) => row.healthLabel,
                header: "健康",
                meta: { label: "健康", width: "status" },
                cell: ({ row }) => (
                    <div className="space-y-0.5">
                        <BusinessStatusBadge
                            context="list"
                            label={row.original.healthLabel}
                            tone={row.original.healthTone}
                        />
                        <div className="text-tiny text-muted-foreground">
                            {formatDateTime(
                                row.original.lastHealthAt,
                                "default",
                            )}
                        </div>
                    </div>
                ),
            },
            {
                id: "catalog",
                accessorFn: (row) => row.catalogLabel,
                header: freshnessText.catalogSyncAt,
                meta: { label: freshnessText.catalogSyncAt },
                cell: ({ row }) => (
                    <span className="text-sm">{row.original.catalogLabel}</span>
                ),
            },
            {
                id: "nextStep",
                accessorFn: (row) => row.nextStep,
                header: "下一步",
                meta: { label: "下一步" },
                cell: ({ row }) => (
                    <span className="line-clamp-2 text-sm text-muted-foreground">
                        {row.original.nextStep}
                    </span>
                ),
            },
            {
                id: "owners",
                accessorFn: (row) =>
                    `${row.businessOwner ?? "—"} / ${row.technicalOwner ?? "—"}`,
                header: "业务/技术",
                meta: { label: "业务/技术负责人" },
                cell: ({ row }) => (
                    <span className="text-xs text-muted-foreground">
                        {row.original.businessOwner ?? "—"} /{" "}
                        {row.original.technicalOwner ?? "—"}
                    </span>
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "status" },
                enableSorting: false,
                cell: ({ row }) => (
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => onOpen(row.original.connectionId)}
                    >
                        打开
                    </Button>
                ),
            },
        ],
        [onOpen],
    )

    const createSchema = z.object({
        connectionCode: z.string().trim().min(3, "请填写连接代码"),
        supplierId: z.string().trim().min(1, "请选择供应商"),
        supplierName: z.string().trim().min(2, "请选择供应商"),
        environment: z.enum(["DEVELOPMENT", "STAGING", "PRODUCTION"]),
    })

    const form = useAppForm({
        defaultValues: {
            connectionCode: "",
            supplierId: "",
            supplierName: "",
            environment: "PRODUCTION" as
                | "DEVELOPMENT"
                | "STAGING"
                | "PRODUCTION",
        },
        validators: { onChange: createSchema },
        onSubmit: async ({ value }) => {
            const outcome = await createMutation.mutateAsync({
                connectionCode: value.connectionCode,
                supplierId: value.supplierId,
                supplierName: value.supplierName,
                environment: value.environment,
                idempotencyKey: newIdempotencyKey("create"),
            })
            const mapped = outcomeToResult(outcome)
            if (outcome.status === "succeeded" && outcome.connectionId) {
                setCreateOpen(false)
                form.reset()
                setResult(
                    mapped
                        ? {
                              ...mapped,
                              actions: (
                                  <Button
                                      type="button"
                                      size="sm"
                                      onClick={() =>
                                          onOpen(outcome.connectionId!)
                                      }
                                  >
                                      打开连接详情
                                  </Button>
                              ),
                          }
                        : mapped,
                )
            } else {
                setResult(mapped)
            }
        },
    })

    if (listQuery.isPending) {
        return (
            <PageScaffold density="compact">
                <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
                <div className="h-16 animate-pulse rounded-lg bg-muted" />
                <div className="h-72 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (listQuery.isError) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="API 供应商连接" description="加载失败" />
                <BusinessFailureState
                    title="连接列表加载失败"
                    error={listQuery.error}
                    action={
                        <Button
                            type="button"
                            onClick={() => void listQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const empty = data?.emptyReason

    return (
        <PageScaffold density="compact">
            <PageHeader
                title="API 供应商连接"
                breadcrumbs={[
                    {
                        id: "api",
                        label: "供应商 API",
                        href: "/supplier-api/connections",
                    },
                    { id: "conn", label: "API 连接", current: true },
                ]}
                metadata={
                    <DataFreshness
                        updatedAt={
                            data?.projectedAt
                                ? formatDateTime(data.projectedAt, "default")
                                : "—"
                        }
                        dateTime={data?.projectedAt}
                        state={listQuery.isFetching ? "syncing" : "fresh"}
                        label="连接列表"
                    />
                }
                actions={
                    <div className="flex flex-wrap items-center gap-2">
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            className="text-muted-foreground hover:text-foreground"
                            onClick={() => void listQuery.refetch()}
                        >
                            <RefreshCwIcon
                                className="size-3.5"
                                aria-hidden="true"
                            />
                            刷新
                        </Button>
                        <div className="max-sm:hidden">
                            <GuardedBusinessAction
                                type="button"
                                size="sm"
                                disabled={!data?.hasModulePermission}
                                reason={
                                    data?.hasModulePermission
                                        ? undefined
                                        : "当前账号无模块权限"
                                }
                                onClick={() => setCreateOpen(true)}
                            >
                                <PlusIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                新建连接
                            </GuardedBusinessAction>
                        </div>
                    </div>
                }
            />

            {result ? (
                <FormalActionResult
                    status={
                        result.status === "failed"
                            ? "rejected"
                            : result.status === "processing"
                              ? "processing"
                              : result.status
                    }
                    title={result.title}
                    description={result.description}
                    reference={result.reference}
                    facts={result.facts}
                    actions={result.actions}
                />
            ) : null}

            {/* D7：空态不再隐藏筛选区——MetricStrip 与 ListToolbar 常驻，仅表格区切换空态 */}
            <MetricStrip columns={5} aria-label="连接指标筛选">
                <MetricFilterItem
                    label="已启用"
                    value={data?.metrics.enabled ?? 0}
                    active={urlState.status === "ENABLED"}
                    onClick={() =>
                        patchUrl({
                            status:
                                urlState.status === "ENABLED"
                                    ? undefined
                                    : "ENABLED",
                            page: 1,
                        })
                    }
                />
                <MetricFilterItem
                    label="故障"
                    value={data?.metrics.faulted ?? 0}
                    active={urlState.status === "FAULTED"}
                    onClick={() =>
                        patchUrl({
                            status:
                                urlState.status === "FAULTED"
                                    ? undefined
                                    : "FAULTED",
                            page: 1,
                        })
                    }
                />
                <MetricFilterItem
                    label="待配置"
                    value={data?.metrics.pendingConfig ?? 0}
                    active={urlState.status === "PENDING_CONFIG"}
                    onClick={() =>
                        patchUrl({
                            status:
                                urlState.status === "PENDING_CONFIG"
                                    ? undefined
                                    : "PENDING_CONFIG",
                            page: 1,
                        })
                    }
                />
                <MetricFilterItem
                    label="健康异常"
                    value={data?.metrics.healthAbnormal ?? 0}
                    active={Boolean(urlState.health)}
                    onClick={() =>
                        patchUrl({
                            health: urlState.health
                                ? undefined
                                : "FAILED,AUTH_FAILED,PARTIAL,UNKNOWN",
                            page: 1,
                        })
                    }
                />
                <MetricFilterItem
                    label="目录陈旧"
                    value={data?.metrics.catalogStale ?? 0}
                    active={Boolean(urlState.catalogFreshness)}
                    onClick={() =>
                        patchUrl({
                            catalogFreshness: urlState.catalogFreshness
                                ? undefined
                                : "STALE,FAILED",
                            page: 1,
                        })
                    }
                />
            </MetricStrip>

            <BusinessTableFrame
                title="连接列表"
                description="一行展示代码、供应商、环境、状态、能力、健康与下一步；身份与操作列固定；默认仅展示生产环境连接，可在工具栏切换。"
                toolbar={
                    <ListToolbar
                        search={
                            <InputGroup className="max-w-md">
                                <InputGroupAddon>
                                    <SearchIcon
                                        className="size-4"
                                        aria-hidden="true"
                                    />
                                </InputGroupAddon>
                                <InputGroupInput
                                    placeholder="连接代码、供应商名称"
                                    value={searchDraft}
                                    onChange={(e) =>
                                        setSearchDraft(e.target.value)
                                    }
                                    onKeyDown={(e) => {
                                        if (e.key === "Enter") {
                                            patchUrl({
                                                q:
                                                    searchDraft.trim() ||
                                                    undefined,
                                                page: 1,
                                            })
                                        }
                                    }}
                                    aria-label="搜索连接"
                                />
                            </InputGroup>
                        }
                        filters={
                            <>
                                <OptionCombobox
                                    value={urlState.environment}
                                    onValueChange={(v) => {
                                        if (v == null) return
                                        patchUrl({
                                            environment:
                                                v as ConnectionsUrlState["environment"],
                                            page: 1,
                                        })
                                    }}
                                    options={[
                                        { value: "ALL", label: "全部环境" },
                                        { value: "PRODUCTION", label: "生产" },
                                        { value: "STAGING", label: "测试" },
                                        { value: "DEVELOPMENT", label: "开发" },
                                    ]}
                                    className="w-[7.5rem]"
                                    size="sm"
                                    placeholder="环境"
                                    allowClear={false}
                                    aria-label="环境"
                                />
                                <OptionCombobox
                                    value={urlState.status ?? "default"}
                                    onValueChange={(v) => {
                                        if (v == null || v === "default") {
                                            patchUrl({
                                                status: undefined,
                                                page: 1,
                                            })
                                        } else if (v === "all") {
                                            patchUrl({
                                                status: "ENABLED,DISABLED,FAULTED,PENDING_CONFIG",
                                                page: 1,
                                            })
                                        } else {
                                            patchUrl({ status: v, page: 1 })
                                        }
                                    }}
                                    options={[
                                        {
                                            value: "default",
                                            label: "启用+故障+待配置",
                                        },
                                        { value: "all", label: "全部状态" },
                                        { value: "ENABLED", label: "启用" },
                                        { value: "FAULTED", label: "故障" },
                                        { value: "DISABLED", label: "停用" },
                                        {
                                            value: "PENDING_CONFIG",
                                            label: "待配置",
                                        },
                                    ]}
                                    className="w-[8rem]"
                                    size="sm"
                                    placeholder="状态"
                                    allowClear={false}
                                    aria-label="连接状态"
                                />
                                <SupplierSearchCombobox
                                    value={urlState.supplierId || undefined}
                                    onValueChange={(id) =>
                                        patchUrl({
                                            supplierId: id || undefined,
                                            page: 1,
                                        })
                                    }
                                    purpose="filter"
                                    className="w-[12rem]"
                                    placeholder="全部供应商"
                                    aria-label="供应商"
                                />
                            </>
                        }
                        secondary={
                            <OptionCombobox
                                value={urlState.capability ?? ""}
                                onValueChange={(v) =>
                                    patchUrl({
                                        capability: v || undefined,
                                        page: 1,
                                    })
                                }
                                options={[
                                    { value: "", label: "全部能力" },
                                    ...(
                                        Object.keys(CAPABILITY_LABEL) as Array<
                                            keyof typeof CAPABILITY_LABEL
                                        >
                                    ).map((k) => ({
                                        value: k,
                                        label: CAPABILITY_LABEL[k],
                                    })),
                                ]}
                                className="w-[8rem]"
                                size="sm"
                                placeholder="能力"
                                allowClear={false}
                                aria-label="能力"
                            />
                        }
                        actions={
                            <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                onClick={clearFilters}
                                title="清除筛选，保留当前环境"
                                aria-label="清除筛选（保留当前环境）"
                            >
                                清除筛选
                            </Button>
                        }
                    />
                }
                table={
                    empty === "FILTER_NO_RESULT" ? (
                        <BusinessEmptyState
                            kind="filter"
                            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                            title="当前筛选无结果"
                            description="没有连接符合当前环境/状态/能力/健康条件，可清除筛选。"
                            action={
                                <Button
                                    type="button"
                                    variant="secondary"
                                    className="rounded-lg shadow-none"
                                    onClick={clearFilters}
                                >
                                    清除筛选
                                </Button>
                            }
                        />
                    ) : empty === "NO_CONNECTIONS" ? (
                        <BusinessEmptyState
                            kind="no-data"
                            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                            title="尚未接入供应商连接"
                            description="当前环境还没有连接身份。有权限时可新建连接。"
                            action={
                                data?.hasModulePermission ? (
                                    <Button
                                        type="button"
                                        onClick={() => setCreateOpen(true)}
                                    >
                                        新建连接
                                    </Button>
                                ) : null
                            }
                        />
                    ) : (
                        <DataTable
                            data={data?.items ?? []}
                            columns={columns}
                            getRowId={(row) => row.connectionId}
                            rowCount={data?.total ?? 0}
                            rowLabel={(row) => row.connectionCode}
                            caption="API 供应商连接列表"
                            density="compact"
                            layout="flush"
                            enableColumnPinning
                            defaultColumnVisibility={{ owners: false }}
                            defaultColumnPinning={{
                                left: ["identity"],
                                right: ["actions"],
                            }}
                            pagination={pagination}
                            onPaginationChange={(next) => {
                                setPagination(next)
                                patchUrl({
                                    page: next.pageIndex + 1,
                                    pageSize: next.pageSize,
                                })
                            }}
                            onRowOpen={(row) => onOpen(row.connectionId)}
                        />
                    )
                }
            />

            <Dialog open={createOpen} onOpenChange={setCreateOpen}>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>新建连接身份</DialogTitle>
                        <DialogDescription>
                            连接代码全局唯一，不可与环境组合复用。创建成功后可在结果中打开连接详情完成配置。
                        </DialogDescription>
                    </DialogHeader>
                    <form
                        className="flex flex-col gap-3"
                        onSubmit={(e) => {
                            e.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <form.AppField
                            name="connectionCode"
                            children={(field) => (
                                <field.TextField
                                    label="连接代码"
                                    placeholder="CONN-XXX-PROD"
                                />
                            )}
                        />
                        <form.AppField
                            name="supplierId"
                            children={(field) => {
                                const isInvalid =
                                    field.state.meta.isTouched &&
                                    !field.state.meta.isValid
                                const errors = toFieldErrors(
                                    field.state.meta.errors,
                                )
                                return (
                                    <Field
                                        data-invalid={isInvalid || undefined}
                                    >
                                        <FieldLabel htmlFor="create-supplierId">
                                            供应商
                                        </FieldLabel>
                                        <SupplierSearchCombobox
                                            value={
                                                field.state.value || undefined
                                            }
                                            onValueChange={(id) => {
                                                field.handleChange(id ?? "")
                                            }}
                                            onItemChange={(supplier) => {
                                                form.setFieldValue(
                                                    "supplierName",
                                                    supplier?.supplierName ??
                                                        "",
                                                )
                                            }}
                                            placeholder="搜索供应商名称或编码"
                                        />
                                        {isInvalid ? (
                                            <FieldError errors={errors} />
                                        ) : null}
                                    </Field>
                                )
                            }}
                        />
                        <form.AppField
                            name="environment"
                            children={(field) => (
                                <div className="space-y-1.5">
                                    <Label>环境</Label>
                                    <OptionCombobox
                                        value={field.state.value}
                                        onValueChange={(v) => {
                                            if (v)
                                                field.handleChange(
                                                    v as typeof field.state.value,
                                                )
                                        }}
                                        options={[
                                            {
                                                value: "PRODUCTION",
                                                label: "生产",
                                            },
                                            { value: "STAGING", label: "测试" },
                                            {
                                                value: "DEVELOPMENT",
                                                label: "开发",
                                            },
                                        ]}
                                        allowClear={false}
                                    />
                                    {field.state.value === "PRODUCTION" ? (
                                        <p
                                            className="text-xs text-muted-foreground"
                                            role="status"
                                        >
                                            正在创建生产环境连接身份
                                        </p>
                                    ) : null}
                                </div>
                            )}
                        />
                        <DialogFooter>
                            <Button
                                type="button"
                                variant="ghost"
                                onClick={() => setCreateOpen(false)}
                            >
                                取消
                            </Button>
                            <form.AppForm>
                                <form.SubmitButton label="创建" />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>
        </PageScaffold>
    )
}

export { ConnectionList }
