"use client"

import * as React from "react"
import { useRouter } from "@/lib/navigation"
import { PlusIcon, UploadIcon } from "lucide-react"
import { DetailWorkspaceSection } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { toast } from "@/components/ui/toast"
import { SellableSkuSelectDialog } from "@/features/sales-orders/components/sellable-sku-select-dialog"
import type { SellableSkuPick } from "@/features/sales-orders/lib/sellable-sku-pick"
import { hasPermission } from "@/lib/permissions"
import { getErrorMessage } from "@/lib/api/errors"
import { compactFixed, sumFixed } from "@/lib/fixed-decimal"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"
import { inquiryPath } from "../api"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import type { DemandFormValues } from "../lib/demand-form"
import { deliveryProgress } from "../lib/delivery-progress"
import {
    inquiryStorageKey,
    readInquiryLocalValue,
    writeInquiryLocalValue,
} from "../lib/local-draft"
import type { InquiryLine, SalesInquiry } from "../types"
import { InquiryDemandDetails } from "./inquiry-demand-details"
import { InquiryDemandLines } from "./inquiry-demand-lines"
import { InquiryProgress } from "./inquiry-progress"

export type InquiryDemandEditorHandle = {
    save: () => void
}

export type InquiryDemandEditorStatus = {
    dirty: boolean
    saving: boolean
    disabled: boolean
    lineCount: number
    pendingDeliveryCount: number
    canSave: boolean
}

type DemandRevision = {
    onCancel: () => void
    onSubmitted: () => void
}

