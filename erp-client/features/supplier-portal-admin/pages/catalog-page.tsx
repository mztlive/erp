"use client"
import { useCallback, useMemo, useRef, useState } from "react"
import { z } from "zod"
import type { ColumnDef } from "@tanstack/react-table"
import { useAppForm } from "@/components/form"
import { DataTable } from "@/components/business"
import { ListWorkSurface } from "@/components/business/list-workspace"
import {
    CompanySkuSearchCombobox,
    SupplierSearchCombobox,
} from "@/features/entity-selectors"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    commandFailureDisposition,
    type CommandFailureDisposition,
} from "@/lib/api/command-recovery"
import { PortalCommandConflict } from "../components/command-conflict"
import { PortalError } from "@/features/supplier-portal/components/surface"
import { commandKey } from "@/features/supplier-portal/lib/presentation"
import { savePortalGrant, type PortalGrant } from "../api"
import {
    usePortalGrants,
    usePortalAdminAccess,
    usePortalAdminCommand,
} from "../hooks"
import { PortalAdminFrame } from "../components/admin-frame"
const schema = z.object({
    supplierId: z.string().min(1, "请选择供应商"),
    skuId: z.string().min(1, "请选择精确公司SKU"),
})
export function PortalAdminCatalogPage() {
    const access = usePortalAdminAccess()
    const [supplierId, setSupplierId] = useState("")
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
                const targetSupplier = body.supplier_id
                if (
                    typeof targetSupplier === "string" &&
                    targetSupplier !== supplierId
                ) {
                    setSupplierId(targetSupplier)
                    setPagination((current) => ({ ...current, pageIndex: 0 }))
                }
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
            } catch (cause) {
                const disposition = commandFailureDisposition(cause)
                if (disposition === "rejected") intent.current = null
                setFailure(disposition)
                setError(cause)
            }
        },
        [mutation, failure, supplierId],
    )
    const form = useAppForm({
        defaultValues: { supplierId: "", skuId: "" },
        validators: { onSubmit: schema },
        onSubmit: async ({ value }) => {
            await change({
                supplier_id: value.supplierId,
                sku_id: value.skuId,
                active: true,
                expected_version:
                    reviewedBody?.supplier_id === value.supplierId &&
                    reviewedBody?.sku_id === value.skuId
                        ? reviewedBody.expected_version
                        : null,
            })
        },
    })
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
                            !access.can("supplier_portal_catalog:update") ||
                            mutation.isPending ||
                            !!intent.current
                        }
                        onClick={() =>
                            void change({
                                supplier_id: row.original.supplier_id,
                                sku_id: row.original.sku_id,
                                active: !row.original.active,
                                expected_version: row.original.version,
                            })
                        }
                    >
                        {row.original.active ? "撤销开放" : "重新开放"}
                    </Button>
                ),
            },
        ],
        [access, mutation.isPending, change],
    )
    if (
        !access.can("supplier_portal_catalog:list") &&
        !access.profile.isPending
    )
        return (
            <PortalAdminFrame title="SKU定向开放">
                <p>当前账号没有定向目录查看权限。</p>
            </PortalAdminFrame>
        )
    if (!access.can("supplier:list"))
        return (
            <PortalAdminFrame title="SKU定向开放">
                <p className="text-sm">
                    需供应商列表查看权限才能选择供应商；请联系管理员配置资格，具体审核可从工作台进入。
                </p>
            </PortalAdminFrame>
        )
    return (
        <PortalAdminFrame title="SKU定向开放">
            <PortalError error={error} />
            {intent.current && failure === "unknown" && (
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
                <p className="text-sm">
                    配置结果尚未确认，请保留原内容并重试原目录配置。
                </p>
            )}
            {failure === "conflict" && (
                <PortalCommandConflict
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
            {reviewedBody && (
                <div className="space-y-2 rounded-lg border p-4">
                    <p className="text-sm">
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
            <ListWorkSurface
                ariaLabel="SKU定向开放"
                toolbar={
                    <div className="w-full max-w-lg space-y-2">
                        <label
                            htmlFor="supplier-portal-grants-filter-supplier"
                            className="text-sm"
                        >
                            查看供应商开放目录
                        </label>
                        <SupplierSearchCombobox
                            id="supplier-portal-grants-filter-supplier"
                            value={supplierId || undefined}
                            disabled={mutation.isPending || !!intent.current}
                            onValueChange={(value) => {
                                setSupplierId(value ?? "")
                                setPagination((p) => ({ ...p, pageIndex: 0 }))
                            }}
                        />
                    </div>
                }
                selectionBar={
                    <span className="text-sm">
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
                            <PortalError
                                error={query.error}
                                retry={() => void query.refetch()}
                                id="supplier-portal-admin-grants-retry"
                            />
                        }
                        emptyState={
                            <p className="p-6 text-sm">
                                {supplierId
                                    ? "此供应商暂无定向开放配置。"
                                    : "请选择供应商查看目录。"}
                            </p>
                        }
                    />
                }
            />
            {access.can("supplier_portal_catalog:update") &&
                !access.can("sku:list") && (
                    <p className="text-sm text-muted-foreground">
                        需公司SKU列表查看权限才能选择并开放新规格；现有配置按已有资格维护。
                    </p>
                )}
            {access.can("supplier_portal_catalog:update") &&
                access.can("sku:list") && (
                    <form
                        className="space-y-4 rounded-xl border p-5"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <h2 className="font-semibold">开放一个精确SKU</h2>
                        <p className="text-sm text-muted-foreground">
                            包含未上架规格；开放报价不改变销售上架状态。
                        </p>
                        <form.AppField name="supplierId">
                            {(field) => (
                                <div className="space-y-2">
                                    <label
                                        className="text-sm"
                                        htmlFor="supplier-portal-grant-supplier"
                                    >
                                        供应商
                                    </label>
                                    <SupplierSearchCombobox
                                        id="supplier-portal-grant-supplier"
                                        value={field.state.value || undefined}
                                        onValueChange={(value) =>
                                            field.handleChange(value ?? "")
                                        }
                                        disabled={
                                            mutation.isPending ||
                                            !!intent.current
                                        }
                                        allowClear={false}
                                    />
                                </div>
                            )}
                        </form.AppField>
                        <form.AppField name="skuId">
                            {(field) => (
                                <div className="space-y-2">
                                    <label
                                        className="text-sm"
                                        htmlFor="supplier-portal-grant-sku"
                                    >
                                        精确公司SKU
                                    </label>
                                    <CompanySkuSearchCombobox
                                        id="supplier-portal-grant-sku"
                                        label="精确公司SKU"
                                        value={field.state.value || undefined}
                                        onValueChange={(value) =>
                                            field.handleChange(value ?? "")
                                        }
                                        disabled={
                                            mutation.isPending ||
                                            !!intent.current
                                        }
                                        allowClear={false}
                                    />
                                </div>
                            )}
                        </form.AppField>
                        <form.AppForm>
                            <form.SubmitButton
                                id="supplier-portal-grant-save"
                                label={
                                    intent.current
                                        ? "重试原配置"
                                        : "开放此SKU报价"
                                }
                                disabled={
                                    mutation.isPending || failure === "conflict"
                                }
                            />
                        </form.AppForm>
                    </form>
                )}
        </PortalAdminFrame>
    )
}
