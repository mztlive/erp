"use client"

import * as React from "react"
import { createPortal } from "react-dom"
import { useRouter } from "@/lib/navigation"
import { PlusIcon, UploadIcon } from "lucide-react"
import type { DemandFormValues } from "../lib/demand-form"
import { deliveryProgress } from "../lib/delivery-progress"
import { InquiryDemandDetails } from "./inquiry-demand-details"
import { InquiryDemandHeader } from "./inquiry-demand-header"
import { InquiryProgress } from "./inquiry-progress"
import { InquiryDemandLines } from "./inquiry-demand-lines"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { SellableSkuSelectDialog } from "@/features/sales-orders/components/sellable-sku-select-dialog"
import type { SellableSkuPick } from "@/features/sales-orders/lib/sellable-sku-pick"
import { hasPermission } from "@/lib/permissions"
import { getErrorMessage } from "@/lib/api/errors"
import { compactFixed, sumFixed } from "@/lib/fixed-decimal"
import { inquiryPath } from "../api"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import {
    inquiryStorageKey,
    readInquiryLocalValue,
    writeInquiryLocalValue,
} from "../lib/local-draft"
import type { InquiryLine, InquiryRequirement, SalesInquiry } from "../types"

function fromPick(pick: SellableSkuPick): InquiryLine {
    return {
        line_id: crypto.randomUUID(),
        sku_id: pick.skuId,
        sku_revision_id: pick.skuRevisionId,
        sku_code: pick.skuNo,
        product_name: pick.name,
        specification: pick.specificationLabel,
        unit_code: pick.baseUnit,
        quantity_scale: 6,
        quantity: "1",
        fulfillment_kind:
            pick.productKind === "OFFLINE_SERVICE"
                ? "service"
                : pick.productKind === "VIRTUAL"
                  ? "virtual"
                  : "physical",
        delivery_due_at: 0,
        delivery_terms: "",
        sales_reference_price_gross: null,
        allocations: [],
    }
}

function addPicks(
    lines: readonly InquiryLine[],
    picks: readonly SellableSkuPick[],
): InquiryLine[] {
    const next = [...lines]
    for (const pick of picks) {
        const matches = next.filter((line) => line.sku_id === pick.skuId)
        // 同一 SKU 有多条需求时，不能替销售选择交期或商业条件。
        if (matches.length !== 1) {
            next.push(fromPick(pick))
            continue
        }
        const line = matches[0]!
        let quantity: string
        try {
            quantity = compactFixed(
                sumFixed([line.quantity, "1"], {
                    maxScale: line.quantity_scale,
                    outputScale: line.quantity_scale,
                }),
            )
        } catch {
            throw new Error(
                `请先修正「${line.product_name}」的需求数量，再添加商品。`,
            )
        }
        next[next.indexOf(line)] = { ...line, quantity }
    }
    return next
}

