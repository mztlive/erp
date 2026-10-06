"use client"
import { useRef, useState } from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useAppForm } from "@/components/form"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { percentageFromRate } from "../lib/forms"
import { PortalImage } from "../components/portal-image"
import { Button } from "@/components/ui/button"
import { z } from "zod"
import { portalSaveApplication } from "../api"
import {
    usePortalCommand,
    usePortalHistory,
    usePortalOffering,
} from "../hooks/queries"
import {
    availabilityLabels,
    commandKey,
    isRejectedPortalCommand,
    offeringName,
    offeringTerms,
    relationLabels,
    timeLabel,
} from "../lib/presentation"
import { PortalCommandConflict } from "../components/command-conflict"
import { PortalSurface, PortalError } from "../components/surface"
import { PortalAvailabilityForm } from "../components/availability-form"
import { PortalTermsForm } from "../components/terms-form"
import { usePortalProfile } from "../components/portal-session"
export function PortalOfferingDetailPage({
    offeringId,
}: {
    offeringId: string
}) {
    const query = usePortalOffering(offeringId)
    const history = usePortalHistory(offeringId)
    const profile = usePortalProfile()
    const router = useRouter()
    const [editTerms, setEditTerms] = useState(false)
    const [error, setError] = useState<unknown>(null)
    const intent = useRef<Record<string, unknown> | null>(null)
    const mutation = usePortalCommand((body: Record<string, unknown>) =>
        portalSaveApplication(body),
    )
    const form = useAppForm({
        defaultValues: { reason: "" },
        validators: {
            onSubmit: z.object({
                reason: z.string().trim().min(1, "请填写停止供应原因"),
            }),
        },
        onSubmit: async ({ value }) => {
            if (!query.data) return
            intent.current ??= {
                snapshot: {
                    kind: "STOP_SUPPLY",
                    offering_id: query.data.id,
                    expected_offering_version: query.data.version,
                    expected_revision_no: query.data.current_revision_no,
                },
                reason: value.reason.trim(),
                expected_version: null,
                idempotency_key: commandKey("stop"),
            }
            try {
                const result = await mutation.mutateAsync(intent.current)
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
    if (!query.data)
        return (
            <PortalSurface title="供给资料">
                <PortalError
                    error={query.error}
                    retry={() => void query.refetch()}
                />
                {query.isPending && <p>读取供给资料…</p>}
            </PortalSurface>
        )
    const offering = query.data
    const terms = offeringTerms(offering)
    const writable =
        profile?.role === "maintainer" &&
        offering.source_type !== "API" &&
        offering.writable !== false
    return (
        <PortalSurface
            title={offeringName(offering)}
            description={`${offering.specification ?? ""} · 订货编码：${offering.supplier_sku_code}`}
            actions={
                <Link
                    id="supplier-portal-offering-back"
                    href="/supplier-portal/offerings"
                    className="text-sm text-primary"
                >
                    返回我的供给
                </Link>
            }
        >
            <PortalImage
                assetId={offering.image_asset_id}
                source={{ offering_id: offering.id }}
                alt={offeringName(offering)}
                className="h-36 w-36 rounded-lg object-contain"
            />
            <div className="grid gap-4 rounded-xl border bg-card p-5 md:grid-cols-3">
                <div>
                    <p className="text-sm text-muted-foreground">
                        代发含税供货价
                    </p>
                    <MoneyValue
                        value={terms.dropship_supply_price_gross}
                        taxBasis="gross"
                        size="summary"
                    />
                </div>
                <div>
                    <p className="text-sm text-muted-foreground">
                        集采含税供货价
                    </p>
                    <MoneyValue
                        value={terms.bulk_supply_price_gross}
                        taxBasis="gross"
                        size="summary"
                    />
                </div>
                <div>
                    <p className="text-sm text-muted-foreground">可供数量</p>
                    {offering.available_quantity == null ? (
                        "未提供"
                    ) : (
                        <QuantityValue
                            value={offering.available_quantity}
                            unit={offering.unit_name ?? ""}
                        />
                    )}
                    <p className="mt-1 text-xs text-muted-foreground">
                        {timeLabel(offering.availability_source_updated_at)}
                    </p>
                </div>
                <p>供给关系：{relationLabels[offering.status]}</p>
                <p>
                    可供情况：
                    {availabilityLabels[offering.availability_status] ??
                        "待核对"}
                </p>
                <p>可供区域：{terms.supply_region.join("、")}</p>
                <p>
                    集采起订量：
                    <QuantityValue
                        value={terms.bulk_minimum_order_quantity}
                        unit={offering.unit_name ?? ""}
                    />
                </p>
                <p>
                    有效期：{terms.valid_from} 至 {terms.valid_to ?? "长期有效"}
                </p>
                <p>
                    税率：
                    <RateValue
                        value={percentageFromRate(terms.input_tax_rate)}
                        precision={2}
                    />
                </p>
            </div>
            {offering.pending_applications?.length ? (
                <section className="rounded-lg border p-5">
                    <h2 className="font-semibold">待确认变更</h2>
                    {offering.pending_applications.map((application) => (
                        <p key={application.id}>
                            <Link
                                id={`supplier-portal-offering-application-${toAutomationIdSegment(application.id)}`}
                                href={`/supplier-portal/applications/${encodeURIComponent(application.id)}`}
                                className="text-primary"
                            >
                                {application.title ?? "查看本次申请"}
                            </Link>{" "}
                            · 待采购确认
                        </p>
                    ))}
                </section>
            ) : null}
            <PortalAvailabilityForm
                offering={offering}
                onReload={async () => (await query.refetch()).data}
            />
            <section className="space-y-3">
                <Button
                    id="supplier-portal-offering-terms-edit"
                    variant="outline"
                    disabled={!writable}
                    onClick={() => setEditTerms(!editTerms)}
                >
                    {editTerms ? "收起条款申请" : "申请修改供给条款"}
                </Button>
                {editTerms && <PortalTermsForm offering={offering} />}
            </section>
            <section className="space-y-3 rounded-xl border p-5">
                <h2 className="font-semibold">条款历史</h2>
                <PortalError
                    error={history.error}
                    retry={() => void history.refetch()}
                    id="supplier-portal-history-retry"
                />
                {history.data?.map((item) => (
                    <div
                        key={item.id}
                        className="flex flex-wrap gap-4 border-b pb-2 text-sm"
                    >
                        <span>第 {item.revision_no} 版</span>
                        <span>
                            代发供货价{" "}
                            <MoneyValue
                                value={item.terms?.dropship_supply_price_gross}
                                taxBasis="gross"
                            />
                        </span>
                        <span>
                            集采供货价{" "}
                            <MoneyValue
                                value={item.terms?.bulk_supply_price_gross}
                                taxBasis="gross"
                            />
                        </span>
                    </div>
                ))}
            </section>
            <form
                className="space-y-3 rounded-xl border p-5"
                onSubmit={(event) => {
                    event.preventDefault()
                    void form.handleSubmit()
                }}
            >
                <h2 className="font-semibold">申请停止供应</h2>
                <p className="text-sm text-muted-foreground">
                    采购确认后停止关系。当前临时缺货请在可供情况中单独报送。
                </p>
                <PortalError error={error} />
                <PortalCommandConflict
                    error={error}
                    id="supplier-portal-stop-create-recheck-conflict"
                    currentSummary={
                        query.data
                            ? `当前供给：${offeringName(query.data)}；${relationLabels[query.data.status] ?? "待核对"}。请核对停止供应原因。`
                            : undefined
                    }
                    disabled={mutation.isPending}
                    onRecheck={async () => {
                        const result = await query.refetch()
                        if (!result.data || result.isError)
                            throw (
                                result.error ??
                                new Error("供给资料暂不可用，请重新读取")
                            )
                    }}
                    onConfirmed={() => {
                        intent.current = null
                        setError(null)
                    }}
                />
                <form.AppField name="reason">
                    {(field) => (
                        <field.TextareaField
                            id="supplier-portal-stop-reason"
                            label="停止供应原因"
                            required
                            disabled={
                                !writable ||
                                mutation.isPending ||
                                !!intent.current
                            }
                        />
                    )}
                </form.AppField>
                <form.AppForm>
                    <form.SubmitButton
                        id="supplier-portal-stop-save"
                        label={
                            intent.current
                                ? "重试原停止申请"
                                : "保存停止供应草稿"
                        }
                        disabled={!writable || mutation.isPending}
                    />
                </form.AppForm>
            </form>
        </PortalSurface>
    )
}
