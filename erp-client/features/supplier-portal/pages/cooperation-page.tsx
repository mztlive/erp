"use client"
import { useRef, useState } from "react"
import { Button } from "@/components/ui/button"
import { useRouter } from "next/navigation"
import { z } from "zod"
import { paymentTermLabel } from "@/lib/business-options"
import { useAppForm } from "@/components/form"
import {
    PERIODIC_SETTLEMENTS,
    parsePeriodicTerm,
    periodicPaymentTerm,
    reconciliationCycle,
} from "@/lib/supplier-payment-terms"
import { portalCooperationApplication, portalSaveApplication } from "../api"
import {
    usePortalApplication,
    usePortalCommand,
    usePortalCooperation,
} from "../hooks/queries"
import { commandKey, isRejectedPortalCommand } from "../lib/presentation"
import type { PortalApplication, PortalCooperation } from "../types"
import { usePortalProfile } from "../components/portal-session"
import { PortalCommandConflict } from "../components/command-conflict"
import { PortalError, PortalSurface } from "../components/surface"
const settlements = [
    { value: "prepayment", label: "预付款" },
    { value: "cash_settlement", label: "现结" },
    ...PERIODIC_SETTLEMENTS,
]
const prepayments = [
    { value: "PREPAY_100", label: "先款100%" },
    { value: "PREPAY_50", label: "先款50%" },
    { value: "PREPAY_30", label: "先款30%" },
]
const schema = z.object({
    settlement: z.string().min(1, "请选择结算方式"),
    term: z.string(),
    days: z
        .string()
        .regex(/^\d{1,3}$/, "请输入期末后付款天数")
        .refine((value) => Number(value) <= 366, "付款天数最多366天"),
    reason: z.string().trim().min(1, "请填写申请原因"),
})
export function PortalCooperationPage() {
    const query = usePortalCooperation()
    return (
        <PortalSurface
            title="合作资料"
            description="付款条件由采购确认后生效，已冻结的采购单保持原条件。"
        >
            <PortalError
                error={query.error}
                retry={() => void query.refetch()}
            />
            {query.data && (
                <>
                    <div className="grid gap-3 rounded-xl border p-5 text-sm md:grid-cols-2">
                        <p>供应商：{query.data.supplier_name}</p>
                        <p>
                            当前付款条件：
                            {query.data.payment_term_label ??
                                (query.data.payment_term
                                    ? paymentTermLabel(query.data.payment_term)
                                    : "待核对")}
                        </p>
                        <p>采购联系人：{query.data.contact_name ?? "未配置"}</p>
                        <p>联系电话：{query.data.contact_phone ?? "未提供"}</p>
                    </div>
                    <PortalCooperationEditor profile={query.data} />
                </>
            )}
        </PortalSurface>
    )
}
export function PortalCooperationEditor({
    profile,
    draft,
}: {
    profile: PortalCooperation
    draft?: PortalApplication
}) {
    const actor = usePortalProfile()
    const router = useRouter()
    const intent = useRef<{
        key: string
        input: Record<string, unknown>
        version: number | null
    } | null>(null)
    const [error, setError] = useState<unknown>(null)
    const [draftVersion, setDraftVersion] = useState(draft?.version ?? null)
    const application = usePortalApplication(draft?.id ?? "")
    const currentProfile = usePortalCooperation()
    const mutation = usePortalCommand((body: Record<string, unknown>) =>
        draft
            ? portalSaveApplication(body, draft.id)
            : portalCooperationApplication(body),
    )
    const term = String(
        draft?.input.payment_term ?? profile.payment_term ?? "PREPAY_100",
    )
    const periodic = parsePeriodicTerm(term)
    const form = useAppForm({
        defaultValues: {
            settlement: String(
                draft?.input.settlement_mode ??
                    profile.settlement_mode ??
                    periodic?.value ??
                    "monthly",
            ),
            term,
            days: String(periodic?.days ?? 15),
            expectedSupplierVersion:
                typeof draft?.input.expected_supplier_version === "number"
                    ? draft.input.expected_supplier_version
                    : (profile.supplier_version ?? profile.version),
            expectedProfileId: String(
                draft?.input.expected_profile_id ?? profile.profile_id ?? "",
            ),
            reason: String(draft?.input.reason ?? draft?.reason ?? ""),
        },
        validators: {
            onSubmit: schema.extend({
                expectedSupplierVersion: z.number().int(),
                expectedProfileId: z
                    .string()
                    .min(1, "供应商付款条件资料尚未完整，请联系采购核对"),
            }),
        },
        onSubmit: async ({ value }) => {
            setError(null)
            const input = {
                expected_supplier_version: value.expectedSupplierVersion,
                expected_profile_id: value.expectedProfileId,
                settlement_mode: value.settlement,
                reconciliation_cycle: reconciliationCycle(value.settlement),
                payment_term:
                    value.settlement === "prepayment"
                        ? value.term
                        : value.settlement === "cash_settlement"
                          ? "CASH_ON_APPROVAL"
                          : periodicPaymentTerm(value.settlement, value.days),
                reason: value.reason.trim(),
            }
            if (
                intent.current &&
                JSON.stringify(input) !== JSON.stringify(intent.current.input)
            ) {
                setError(
                    new Error("请先重试原保存或确认上次申请结果，再修改内容"),
                )
                return
            }
            intent.current ??= {
                key: commandKey("cooperation"),
                input,
                version: draftVersion,
            }
            try {
                const result = await mutation.mutateAsync({
                    input: intent.current.input,
                    expected_version: intent.current.version,
                    idempotency_key: intent.current.key,
                })
                intent.current = null
                router.push(
                    `/supplier-portal/applications/${encodeURIComponent(result.id)}`,
                )
            } catch (cause) {
                if (isRejectedPortalCommand(cause)) intent.current = null
                setError(cause)
            }
        },
    })
    const disabled =
        actor?.role !== "maintainer" || mutation.isPending || !!intent.current
    return (
        <form
            className="space-y-4 rounded-xl border p-5"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <h2 className="font-semibold">申请调整付款条件</h2>
            <p className="text-sm text-muted-foreground">
                当前生效付款条件：
                {profile.payment_term
                    ? paymentTermLabel(profile.payment_term)
                    : "待核对"}
            </p>
            {draft && (
                <Button
                    id="supplier-portal-cooperation-use-current"
                    type="button"
                    variant="outline"
                    disabled={disabled}
                    onClick={() => {
                        form.setFieldValue(
                            "expectedSupplierVersion",
                            profile.supplier_version ?? profile.version,
                        )
                        form.setFieldValue(
                            "expectedProfileId",
                            profile.profile_id ?? "",
                        )
                    }}
                >
                    已核对，按当前合作资料重新保存申请
                </Button>
            )}
            <PortalError error={error} />
            <PortalCommandConflict
                error={error}
                id="supplier-portal-cooperation-recheck-conflict"
                currentSummary={
                    currentProfile.data?.payment_term
                        ? `当前付款条件：${paymentTermLabel(currentProfile.data.payment_term)}。请核对原申请内容仍适用。`
                        : undefined
                }
                disabled={mutation.isPending}
                onRecheck={async () => {
                    const result = await currentProfile.refetch()
                    if (!result.data || result.isError)
                        throw (
                            result.error ??
                            new Error("合作资料暂不可用，请重新读取")
                        )
                    if (draft) {
                        const saved = await application.refetch()
                        if (!saved.data || saved.isError)
                            throw (
                                saved.error ??
                                new Error("申请暂不可用，请重新读取")
                            )
                        if (
                            !["draft", "returned", "withdrawn"].includes(
                                saved.data.status,
                            )
                        )
                            throw new Error(
                                "申请当前状态不能修改，请返回申请页核对",
                            )
                        setDraftVersion(saved.data.version)
                    }
                    form.setFieldValue(
                        "expectedSupplierVersion",
                        result.data.supplier_version ?? result.data.version,
                    )
                    form.setFieldValue(
                        "expectedProfileId",
                        result.data.profile_id ?? "",
                    )
                }}
                onConfirmed={() => {
                    intent.current = null
                    setError(null)
                }}
            />
            <form.AppField name="settlement">
                {(field) => (
                    <field.SelectField
                        id="supplier-portal-cooperation-settlement"
                        label="结算方式"
                        allowClear={false}
                        options={settlements}
                        disabled={disabled}
                    />
                )}
            </form.AppField>
            <form.Subscribe selector={(state) => state.values.settlement}>
                {(settlement) =>
                    settlement === "prepayment" ? (
                        <form.AppField name="term">
                            {(field) => (
                                <field.SelectField
                                    id="supplier-portal-cooperation-prepay"
                                    label="先款比例"
                                    options={prepayments}
                                    allowClear={false}
                                    disabled={disabled}
                                />
                            )}
                        </form.AppField>
                    ) : settlement === "cash_settlement" ? (
                        <p className="text-sm">采购审批通过日付款。</p>
                    ) : (
                        <form.AppField name="days">
                            {(field) => (
                                <field.TextField
                                    id="supplier-portal-cooperation-days"
                                    label="自然结算期末后付款天数"
                                    disabled={disabled}
                                />
                            )}
                        </form.AppField>
                    )
                }
            </form.Subscribe>
            <form.AppField name="reason">
                {(field) => (
                    <field.TextareaField
                        id="supplier-portal-cooperation-reason"
                        label="申请原因"
                        required
                        disabled={disabled}
                    />
                )}
            </form.AppField>
            <form.AppForm>
                <form.SubmitButton
                    id="supplier-portal-cooperation-save"
                    label={
                        intent.current ? "重试保存原内容" : "保存付款条件草稿"
                    }
                    disabled={
                        actor?.role !== "maintainer" || mutation.isPending
                    }
                />
            </form.AppForm>
        </form>
    )
}
