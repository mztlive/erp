"use client"

import { useRef, useState } from "react"
import { z } from "zod"

import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { createMasterDataObject } from "@/features/master-data/api"
import { parseQuantityScale } from "@/features/master-data/api/mutations/shared"
import type { CreateMasterDataInput } from "@/features/master-data/types"
import { PortalError } from "@/features/supplier-portal/components/surface"
import { commandKey } from "@/features/supplier-portal/lib/presentation"
import type { PortalDictionary } from "@/features/supplier-portal/types"
import { isApiError } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    usePortalAdminAccess,
    usePortalAdminCommand,
    useReviewDictionary,
} from "../hooks"

type DictionaryKind = "brand" | "category" | "unit"
type DictionaryCandidate = PortalDictionary & {
    code?: string
    parent_id?: string | null
    quantity_scale?: number | null
}
type Props = {
    kind: DictionaryKind
    productKind: string
    rawName: string
    onCreated: (candidate: PortalDictionary) => void
    onClose: () => void
}

const dictionaryLabels = { brand: "品牌", category: "分类", unit: "计量单位" }
const createPermissions = {
    brand: "product_brand:create",
    category: "product_category:create",
    unit: "unit_of_measure:create",
}
const resources = {
    brand: "brands",
    category: "categories",
    unit: "unit-of-measures",
} as const
const productKindLabels: Record<string, string> = {
    PHYSICAL: "实物",
    VIRTUAL: "虚拟商品",
    OFFLINE_SERVICE: "线下服务",
    VOUCHER: "卡券",
}
const formValuesSchema = z.object({
    name: z.string().trim().min(1, "请填写名称"),
    code: z.string().trim().min(1, "请填写字典代码"),
    parentId: z.string(),
    symbol: z.string(),
    quantityScale: z.string(),
})
type FormValues = z.input<typeof formValuesSchema>
type CreationIntent = {
    values: FormValues
    knownIds: string[]
    idempotencyKey: string
    effectiveFrom: string
}

const intentSchema = z.object({
    values: formValuesSchema,
    knownIds: z.array(z.string()),
    idempotencyKey: z.string().min(1),
    effectiveFrom: z.string().min(1),
})

function readIntent(key: string): CreationIntent | null {
    try {
        const result = intentSchema.safeParse(
            JSON.parse(sessionStorage.getItem(key) ?? "null"),
        )
        return result.success ? result.data : null
    } catch {
        return null
    }
}

function saveIntent(key: string, intent: CreationIntent | null) {
    try {
        if (intent) sessionStorage.setItem(key, JSON.stringify(intent))
        else sessionStorage.removeItem(key)
    } catch {
        // 存储不可用时仍冻结本次对话框中的请求。
    }
}

function candidateMatches(
    candidate: DictionaryCandidate,
    values: FormValues,
    kind: DictionaryKind,
    productKind: string,
) {
    if (candidate.code !== values.code || candidate.name !== values.name)
        return false
    if (kind === "category")
        return (
            candidate.product_kind === productKind &&
            (candidate.parent_id ?? "") === values.parentId
        )
    return (
        kind !== "unit" ||
        candidate.quantity_scale === parseQuantityScale(values.quantityScale)
    )
}

/** 审核内独立补建入口；创建权限与读取审核候选的权限分别核对。 */
export function PortalDictionaryCreateDialog(props: Props) {
    const access = usePortalAdminAccess()
    if (!access.can(createPermissions[props.kind]))
        return (
            <Dialog open onOpenChange={(open) => !open && props.onClose()}>
                <DialogContent closeButtonId="supplier-portal-review-dictionary-denied-close">
                    <DialogHeader>
                        <DialogTitle>暂不能补建字典</DialogTitle>
                        <DialogDescription>
                            当前账号没有对应维护权限，请转交具备权限的同事处理。
                        </DialogDescription>
                    </DialogHeader>
                    <Button
                        id="supplier-portal-review-dictionary-denied-return"
                        onClick={props.onClose}
                    >
                        返回申请
                    </Button>
                </DialogContent>
            </Dialog>
        )
    return (
        <DictionaryCreateForm
            key={`${access.profile.data?.userid ?? ""}:${props.kind}:${props.productKind}:${props.rawName}`}
            {...props}
            canRead={access.can("supplier_portal_request:review")}
            actorId={access.profile.data?.userid ?? ""}
        />
    )
}