function validateRevisionReason({ value }: { value: string | undefined }) {
    if (!value?.trim()) return "请填写修改原因"
    if (value.trim().length > 2000) return "修改原因最多 2000 字"
    return undefined
}

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
    ref,
    inquiry,
    copyFrom,
    userId,
    permissions,
    onSaved,
    onImport,
    productActions,
    feeOwner,
    onDirtyChange,
    onStatusChange,
    revision,
    storageScope,
}: {
    ref?: React.Ref<InquiryDemandEditorHandle>
    inquiry?: SalesInquiry
    copyFrom?: SalesInquiry
    userId: string
    permissions: readonly string[]
    onSaved: () => void
    onImport?: () => void
    productActions?: React.ReactNode
    feeOwner?: React.ReactNode
    onDirtyChange?: (dirty: boolean) => void
    onStatusChange?: (status: InquiryDemandEditorStatus) => void
    revision?: DemandRevision
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
        restoredDirty: boolean
    } | null>(null)
    React.useEffect(() => {
        const stored = readInquiryLocalValue<{
            base_version: number
            values: DemandFormValues
        }>(storageKey)
        onDirtyChange?.(Boolean(stored))
        setInitial(
            stored
                ? { ...stored, restoredDirty: true }
                : {
                      base_version: inquiry?.version ?? 0,
                      restoredDirty: false,
                      values: {
                          customer_id:
                              inquiry?.customer_id ??
                              copyFrom?.customer_id ??
                              "",
                          customer_name:
                              inquiry?.customer_name ??
                              copyFrom?.customer_name ??
                              "",
                          sales_owner_user_id:
                              inquiry?.sales_owner_user_id ?? userId,
                          business_org_unit_id:
                              inquiry?.business_org_unit_id ?? "",
                          deadline_at: inquiry?.deadline_at ?? 0,
                          revision_reason: "",
                          copied_from_inquiry_id: copyFrom?.id ?? null,
                          draft:
                              inquiry?.draft ??
                              (copyFrom
                                  ? {
                                        ...copyFrom.draft,
                                        lines: copyFrom.draft.lines.map(
                                            (line) => ({
                                                ...line,
                                                line_id: crypto.randomUUID(),
                                                allocations:
                                                    line.allocations.map(
                                                        (address) => ({
                                                            ...address,
                                                            allocation_id:
                                                                crypto.randomUUID(),
                                                        }),
                                                    ),
                                            }),
                                        ),
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
            ref={ref}
            key={storageKey}
            initial={initial.values}
            baseVersion={initial.base_version}
            restoredDirty={initial.restoredDirty}
            inquiry={inquiry}
            userId={userId}
            permissions={permissions}
            storageKey={storageKey}
            onSaved={onSaved}
            onImport={onImport}
            productActions={productActions}
            feeOwner={feeOwner}
            onDirtyChange={onDirtyChange}
            onStatusChange={onStatusChange}
            revision={revision}
        />
    ) : (
        <p role="status" className="text-body-compact text-muted-foreground">
            正在恢复询价输入…
        </p>
    )
}

function DemandEditorContent({
    ref,
    initial,
    baseVersion,
    restoredDirty,
    inquiry,
    userId,
    permissions,
    storageKey,
    onSaved,
    onImport,
    productActions,
    feeOwner,
    onDirtyChange,
    onStatusChange,
    revision,
}: {
    ref?: React.Ref<InquiryDemandEditorHandle>
    initial: DemandFormValues
    baseVersion: number
    restoredDirty: boolean
    inquiry?: SalesInquiry
    userId: string
    permissions: readonly string[]
    storageKey: string
    onSaved: () => void
    onImport?: () => void
    productActions?: React.ReactNode
    feeOwner?: React.ReactNode
    onDirtyChange?: (dirty: boolean) => void
    onStatusChange?: (status: InquiryDemandEditorStatus) => void
    revision?: DemandRevision
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
    const [dirty, setDirty] = React.useState(restoredDirty)
    const [lineStats, setLineStats] = React.useState(() => ({
        lineCount: initial.draft.lines.length,
        pendingDeliveryCount: initial.draft.lines.filter(
            (line) => !deliveryProgress(line).complete,
        ).length,
    }))
    const canSave =
        hasPermission(permissions, "sellable_sku:list") &&
        hasPermission(
            permissions,
            revision
                ? "sales_inquiry:reinquire"
                : inquiry
                  ? "sales_inquiry:update"
                  : "sales_inquiry:create",
        )
    const [editVersion, setEditVersion] = React.useState(baseVersion)
    const form = useAppForm({
        defaultValues: initial,
        listeners: {
            onChange: ({ formApi }) => {
                setSaved(false)
                setDirty(true)
                onDirtyChange?.(true)
                const lines = formApi.state.values.draft.lines
                setLineStats({
                    lineCount: lines.length,
                    pendingDeliveryCount: lines.filter(
                        (line) => !deliveryProgress(line).complete,
                    ).length,
                })
                try {
                    writeInquiryLocalValue(storageKey, {
                        base_version: editVersion,
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
                if (revision && inquiry) {
                    if (!canSave) throw new Error("当前账号没有修改需求权限。")
                    if (inquiry.version !== editVersion)
                        throw new Error(
                            "询价已更新，请核对后从最新需求重新编辑。当前输入已保留。",
                        )
                    if (
                        !["awaiting_sales", "confirmed"].includes(
                            inquiry.status,
                        )
                    )
                        throw new Error(
                            "当前询价状态已变化，不能提交需求修改。输入已保留，请取消修改后核对当前状态。",
                        )
                    const reason = value.revision_reason?.trim() ?? ""
                    if (!reason || reason.length > 2000)
                        throw new Error("请填写修改原因，最多 2000 字。")
                    await command.run(
                        `${inquiryPath(false, inquiry.id)}/reinquire`,
                        {
                            expected_version: editVersion,
                            data: {
                                reason,
                                line_ids: [],
                                fee_ids: [],
                                concerns: "",
                                requirement: value.draft,
                            },
                        },
                    )
                    toast.add({
                        title: "已提交需求修改并重新询价",
                        type: "success",
                    })
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
                              expected_version: editVersion,
                              deadline_at: value.deadline_at,
                              data: value.draft,
                          }
                        : {
                              data: {
                                  customer_id: value.customer_id,
                                  customer_name: value.customer_name,
                                  sales_owner_user_id:
                                      value.sales_owner_user_id,
                                  business_org_unit_id:
                                      value.business_org_unit_id,
                                  deadline_at: value.deadline_at,
                                  copied_from_inquiry_id:
                                      value.copied_from_inquiry_id,
                                  draft: value.draft,
                              },
                          },
                    inquiry ? "PUT" : "POST",
                )
                localStorage.removeItem(storageKey)
                if (result.version != null) setEditVersion(result.version)
                setSaved(true)
                setDirty(false)
                if (formApi.state.values === value) formApi.reset(value)
                onDirtyChange?.(false)
                toast.add({
                    title: inquiry ? "需求已保存" : "询价草稿已创建",
                    type: "success",
                })
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
    const saveDisabled =
        busy ||
        !canSave ||
        Boolean(
            revision &&
            inquiry &&
            (inquiry.version !== editVersion ||
                !["awaiting_sales", "confirmed"].includes(inquiry.status)),
        )

    React.useImperativeHandle(
        ref,
        () => ({
            save: () => {
                if (!saveDisabled) void form.handleSubmit()
            },
        }),
        [form, saveDisabled],
    )

    React.useEffect(() => {
        onStatusChange?.({
            dirty,
            saving: command.isPending,
            disabled: saveDisabled,
            lineCount: lineStats.lineCount,
            pendingDeliveryCount: lineStats.pendingDeliveryCount,
            canSave,
        })
    }, [
        canSave,
        command.isPending,
        dirty,
        lineStats.lineCount,
        lineStats.pendingDeliveryCount,
        onStatusChange,
        saveDisabled,
    ])

    // 新建与重询模式下由编辑器直接接入工作台脏离开拦截；
    // 已有询价编辑模式由父视图 InquirySalesView 统一接入。
    useWorkspaceDirtyState({
        dirty: (!inquiry || Boolean(revision)) && dirty,
        busy: command.isPending,
    })

    React.useEffect(() => {
        if (command.isPending) return
        if (
            revision &&
            inquiry &&
            command.resolution?.path ===
                `${inquiryPath(false, inquiry.id)}/reinquire`
        ) {
            try {
                localStorage.removeItem(storageKey)
            } catch {
                // 已提交成功，旧输入保持原版本，后续不可重复提交。
            }
            form.reset()
            setDirty(false)
            onDirtyChange?.(false)
            revision.onSubmitted()
            return
        }
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
        setEditVersion(result.version)
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
    }, [command.resolution, command.isPending])

    const showInlineFooter = !inquiry || Boolean(revision)

    return (
        <form
            id={
                revision
                    ? "sales-inquiry-revision-form"
                    : "sales-inquiry-demand-form"
            }
            className="flex min-w-0 flex-col gap-6"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <fieldset disabled={command.isPending} className="contents">
                {revision && (
                    <DetailWorkspaceSection
                        title="修改需求"
                        subtitle="直接修改商品数量，点击“编辑配送”调整地址。提交后建立新需求版本并重新询价，历史报价及已有销售单保留原版本。"
                        width="form"
                        divider={false}
                        className="py-2 first:pt-0 last:pb-0"
                    >
                        <div className="space-y-4">
                            <form.AppField
                                name="revision_reason"
                                validators={{
                                    onChange: validateRevisionReason,
                                    onSubmit: validateRevisionReason,
                                }}
                                children={(field) => (
                                    <field.TextareaField
                                        id="sales-inquiry-revision-reason"
                                        label="修改原因"
                                        required
                                        disabled={busy}
                                    />
                                )}
                            />
                            <form.AppField
                                name="draft.note"
                                children={(field) => (
                                    <field.TextareaField
                                        id="sales-inquiry-revision-note"
                                        label="需求说明"
                                        disabled={busy}
                                    />
                                )}
                            />
                        </div>
                    </DetailWorkspaceSection>
                )}
                {revision && inquiry && inquiry.version !== editVersion && (
                    <Alert variant="warning">
                        <AlertDescription>
                            询价已更新，当前输入已保留。重新载入会清除本次商品和配送修改，请先核对并保留需要的内容。
                            <Button
                                id="sales-inquiry-revision-reload"
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={busy}
                                onClick={() => {
                                    setEditVersion(inquiry.version)
                                    const values = {
                                        ...form.state.values,
                                        draft: inquiry.draft,
                                    }
                                    form.reset(values)
                                    try {
                                        writeInquiryLocalValue(storageKey, {
                                            base_version: inquiry.version,
                                            values,
                                        })
                                        setLocalFailure(false)
                                    } catch {
                                        setLocalFailure(true)
                                    }
                                    setFailure(null)
                                }}
                            >
                                从最新需求重新编辑
                            </Button>
                        </AlertDescription>
                    </Alert>
                )}
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
                {!revision && (
                    <InquiryDemandDetails
                        form={form}
                        inquiry={inquiry}
                        feeOwner={feeOwner}
                    />
                )}
                <section
                    className="min-w-0 space-y-4 border-t border-border/60 pt-5"
                    aria-label="商品与配送需求"
                >
                    <div className="flex flex-wrap items-center justify-between gap-3">
                        <div className="space-y-2">
                            <form.Subscribe
                                selector={(state) => state.values.draft.lines}
                            >
                                {(lines) => {
                                    const pending = lines.filter(
                                        (line) =>
                                            !deliveryProgress(line).complete,
                                    ).length
                                    return inquiry ? (
                                        <p className="text-body-compact font-medium">
                                            共{" "}
                                            <span className="num">
                                                {lines.length}
                                            </span>{" "}
                                            项商品
                                            {pending > 0 ? (
                                                <span className="ml-2 text-xs font-normal text-warning-soft-foreground">
                                                    ·{" "}
                                                    <span className="num">
                                                        {pending}
                                                    </span>{" "}
                                                    项待完善配送
                                                </span>
                                            ) : null}
                                            {saved ? (
                                                <span
                                                    role="status"
                                                    className="ml-2 text-xs font-normal text-muted-foreground"
                                                >
                                                    · 需求已保存
                                                </span>
                                            ) : null}
                                        </p>
                                    ) : (
                                        <h2 className="text-section-title font-semibold tracking-tight">
                                            商品需求
                                            <span className="num ml-2 text-body-compact font-normal text-muted-foreground">
                                                {lines.length} 项
                                            </span>
                                        </h2>
                                    )
                                }}
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
                                disabled={Boolean(revision && busy)}
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
                        disabled={
                            command.isPending || Boolean(revision && busy)
                        }
                    />
                </section>
                {showInlineFooter && (
                    <div className="flex flex-wrap items-center justify-end gap-3 border-t border-border/60 pt-4">
                        <div className="mr-auto space-y-1">
                            <p className="text-body-compact font-medium">
                                已选{" "}
                                <span className="num">
                                    {lineStats.lineCount}
                                </span>{" "}
                                项商品
                                {lineStats.pendingDeliveryCount > 0 ? (
                                    <>
                                        {" "}
                                        ·{" "}
                                        <span className="num">
                                            {lineStats.pendingDeliveryCount}
                                        </span>{" "}
                                        项待完善配送
                                    </>
                                ) : null}
                            </p>
                            <p
                                className="text-xs text-muted-foreground"
                                role="status"
                            >
                                {saved
                                    ? "需求已保存。"
                                    : localFailure
                                      ? "本机保留失败，请保存需求后再离开。"
                                      : revision
                                        ? "输入已在当前浏览器保留；取消修改不提交，重新进入可继续编辑。"
                                        : "输入已在当前浏览器保留，创建后可在详情页继续编辑或提交采购。"}
                            </p>
                        </div>
                        <div className="flex shrink-0 flex-wrap items-center gap-3">
                            {revision && (
                                <Button
                                    id="sales-inquiry-revision-cancel"
                                    type="button"
                                    variant="outline"
                                    disabled={command.isPending}
                                    onClick={revision.onCancel}
                                >
                                    取消修改
                                </Button>
                            )}
                            {canSave && (
                                <Button
                                    id={
                                        revision
                                            ? "sales-inquiry-revision-submit"
                                            : "sales-inquiry-save-demand"
                                    }
                                    type="submit"
                                    variant="default"
                                    disabled={saveDisabled}
                                >
                                    {command.isPending
                                        ? "正在提交…"
                                        : revision
                                          ? "提交修改并重新询价"
                                          : "创建询价草稿"}
                                </Button>
                            )}
                        </div>
                    </div>
                )}
                <SellableSkuSelectDialog
                    open={pickerOpen}
                    onOpenChange={setPickerOpen}
                    excludeProductKind="VOUCHER"
                    title="选择询价商品"
                    description="每次选中增加 1 个需求单位。已有唯一需求行的商品累加数量；同一商品有多条需求行时另加一行。销售价不作为采购报价。"
                    onConfirm={(picks) => {
                        if (revision && busy) return
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
