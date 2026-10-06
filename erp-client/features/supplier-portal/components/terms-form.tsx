"use client"
import { useRef, useState } from "react"
import { z } from "zod"
import { Button } from "@/components/ui/button"
import { useRouter } from "next/navigation"
import { useAppForm } from "@/components/form"
import { portalSaveApplication } from "../api"
import {
    usePortalApplication,
    usePortalCatalogTarget,
    usePortalCommand,
    usePortalOffering,
} from "../hooks/queries"
import type {
    PortalApplication,
    PortalCatalogSku,
    PortalOffering,
    PortalQuoteTargetVersion,
} from "../types"
import {
    commandKey,
    isRejectedPortalCommand,
    offeringTerms,
    timeLabel,
} from "../lib/presentation"
import {
    termsFields,
    termsFromValues,
    termsSchema,
    valuesFromTerms,
} from "../lib/forms"
import { PortalCommandConflict } from "./command-conflict"
import { PortalError } from "./surface"
import { usePortalProfile } from "./portal-session"

/** 已有商品首次报价和已有供给修订使用相同条款控件，分别冻结目标身份和版本。 */
export function PortalTermsForm({
    sku,
    offering,
    draft,
}: {
    sku?: PortalCatalogSku
    offering?: PortalOffering
    draft?: PortalApplication
}) {
    const router = useRouter()
    const profile = usePortalProfile()
    const intent = useRef<{ body: string; key: string } | null>(null)
    const [error, setError] = useState<unknown>(null)
    const [draftVersion, setDraftVersion] = useState(draft?.version ?? null)
    const [offeringVersion, setOfferingVersion] = useState<number | undefined>()
    const [revisionNo, setRevisionNo] = useState<number | undefined>()
    const [quoteTarget, setQuoteTarget] = useState<
        PortalQuoteTargetVersion | undefined
    >()
    const application = usePortalApplication(draft?.id ?? "")
    const offeringQuery = usePortalOffering(
        offering?.id ?? String(draft?.input.offering_id ?? ""),
    )
    const catalog = usePortalCatalogTarget(
        sku?.id ?? String(draft?.input.sku_id ?? ""),
    )
    const mutation = usePortalCommand((body: Record<string, unknown>) =>
        portalSaveApplication(body, draft?.id),
    )
    const current = offering
        ? offeringTerms(offering)
        : (draft?.input.terms as ReturnType<typeof offeringTerms> | undefined)
    const form = useAppForm({
        defaultValues: {
            ...valuesFromTerms(current),
            orderingCode:
                offering?.supplier_sku_code ??
                String(draft?.input.supplier_sku_code ?? ""),
            availability: String(
                draft?.input.availability_status ?? "AVAILABLE",
            ),
            quantity: String(draft?.input.available_quantity ?? ""),
            reportedAt:
                typeof draft?.input.availability_reported_at === "number"
                    ? draft.input.availability_reported_at
                    : Math.floor(Date.now() / 1000),
            reason: draft?.reason ?? "",
        },
        validators: {
            onSubmit: termsSchema.extend({ reportedAt: z.number().int() }),
        },
        onSubmit: async ({ value }) => {
            setError(null)
            if (
                !offering &&
                draft?.kind !== "terms" &&
                !value.orderingCode.trim()
            ) {
                setError(new Error("请填写自己的订货编码"))
                return
            }
            const terms = {
                ...termsFromValues(value),
                product_capabilities: current?.product_capabilities ?? [],
            }
            const snapshot =
                offering || draft?.kind === "terms"
                    ? {
                          kind: "TERMS_CHANGE",
                          offering_id: offering?.id ?? draft?.input.offering_id,
                          expected_offering_version:
                              offeringVersion ??
                              offering?.version ??
                              draft?.input.expected_offering_version,
                          expected_revision_no:
                              revisionNo ??
                              offering?.current_revision_no ??
                              draft?.input.expected_revision_no,
                          terms,
                      }
                    : {
                          kind: "EXISTING_QUOTE",
                          sku_id: sku?.id ?? draft?.input.sku_id,
                          target_version:
                              quoteTarget ??
                              sku?.target_version ??
                              draft?.input.target_version,
                          supplier_sku_code: value.orderingCode.trim(),
                          supplier_product_code: null,
                          terms,
                          availability_status: value.availability,
                          available_quantity: value.quantity.trim() || null,
                          availability_reported_at: value.reportedAt,
                      }
            const body = {
                snapshot,
                reason: value.reason.trim(),
                expected_version: draftVersion,
            }
            const fingerprint = JSON.stringify(body)
            intent.current ??= { body: fingerprint, key: commandKey("save") }
            try {
                const saved = await mutation.mutateAsync({
                    ...(JSON.parse(intent.current.body) as Record<
                        string,
                        unknown
                    >),
                    idempotency_key: intent.current.key,
                })
                intent.current = null
                router.push(
                    `/supplier-portal/applications/${encodeURIComponent(saved.id)}`,
                )
            } catch (cause) {
                if (isRejectedPortalCommand(cause)) intent.current = null
                setError(cause)
            }
        },
    })
    const readOnly =
        profile?.role !== "maintainer" ||
        offering?.source_type === "API" ||
        offering?.writable === false
    return (
        <form
            className="space-y-5 rounded-xl border bg-card p-5"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <p className="text-sm text-muted-foreground">
                保存后为草稿；提交采购确认前，当前供货条款继续有效。报价数量和价格按
                {sku?.unit_name ?? offering?.unit_name ?? "当前基础单位"}填写。
            </p>
            <PortalError error={error} />
            <PortalCommandConflict
                error={error}
                id="supplier-portal-terms-recheck-conflict"
                currentSummary={
                    offeringQuery.data
                        ? `当前供给：${offeringQuery.data.name ?? offeringQuery.data.sku_name ?? "商品规格"}；代发含税价${offeringTerms(offeringQuery.data).dropship_supply_price_gross}，集采含税价${offeringTerms(offeringQuery.data).bulk_supply_price_gross}`
                        : catalog.data
                          ? `当前规格：${catalog.data.name} / ${catalog.data.specification} / ${catalog.data.unit_name}。请核对原报价仍适用于当前规格。`
                          : undefined
                }
                disabled={mutation.isPending}
                onRecheck={async () => {
                    if (draft) {
                        const result = await application.refetch()
                        if (!result.data || result.isError)
                            throw (
                                result.error ??
                                new Error("申请暂不可用，请重新读取")
                            )
                        if (
                            !["draft", "returned", "withdrawn"].includes(
                                result.data.status,
                            )
                        )
                            throw new Error(
                                "申请当前状态不能修改，请返回申请页核对",
                            )
                        setDraftVersion(result.data.version)
                    }
                    if (offering || draft?.kind === "terms") {
                        const result = await offeringQuery.refetch()
                        if (!result.data || result.isError)
                            throw (
                                result.error ??
                                new Error("供给资料暂不可用，请重新读取")
                            )
                        setOfferingVersion(result.data.version)
                        setRevisionNo(result.data.current_revision_no)
                    } else {
                        const result = await catalog.refetch()
                        if (result.isError) throw result.error
                        const target = result.data
                        if (!target)
                            throw new Error(
                                "目标规格已不在开放目录，请联系采购核对",
                            )
                        setQuoteTarget(target.target_version)
                    }
                }}
                onConfirmed={() => {
                    intent.current = null
                    setError(null)
                }}
            />
            {!offering && draft?.kind !== "terms" && (
                <form.AppField name="orderingCode">
                    {(field) => (
                        <field.TextField
                            id="supplier-portal-quote-ordering-code"
                            label="供应商订货编码"
                            required
                            disabled={
                                readOnly ||
                                mutation.isPending ||
                                !!intent.current
                            }
                        />
                    )}
                </form.AppField>
            )}
            <div className="grid gap-4 md:grid-cols-2">
                {termsFields.map(([name, label]) => (
                    <form.AppField key={name} name={name}>
                        {(field) => (
                            <field.TextField
                                id={`supplier-portal-terms-${name}`}
                                label={label}
                                disabled={
                                    readOnly ||
                                    mutation.isPending ||
                                    !!intent.current
                                }
                                required={[
                                    "dropshipPrice",
                                    "bulkPrice",
                                    "taxPercentage",
                                    "minimumQuantity",
                                    "regions",
                                ].includes(name)}
                            />
                        )}
                    </form.AppField>
                ))}
                <form.AppField name="validFrom">
                    {(field) => (
                        <field.DateField
                            id="supplier-portal-terms-valid-from"
                            label="生效日期"
                            required
                            disabled={
                                readOnly ||
                                mutation.isPending ||
                                !!intent.current
                            }
                        />
                    )}
                </form.AppField>
                <form.AppField name="validTo">
                    {(field) => (
                        <field.DateField
                            id="supplier-portal-terms-valid-to"
                            label="失效日期"
                            disabled={
                                readOnly ||
                                mutation.isPending ||
                                !!intent.current
                            }
                            description="留空表示长期有效"
                        />
                    )}
                </form.AppField>
                {!offering && (
                    <>
                        <form.AppField name="availability">
                            {(field) => (
                                <field.SelectField
                                    id="supplier-portal-quote-availability"
                                    label="可供情况"
                                    disabled={
                                        readOnly ||
                                        mutation.isPending ||
                                        !!intent.current
                                    }
                                    allowClear={false}
                                    options={[
                                        { value: "AVAILABLE", label: "有货" },
                                        {
                                            value: "OUT_OF_STOCK",
                                            label: "临时缺货",
                                        },
                                    ]}
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="quantity">
                            {(field) => (
                                <field.TextField
                                    id="supplier-portal-quote-quantity"
                                    label="可供数量"
                                    disabled={
                                        readOnly ||
                                        mutation.isPending ||
                                        !!intent.current
                                    }
                                    description="空白表示未提供；0表示明确无货"
                                />
                            )}
                        </form.AppField>
                        <form.Subscribe
                            selector={(state) => state.values.reportedAt}
                        >
                            {(at) => (
                                <div className="flex flex-wrap items-center gap-3 text-sm">
                                    <span>实际报送时间：{timeLabel(at)}</span>
                                    <Button
                                        id="supplier-portal-quote-report-now"
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        disabled={
                                            readOnly ||
                                            mutation.isPending ||
                                            !!intent.current
                                        }
                                        onClick={() =>
                                            form.setFieldValue(
                                                "reportedAt",
                                                Math.floor(Date.now() / 1000),
                                            )
                                        }
                                    >
                                        已核对，按当前时间重新报送可供
                                    </Button>
                                </div>
                            )}
                        </form.Subscribe>
                    </>
                )}
            </div>
            <form.AppField name="reason">
                {(field) => (
                    <field.TextareaField
                        id="supplier-portal-terms-reason"
                        label="申请原因"
                        required
                        disabled={
                            readOnly || mutation.isPending || !!intent.current
                        }
                    />
                )}
            </form.AppField>
            <form.AppForm>
                <form.SubmitButton
                    id="supplier-portal-terms-save"
                    label={intent.current ? "重试保存原内容" : "保存草稿"}
                    disabled={readOnly || mutation.isPending}
                />
            </form.AppForm>
            {readOnly && (
                <p className="text-sm text-muted-foreground">
                    {offering?.source_type === "API"
                        ? "当前供给由接口同步，请联系采购处理商务变更。"
                        : "当前账号仅可查看。"}
                </p>
            )}
        </form>
    )
}