function DictionaryCreateForm({
    kind,
    productKind,
    rawName,
    canRead,
    actorId,
    onCreated,
    onClose,
}: Props & { canRead: boolean; actorId: string }) {
    const prefix = `supplier-portal-review-dictionary-create-${kind}`
    const storageKey = `supplier-portal-dictionary-create:${actorId}:${kind}:${productKind}:${rawName}`
    const [storedIntent] = useState(() => readIntent(storageKey))
    const intent = useRef<CreationIntent | null>(storedIntent)
    const retryRequested = useRef(false)
    const [unknown, setUnknown] = useState(!!storedIntent)
    const [error, setError] = useState<unknown>(null)
    const [recoveryPending, setRecoveryPending] = useState(false)
    const dictionary = useReviewDictionary(kind, canRead)
    const candidates = (dictionary.data ?? []) as DictionaryCandidate[]
    const access = usePortalAdminAccess()
    const mutation = usePortalAdminCommand(createMasterDataObject)
    const clearIntent = () => {
        intent.current = null
        saveIntent(storageKey, null)
    }
    const form = useAppForm({
        defaultValues: storedIntent?.values ?? {
            name: rawName.trim(),
            code: "",
            parentId: "",
            symbol: kind === "unit" ? rawName.trim() : "",
            quantityScale: "0",
        },
        validators: {
            onSubmit: formValuesSchema.superRefine((values, ctx) => {
                if (kind !== "unit") return
                if (!values.symbol.trim())
                    ctx.addIssue({
                        code: "custom",
                        path: ["symbol"],
                        message: "请填写单位符号",
                    })
                if (parseQuantityScale(values.quantityScale) === null)
                    ctx.addIssue({
                        code: "custom",
                        path: ["quantityScale"],
                        message: "数量小数位必须是0至6的整数",
                    })
            }),
        },
        onSubmit: async ({ value }) => {
            setError(null)
            const retrying =
                retryRequested.current && unknown && !!intent.current
            if (!access.can(createPermissions[kind]) || (unknown && !retrying))
                return
            if (kind === "category" && !productKindLabels[productKind]) {
                setError("请先确认提报商品的类型后补建分类")
                return
            }
            if (canRead && !dictionary.isSuccess) {
                setError("请先读取并核对已有字典，再提交补建")
                return
            }
            const values =
                retrying && intent.current
                    ? intent.current.values
                    : {
                          ...value,
                          name: value.name.trim(),
                          code: value.code.trim(),
                          symbol: value.symbol.trim(),
                      }
            if (
                candidates.some((candidate) => candidate.code === values.code)
            ) {
                setError(
                    retrying
                        ? "该代码已有记录，请核对已创建记录"
                        : "该代码已存在，请返回申请核对已有候选",
                )
                return
            }
            if (
                !retrying &&
                kind === "brand" &&
                candidates.some(
                    (candidate) => candidate.name.trim() === values.name,
                )
            ) {
                setError("已有相同名称品牌，请先核对并复用已有候选")
                return
            }
            if (
                kind === "category" &&
                values.parentId &&
                !candidates.some(
                    (candidate) =>
                        candidate.id === values.parentId &&
                        candidate.product_kind === productKind,
                )
            ) {
                setError("上级分类已不可用，请重新读取后选择")
                return
            }
            if (!retrying)
                intent.current = {
                    values,
                    knownIds: candidates.map((candidate) => candidate.id),
                    idempotencyKey: commandKey(`dictionary-${kind}`),
                    effectiveFrom: new Date().toISOString(),
                }
            if (!intent.current) return
            saveIntent(storageKey, intent.current)
            const input: CreateMasterDataInput = {
                resource: resources[kind],
                name: values.name,
                effectiveFrom: intent.current.effectiveFrom,
                changeReason: "供应商新品审核补建",
                idempotencyKey: intent.current.idempotencyKey,
                fields:
                    kind === "brand"
                        ? { code: values.code }
                        : kind === "category"
                          ? {
                                code: values.code,
                                parentId: values.parentId || undefined,
                                productKind,
                            }
                          : {
                                code: values.code,
                                symbol: values.symbol,
                                quantityScale: values.quantityScale,
                            },
            }
            try {
                const result = await mutation.mutateAsync(input)
                if (result.outcome !== "succeeded") {
                    if (result.outcome === "unknown" || retrying)
                        setUnknown(true)
                    else clearIntent()
                    setError(result.message)
                    return
                }
                if (
                    !result.stableId ||
                    !Number.isSafeInteger(result.revisionNo) ||
                    result.revisionNo < 0
                ) {
                    setUnknown(true)
                    setError("补建结果尚未确认，请重新核对已创建记录")
                    return
                }
                const parent = candidates.find(
                    (candidate) => candidate.id === values.parentId,
                )
                clearIntent()
                onCreated({
                    id: result.stableId,
                    version: result.revisionNo,
                    name: values.name,
                    ...(kind === "category"
                        ? {
                              product_kind: productKind,
                              path: parent
                                  ? `${parent.path ?? parent.name} / ${values.name}`
                                  : values.name,
                          }
                        : {}),
                })
                onClose()
            } catch (cause) {
                if (
                    !retrying &&
                    isApiError(cause) &&
                    [400, 401, 403, 404, 409, 422, 429].includes(
                        cause.status ?? 0,
                    )
                ) {
                    clearIntent()
                    setUnknown(false)
                } else setUnknown(true)
                setError(cause)
            }
        },
    })
    const locked = mutation.isPending || unknown || recoveryPending
    const recover = async () => {
        const pending = intent.current
        if (!pending || !canRead) return
        setRecoveryPending(true)
        setError(null)
        try {
            const result = await dictionary.refetch({ throwOnError: true })
            const found = (
                result.data as DictionaryCandidate[] | undefined
            )?.find(
                (candidate) =>
                    !pending.knownIds.includes(candidate.id) &&
                    candidateMatches(
                        candidate,
                        pending.values,
                        kind,
                        productKind,
                    ),
            )
            if (!found) {
                setError(
                    "尚未找到与本次填写一致的新增记录，请保留创建代码并稍后核对",
                )
                return
            }
            clearIntent()
            onCreated(found)
            onClose()
        } catch (cause) {
            setError(cause)
        } finally {
            setRecoveryPending(false)
        }
    }
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !mutation.isPending && !recoveryPending) onClose()
            }}
        >
            <DialogContent
                closeButtonId={`${prefix}-close`}
                showCloseButton={!mutation.isPending && !recoveryPending}
                className="sm:max-w-lg"
            >
                <DialogHeader>
                    <DialogTitle>补建{dictionaryLabels[kind]}</DialogTitle>
                    <DialogDescription>
                        供应商原稿：{rawName || "未填写"}
                        。创建后仍需在当前申请核对映射。
                    </DialogDescription>
                </DialogHeader>
                <PortalError error={error} />
                {canRead && (
                    <PortalError
                        error={dictionary.error}
                        retry={() => void dictionary.refetch()}
                        id={`${prefix}-candidates-retry`}
                    />
                )}
                {unknown && (
                    <p role="status" className="text-sm text-muted-foreground">
                        本次结果尚未确认，填写内容已保留。请核对已创建记录，再继续审核。
                    </p>
                )}
                {!unknown &&
                    kind === "brand" &&
                    canRead &&
                    candidates.some(
                        (candidate) => candidate.name.trim() === rawName.trim(),
                    ) && (
                        <div className="space-y-2 text-sm">
                            <p>已有相同名称品牌，请确认是否复用。</p>
                            {candidates
                                .filter(
                                    (candidate) =>
                                        candidate.name.trim() ===
                                        rawName.trim(),
                                )
                                .map((candidate) => (
                                    <Button
                                        key={candidate.id}
                                        id={`${prefix}-reuse-${toAutomationIdSegment(candidate.id)}`}
                                        type="button"
                                        variant="outline"
                                        disabled={
                                            locked ||
                                            !access.can(
                                                "supplier_portal_request:review",
                                            )
                                        }
                                        onClick={() => {
                                            onCreated(candidate)
                                            onClose()
                                        }}
                                    >
                                        复用 {candidate.name}
                                        {candidate.code
                                            ? `（${candidate.code}）`
                                            : ""}
                                    </Button>
                                ))}
                        </div>
                    )}
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit().catch(setError)
                    }}
                >
                    <form.AppField name="name">
                        {(field) => (
                            <field.TextField
                                id={`${prefix}-name`}
                                label="名称"
                                required
                                disabled={locked}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="code">
                        {(field) => (
                            <field.TextField
                                id={`${prefix}-code`}
                                label={`${dictionaryLabels[kind]}代码`}
                                required
                                disabled={locked}
                            />
                        )}
                    </form.AppField>
                    {kind === "category" && (
                        <>
                            <p className="text-sm">
                                适用商品类型：
                                {productKindLabels[productKind] ?? "待确认"}
                            </p>
                            <form.AppField name="parentId">
                                {(field) => (
                                    <field.SelectField
                                        id={`${prefix}-parent`}
                                        label="上级分类"
                                        description="留空创建根分类；选择已有分类时按完整层级路径核对。"
                                        options={candidates
                                            .filter(
                                                (candidate) =>
                                                    candidate.product_kind ===
                                                    productKind,
                                            )
                                            .map((candidate) => ({
                                                value: candidate.id,
                                                label:
                                                    candidate.path ??
                                                    candidate.name,
                                            }))}
                                        loading={
                                            canRead && dictionary.isPending
                                        }
                                        disabled={locked || !canRead}
                                    />
                                )}
                            </form.AppField>
                            <form.Subscribe
                                selector={(state) =>
                                    [
                                        state.values.parentId,
                                        state.values.name,
                                    ] as const
                                }
                            >
                                {([parentId, name]) => {
                                    const parent = candidates.find(
                                        (candidate) =>
                                            candidate.id === parentId,
                                    )
                                    return (
                                        <p className="text-sm text-muted-foreground">
                                            创建路径：
                                            {parent
                                                ? `${parent.path ?? parent.name} / `
                                                : ""}
                                            {name.trim() || "待填写名称"}
                                        </p>
                                    )
                                }}
                            </form.Subscribe>
                        </>
                    )}
                    {kind === "unit" && (
                        <>
                            <form.AppField name="symbol">
                                {(field) => (
                                    <field.TextField
                                        id={`${prefix}-symbol`}
                                        label="单位符号"
                                        required
                                        disabled={locked}
                                    />
                                )}
                            </form.AppField>
                            <form.AppField name="quantityScale">
                                {(field) => (
                                    <field.SelectField
                                        id={`${prefix}-quantity-scale`}
                                        label="数量小数位"
                                        options={[
                                            "0",
                                            "1",
                                            "2",
                                            "3",
                                            "4",
                                            "5",
                                            "6",
                                        ].map((value) => ({
                                            value,
                                            label: `${value}位小数`,
                                        }))}
                                        allowClear={false}
                                        required
                                        disabled={locked}
                                    />
                                )}
                            </form.AppField>
                        </>
                    )}
                    <DialogFooter>
                        <Button
                            id={`${prefix}-return`}
                            type="button"
                            variant="outline"
                            disabled={mutation.isPending || recoveryPending}
                            onClick={onClose}
                        >
                            返回申请
                        </Button>
                        {unknown ? (
                            <>
                                <Button
                                    id={`${prefix}-retry-original`}
                                    type="button"
                                    variant="outline"
                                    disabled={
                                        mutation.isPending ||
                                        recoveryPending ||
                                        !access.can(createPermissions[kind])
                                    }
                                    onClick={() => {
                                        retryRequested.current = true
                                        void form
                                            .handleSubmit()
                                            .catch(setError)
                                            .finally(() => {
                                                retryRequested.current = false
                                            })
                                    }}
                                >
                                    重试本次创建
                                </Button>
                                <Button
                                    id={`${prefix}-recover`}
                                    type="button"
                                    disabled={
                                        !canRead ||
                                        mutation.isPending ||
                                        recoveryPending
                                    }
                                    onClick={() => void recover()}
                                >
                                    {recoveryPending
                                        ? "核对中…"
                                        : "核对已创建记录"}
                                </Button>
                            </>
                        ) : (
                            <form.AppForm>
                                <form.SubmitButton
                                    id={`${prefix}-submit`}
                                    label="创建并返回核对"
                                    loading={mutation.isPending}
                                    disabled={
                                        locked ||
                                        (canRead && !dictionary.isSuccess)
                                    }
                                />
                            </form.AppForm>
                        )}
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
