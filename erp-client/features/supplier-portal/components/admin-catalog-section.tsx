"use client"
import { useCallback, useMemo, useRef, useState } from "react"
import { z } from "zod"
import type { ColumnDef } from "@tanstack/react-table"
import { toFieldErrors, useAppForm } from "@/components/form"
import { BusinessEmptyState, DataTable } from "@/components/business"
import { ListWorkSurface } from "@/components/business/list-workspace"
import { CompanySkuSearchCombobox } from "@/features/entity-selectors"
import { Button } from "@/components/ui/button"
import { PlusIcon } from "lucide-react"
import { FieldError } from "@/components/ui/field"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    commandFailureDisposition,
    type CommandFailureDisposition,
} from "@/lib/api/command-recovery"
import { PortalAdminCommandConflict } from "@/features/supplier-portal/components/admin-command-conflict"
import { PortalError } from "@/features/supplier-portal/components/surface"
import { commandKey } from "@/features/supplier-portal/lib/presentation"
import { savePortalGrant, type PortalGrant } from "../admin-api"
import {
    usePortalGrants,
    usePortalAdminAccess,
    usePortalAdminCommand,
} from "../hooks/admin"
const schema = z.object({
    skuId: z.string().min(1, "请选择精确公司SKU"),
})
export function SupplierCatalogSection({ supplierId }: { supplierId: string }) {
    const access = usePortalAdminAccess()
    const [creating, setCreating] = useState(false)
    const [pagination, setPagination] = useState({ pageIndex: 0, pageSize: 50 })
    const query = usePortalGrants(
        {
            supplier_id: supplierId,
            page: pagination.pageIndex + 1,
            page_size: pagination.pageSize,
        },
        access.can("supplier_portal_catalog:list") && !!supplierId,
    )
    const [error, setError] = useState<unknown>(null)
    const [saved, setSaved] = useState(false)
    const [failure, setFailure] = useState<CommandFailureDisposition | null>(
        null,
    )
    const [reviewedBody, setReviewedBody] = useState<Record<
        string,
        unknown
    > | null>(null)
    const reviewedGrant = useRef<PortalGrant | null>(null)
    const mutation = usePortalAdminCommand(savePortalGrant)
    const intent = useRef<{
        body: Record<string, unknown>
        key: string
    } | null>(null)
    const change = useCallback(
        async (body: Record<string, unknown>) => {
            if (failure === "conflict") return
            setError(null)
            setSaved(false)
            if (
                intent.current &&
                JSON.stringify(intent.current.body) !== JSON.stringify(body)
            ) {
                setError(new Error("请先重试原操作并核对结果"))
                return
            }
            if (!intent.current) {
                intent.current = { body, key: commandKey("catalog-grant") }
                setReviewedBody(null)
            }
            try {
                await mutation.mutateAsync({
                    ...intent.current.body,
                    idempotency_key: intent.current.key,
                })
                intent.current = null
                setFailure(null)
                setSaved(true)
                setCreating(false)
            } catch (cause) {
                const disposition = commandFailureDisposition(cause)
                if (disposition === "rejected") intent.current = null
                setFailure(disposition)
                setError(cause)
            }
        },
        [mutation, failure],
    )
    const form = useAppForm({
        defaultValues: { skuId: "" },
        validators: { onSubmit: schema },
        onSubmit: async ({ value }) => {
            await change({
                supplier_id: supplierId,
                sku_id: value.skuId,
                active: true,
                expected_version:
                    reviewedBody?.supplier_id === supplierId &&
                    reviewedBody?.sku_id === value.skuId
                        ? reviewedBody.expected_version
                        : null,
            })
        },
    })
    const canUpdate = access.can("supplier_portal_catalog:update")
    const canAdd = canUpdate && access.can("sku:list")
    const columns = useMemo<ColumnDef<PortalGrant>[]>(
        () => [
            {
                accessorKey: "name",
                header: "商品",
                cell: ({ row }) => row.original.name ?? "已开放公司规格",
            },
            { accessorKey: "sku_no", header: "公司SKU编号" },
            { accessorKey: "specification", header: "规格" },
            {
                accessorKey: "active",
                header: "报价资格",
                cell: ({ row }) => (row.original.active ? "已开放" : "已撤销"),
            },
            ...(canUpdate
                ? [
                      {
                          id: "actions",
                          header: "操作",
                          enableHiding: false,
                          cell: ({ row }) => (
                              <Button
                                  id={`supplier-portal-grant-change-${toAutomationIdSegment(row.original.id)}`}
                                  variant="outline"
                                  size="sm"
                                  disabled={
                                      mutation.isPending || !!intent.current
                                  }
                                  onClick={() =>
                                      void change({
                                          supplier_id: row.original.supplier_id,
                                          sku_id: row.original.sku_id,
                                          active: !row.original.active,
                                          expected_version:
                                              row.original.version,
                                      })
                                  }
                              >
                                  {row.original.active
                                      ? "撤销开放"
                                      : "重新开放"}
                              </Button>
                          ),
                      } satisfies ColumnDef<PortalGrant>,
                  ]
                : []),
        ],
        [canUpdate, mutation.isPending, change],
    )
    const feedback = (
        <>
            <PortalError error={error} />
            {canUpdate && intent.current && failure === "unknown" && (
                <Button
                    id="supplier-portal-grant-retry-original"
                    type="button"
                    variant="outline"
                    disabled={mutation.isPending}
                    onClick={() => {
                        if (intent.current) void change(intent.current.body)
                    }}
                >
                    重试原目录配置
                </Button>
            )}
            {failure === "unknown" && (
                <p className="text-body-compact">
                    配置结果尚未确认，请保留原内容并重试原目录配置。
                </p>
            )}
            {canUpdate && failure === "conflict" && (
                <PortalAdminCommandConflict
                    idPrefix="supplier-portal-grant-conflict"
                    onReload={async () => {
                        const result = await query.refetch({
                            throwOnError: true,
                        })
                        const original = intent.current?.body
                        reviewedGrant.current =
                            result.data?.items.find(
                                (grant) =>
                                    grant.supplier_id ===
                                        original?.supplier_id &&
                                    grant.sku_id === original?.sku_id,
                            ) ?? null
                        if (
                            original?.expected_version != null &&
                            !reviewedGrant.current
                        ) {
                            throw new Error(
                                "原配置已不在当前目录中，请核对供应商、目录页码和查看资格后继续。",
                            )
                        }
                        return result
                    }}
                    onConfirmed={() => {
                        if (intent.current) {
                            setReviewedBody({
                                ...intent.current.body,
                                expected_version:
                                    reviewedGrant.current?.version ?? null,
                            })
                        }
                        intent.current = null
                        setFailure(null)
                        setError(null)
                    }}
                />
            )}
            {canUpdate && reviewedBody && (
                <div className="space-y-2 rounded-lg border p-4">
                    <p className="text-body-compact">
                        已核对最新目录，原填写的目标仍为
                        {reviewedBody.active
                            ? "开放此SKU报价"
                            : "撤销此SKU报价资格"}
                        。请再次确认后保存，也可调整下方选择。
                    </p>
                    <Button
                        id="supplier-portal-grant-save-reviewed"
                        type="button"
                        disabled={mutation.isPending || !!intent.current}
                        onClick={() => void change(reviewedBody)}
                    >
                        按核对后资料保存原配置
                    </Button>
                </div>
            )}
            {saved && <p role="status">报价资格配置已保存。</p>}
        </>
    )
    const locked = mutation.isPending || !!intent.current
    return (
        <section className="space-y-4" aria-labelledby="supplier-catalog-title">
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="space-y-1">
                    <h2
                        id="supplier-catalog-title"
                        className="text-section-title font-semibold tracking-tight"
                    >
                        SKU 定向开放
                    </h2>
                    <p className="text-xs text-muted-foreground">
                        选择允许当前供应商报价的公司规格，开放报价不改变商品上架状态。
                    </p>
                </div>
                {canAdd && (
                    <Button
                        id="supplier-portal-grant-create"
                        type="button"
                        disabled={locked}
                        onClick={() => {
                            form.reset()
                            setCreating(true)
                        }}
                    >
                        <PlusIcon data-icon="inline-start" aria-hidden />
                        开放 SKU
                    </Button>
                )}
            </div>
            {!creating && feedback}
            {query.isSuccess && query.data.total === 0 ? (
                <BusinessEmptyState
                    kind="no-data"
                    title="尚未开放公司 SKU"
                    description="开放后，此供应商可在门户中查看对应规格并提交报价。"
                    className="py-12"
                />
            ) : (
                <ListWorkSurface
                    ariaLabel="SKU定向开放"
                    selectionBar={
                        <span className="text-xs text-muted-foreground">
                            共 {query.data?.total ?? 0} 项配置
                        </span>
                    }
                    table={
                        <DataTable
                            id="supplier-portal-admin-grants-table"
                            data={query.data?.items ?? []}
                            columns={columns}
                            getRowId={(row) => row.id}
                            pagination={pagination}
                            onPaginationChange={setPagination}
                            rowCount={query.data?.total ?? 0}
                            loading={query.isFetching}
                            errorState={
                                query.isError ? (
                                    <PortalError
                                        error={query.error}
                                        retry={() => void query.refetch()}
                                        id="supplier-portal-admin-grants-retry"
                                    />
                                ) : undefined
                            }
                            emptyTitle="此供应商暂无定向开放配置。"
                            showPagination={
                                (query.data?.total ?? 0) >
                                    pagination.pageSize ||
                                pagination.pageIndex > 0
                            }
                        />
                    }
                />
            )}
            {canAdd && (
                <Dialog
                    open={creating}
                    onOpenChange={(open) => {
                        if (!open && !locked) setCreating(false)
                    }}
                >
                    <DialogContent
                        closeButtonId="supplier-portal-grant-create-close"
                        showCloseButton={!locked}
                        className="max-h-[85svh] overflow-y-auto sm:max-w-lg"
                    >
                        <DialogHeader>
                            <DialogTitle>开放 SKU 报价</DialogTitle>
                            <DialogDescription>
                                选择当前供应商可报价的精确公司
                                SKU，包含未上架规格。
                            </DialogDescription>
                        </DialogHeader>
                        {feedback}
                        <form
                            className="space-y-5"
                            onSubmit={(event) => {
                                event.preventDefault()
                                void form.handleSubmit()
                            }}
                        >
                            <form.AppField name="skuId">
                                {(field) => (
                                    <div className="space-y-2">
                                        <label
                                            className="text-xs"
                                            htmlFor="supplier-portal-grant-sku"
                                        >
                                            精确公司SKU
                                        </label>
                                        <CompanySkuSearchCombobox
                                            id="supplier-portal-grant-sku"
                                            label="精确公司SKU"
                                            value={
                                                field.state.value || undefined
                                            }
                                            onValueChange={(value) =>
                                                field.handleChange(value ?? "")
                                            }
                                            disabled={
                                                mutation.isPending ||
                                                !!intent.current
                                            }
                                            allowClear={false}
                                        />
                                        <FieldError
                                            errors={toFieldErrors(
                                                field.state.meta.errors,
                                            )}
                                        />
                                    </div>
                                )}
                            </form.AppField>
                            <DialogFooter>
                                <Button
                                    id="supplier-portal-grant-create-cancel"
                                    type="button"
                                    variant="outline"
                                    disabled={locked}
                                    onClick={() => setCreating(false)}
                                >
                                    取消
                                </Button>
                                <form.AppForm>
                                    <form.SubmitButton
                                        id="supplier-portal-grant-save"
                                        label={
                                            intent.current
                                                ? "重试原配置"
                                                : "开放此SKU报价"
                                        }
                                        disabled={
                                            mutation.isPending ||
                                            failure === "conflict"
                                        }
                                    />
                                </form.AppForm>
                            </DialogFooter>
                        </form>
                    </DialogContent>
                </Dialog>
            )}
        </section>
    )
}