export function InquiryDemandEditor({
    inquiry,
    copyFrom,
    userId,
    permissions,
    onSaved,
    onImport,
    productActions,
    footerActions,
    detailsContainer,
    headerActionsContainer,
    onDirtyChange,
    onConfirmRequirement,
    storageScope,
}: {
    inquiry?: SalesInquiry
    copyFrom?: SalesInquiry
    userId: string
    permissions: readonly string[]
    onSaved: () => void
    onImport?: () => void
    productActions?: React.ReactNode
    footerActions?: React.ReactNode
    detailsContainer?: HTMLElement | null
    headerActionsContainer?: HTMLElement | null
    onDirtyChange?: (dirty: boolean) => void
    onConfirmRequirement?: (requirement: InquiryRequirement) => void
    storageScope?: string
}) {
    const storageKey = inquiryStorageKey(
        userId,
        storageScope ??
            `${inquiry?.id ?? (copyFrom ? `new-from-${copyFrom.id}` : "new")}:demand`,
    )
    const [initial, setInitial] = React.useState<{
        values: DemandFormValues
        base_version: number
    } | null>(null)
    React.useEffect(() => {
        const stored = readInquiryLocalValue<{
            base_version: number
            values: DemandFormValues
        }>(storageKey)
        onDirtyChange?.(Boolean(stored))
        setInitial(
            stored ?? {
                base_version: inquiry?.version ?? 0,
                values: {
                    customer_id:
                        inquiry?.customer_id ?? copyFrom?.customer_id ?? "",
                    customer_name:
                        inquiry?.customer_name ?? copyFrom?.customer_name ?? "",
                    sales_owner_user_id: inquiry?.sales_owner_user_id ?? userId,
                    business_org_unit_id: inquiry?.business_org_unit_id ?? "",
                    deadline_at: inquiry?.deadline_at ?? 0,
                    copied_from_inquiry_id: copyFrom?.id ?? null,
                    draft:
                        inquiry?.draft ??
                        (copyFrom
                            ? {
                                  ...copyFrom.draft,
                                  lines: copyFrom.draft.lines.map((line) => ({
                                      ...line,
                                      line_id: crypto.randomUUID(),
                                      allocations: line.allocations.map(
                                          (address) => ({
                                              ...address,
                                              allocation_id:
                                                  crypto.randomUUID(),
                                          }),
                                      ),
                                  })),
                              }
                            : { lines: [], note: "" }),
                },
            },
        )
        // 同一编辑对象的后台刷新不重建正在输入的表单。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [storageKey])
    return initial ? (
        <DemandEditorContent
            key={storageKey}
            initial={initial.values}
            baseVersion={initial.base_version}
            inquiry={inquiry}
            userId={userId}
            permissions={permissions}
            storageKey={storageKey}
            onSaved={onSaved}
            onImport={onImport}
            productActions={productActions}
            footerActions={footerActions}
            detailsContainer={detailsContainer}
            headerActionsContainer={headerActionsContainer}
            onDirtyChange={onDirtyChange}
            onConfirmRequirement={onConfirmRequirement}
        />
    ) : (
        <p role="status">正在恢复询价输入…</p>
    )
}

function DemandEditorContent({
    initial,
    baseVersion,
    inquiry,
    userId,
    permissions,
    storageKey,
    onSaved,
    onImport,
    productActions,
    footerActions,
    detailsContainer,
    headerActionsContainer,
    onDirtyChange,
    onConfirmRequirement,
}: {
    initial: DemandFormValues
    baseVersion: number
    inquiry?: SalesInquiry
    userId: string
    permissions: readonly string[]
    storageKey: string
    onSaved: () => void
    onImport?: () => void
    productActions?: React.ReactNode
    footerActions?: React.ReactNode
    detailsContainer?: HTMLElement | null
    headerActionsContainer?: HTMLElement | null
    onDirtyChange?: (dirty: boolean) => void
    onConfirmRequirement?: (requirement: InquiryRequirement) => void
}) {
    const router = useRouter()
    const command = useInquiryCommand(
        userId,
        inquiry?.id ??
            (initial.copied_from_inquiry_id
                ? `new-from-${initial.copied_from_inquiry_id}`
                : "new"),
    )
    const [pickerOpen, setPickerOpen] = React.useState(false)
    const [failure, setFailure] = React.useState<string | null>(null)
    const [localFailure, setLocalFailure] = React.useState(false)
    const [saved, setSaved] = React.useState(false)
    const canSave =
        hasPermission(permissions, "sellable_sku:list") &&
        hasPermission(
            permissions,
            onConfirmRequirement
                ? "sales_inquiry:reinquire"
                : inquiry
                  ? "sales_inquiry:update"
                  : "sales_inquiry:create",
        )
    const versionRef = React.useRef(baseVersion)
    const form = useAppForm({
        defaultValues: initial,
        listeners: {
            onChange: ({ formApi }) => {
                setSaved(false)
                onDirtyChange?.(true)
                try {
                    writeInquiryLocalValue(storageKey, {
                        base_version: versionRef.current,
                        values: formApi.state.values,
                    })
                    setLocalFailure(false)
                } catch {
                    setLocalFailure(true)
                }
            },
        },
        onSubmit: async ({ value, formApi }) => {
            setFailure(null)
            try {
                if (onConfirmRequirement) {
                    onConfirmRequirement(value.draft)
                    return
                }
                if (!value.customer_id) throw new Error("请选择客户后保存。")
                const result = await command.run<{
                    inquiry_id?: string
                    id?: string
                    version?: number
                }>(
                    inquiryPath(false, inquiry?.id) + (inquiry ? "/draft" : ""),
                    inquiry
                        ? {
                              expected_version: versionRef.current,
                              deadline_at: value.deadline_at,
                              data: value.draft,
                          }
                        : { data: value },
                    inquiry ? "PUT" : "POST",
                )
                localStorage.removeItem(storageKey)
                if (result.version != null) versionRef.current = result.version
                setSaved(true)
                if (formApi.state.values === value) formApi.reset(value)
                onDirtyChange?.(false)
                const createdId = result.inquiry_id ?? result.id
                if (!inquiry && createdId)
                    router.replace(
                        `/sales/inquiries/${encodeURIComponent(createdId)}`,
                    )
                else onSaved()
            } catch (error) {
                setFailure(
                    getErrorMessage(error, "保存失败，请保留输入并重试。"),
                )
            }
        },
    })
    const busy = command.isPending || !command.ready || command.unresolved
    React.useEffect(() => {
        const result = command.resolution?.result as
            | { inquiry_id?: string; version?: number }
            | undefined
        const expectedPath =
            inquiryPath(false, inquiry?.id) + (inquiry ? "/draft" : "")
        if (
            command.resolution?.path !== expectedPath ||
            result?.version == null
        )
            return
        versionRef.current = result.version
        if (!inquiry && result.inquiry_id && command.resolution?.replayed) {
            writeInquiryLocalValue(
                inquiryStorageKey(userId, `${result.inquiry_id}:demand`),
                { base_version: result.version, values: form.state.values },
            )
            localStorage.removeItem(storageKey)
            router.replace(
                `/sales/inquiries/${encodeURIComponent(result.inquiry_id)}`,
            )
        }
        // 仅自己的成功命令可更新本地编辑基准；外部刷新仍保留原版本并交由服务端冲突校验。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [command.resolution])
    return (
        <form
            className="flex min-w-0 flex-col gap-6 pb-4"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <fieldset disabled={command.isPending} className="contents">
                {command.unresolved && (
                    <Alert variant="warning">
                        <AlertDescription>
                            上次提交结果尚待核对，当前输入已保留。
                            <Button
                                id="sales-inquiry-demand-recover"
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() =>
                                    void command
                                        .recover()
                                        .then(() => onSaved())
                                        .catch((error) =>
                                            setFailure(getErrorMessage(error)),
                                        )
                                }
                            >
                                核对上次提交结果
                            </Button>
                        </AlertDescription>
                    </Alert>
                )}
                {failure && (
                    <Alert variant="destructive">
                        <AlertDescription>{failure}</AlertDescription>
                    </Alert>
                )}
                {localFailure && (
                    <Alert variant="warning">
                        <AlertDescription>
                            当前浏览器无法保存本地输入，请先保存询价后再离开页面。
                        </AlertDescription>
                    </Alert>
                )}
                {detailsContainer === undefined ? (
                    <InquiryDemandDetails form={form} inquiry={inquiry} />
                ) : detailsContainer ? (
                    createPortal(
                        <InquiryDemandHeader
                            form={form}
                            actionsContainer={headerActionsContainer ?? null}
                            disabled={busy}
                        />,
                        detailsContainer,
                    )
                ) : null}
                <section
                    className="min-w-0 space-y-4"
                    aria-label="商品与配送需求"
                >
                    <div className="flex flex-wrap items-center justify-between gap-3">
                        <div className="space-y-2">
                            <form.Subscribe
                                selector={(state) =>
                                    state.values.draft.lines.length
                                }
                            >
                                {(count) =>
                                    inquiry ? (
                                        <p className="text-sm font-medium">
                                            共 {count} 项商品
                                        </p>
                                    ) : (
                                        <h2 className="text-xl font-semibold">
                                            商品需求
                                            <span className="ml-2 text-sm font-normal text-muted-foreground">
                                                {count} 项
                                            </span>
                                        </h2>
                                    )
                                }
                            </form.Subscribe>
                            {inquiry && (
                                <InquiryProgress
                                    progress={inquiry.sales_progress}
                                    compact
                                />
                            )}
                        </div>
                        <div className="flex flex-wrap items-center gap-2">
                            {productActions}
                            {onImport && (
                                <Button
                                    id="sales-inquiry-import-open"
                                    variant="outline"
                                    type="button"
                                    onClick={onImport}
                                >
                                    <UploadIcon
                                        aria-hidden="true"
                                        data-icon="inline-start"
                                    />
                                    导入选品文件
                                </Button>
                            )}
                            <Button
                                id="sales-inquiry-add-products"
                                variant="outline"
                                type="button"
                                onClick={() => setPickerOpen(true)}
                            >
                                <PlusIcon
                                    aria-hidden="true"
                                    data-icon="inline-start"
                                />
                                添加商品
                            </Button>
                        </div>
                    </div>
                    <InquiryDemandLines
                        form={form}
                        inquiry={inquiry}
                        disabled={command.isPending}
                    />
                </section>
                <div className="sticky bottom-[calc(1rem+env(safe-area-inset-bottom))] z-20 flex flex-wrap items-center justify-end gap-3 rounded-lg border bg-card px-4 py-3 shadow-sm">
                    <div className="mr-auto space-y-1">
                        <form.Subscribe
                            selector={(state) => state.values.draft.lines}
                        >
                            {(lines) => {
                                const pending = lines.filter(
                                    (line) => !deliveryProgress(line).complete,
                                ).length
                                return (
                                    <p className="text-sm font-medium">
                                        已选 {lines.length} 项商品
                                        {pending > 0
                                            ? ` · ${pending} 项待完善配送`
                                            : ""}
                                    </p>
                                )
                            }}
                        </form.Subscribe>
                        <p
                            className="text-xs text-muted-foreground"
                            role="status"
                        >
                            {saved
                                ? "需求已保存。"
                                : localFailure
                                  ? "本机保留失败，请保存需求后再离开。"
                                  : "输入已在当前浏览器保留，保存后同步到询价草稿。"}
                        </p>
                    </div>
                    <div className="flex shrink-0 items-center gap-3">
                        <Button
                            id="sales-inquiry-save-demand"
                            type="submit"
                            variant={footerActions ? "outline" : "default"}
                            disabled={busy || !canSave}
                        >
                            {command.isPending
                                ? "正在保存…"
                                : onConfirmRequirement
                                  ? "确认本次需求调整"
                                  : inquiry
                                    ? "保存需求"
                                    : "创建询价草稿"}
                        </Button>
                        {footerActions}
                    </div>
                </div>
                <SellableSkuSelectDialog
                    open={pickerOpen}
                    onOpenChange={setPickerOpen}
                    excludeProductKind="VOUCHER"
                    title="选择询价商品"
                    description="每次选中增加 1 个需求单位。已有唯一需求行的商品累加数量；同一商品有多条需求行时另加一行。销售参考价不作为采购报价。"
                    onConfirm={(picks) => {
                        setFailure(null)
                        try {
                            const lines = addPicks(
                                form.state.values.draft.lines,
                                picks,
                            )
                            form.setFieldValue("draft.lines", lines)
                            setPickerOpen(false)
                        } catch (error) {
                            setFailure(getErrorMessage(error))
                        }
                    }}
                />
            </fieldset>
        </form>
    )
}
