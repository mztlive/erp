"use client"
import { useRef, useState } from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useAppForm } from "@/components/form"
import {
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
    MoneyValue,
    QuantityValue,
    RateValue,
} from "@/components/business"
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
import { PortalError } from "../components/surface"
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
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="供给资料" />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <PortalError
                            error={query.error}
                            retry={() => void query.refetch()}
                        />
                        {query.isPending && <p role="status">读取供给资料…</p>}
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    const offering = query.data
    const terms = offeringTerms(offering)
    const canMaintain = profile?.role === "maintainer"
    const writable =
        canMaintain &&
        offering.source_type !== "API" &&
        offering.writable !== false
    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title={offeringName(offering)}
                documentNumber={offering.supplier_sku_code}
                numberLabel="订货编码"
                meta={offering.specification || undefined}
                primaryStatus={{
                    label: relationLabels[offering.status] ?? "待核对",
                    tone:
                        offering.status === "ACTIVE"
                            ? "success"
                            : offering.status === "STOPPED"
                              ? "void"
                              : "neutral",
                }}
                back={{
                    id: "supplier-portal-offering-back",
                    href: "/supplier-portal/offerings",
                    label: "返回供货商品目录",
                }}
                media={
                    <PortalImage
                        assetId={offering.image_asset_id}
                        source={{ offering_id: offering.id }}
                        alt={offeringName(offering)}
                        className="size-11 shrink-0 rounded-md object-contain"
                    />
                }
            />
            <DetailWorkspaceBody
                summary={
                    <DetailWorkspaceMetrics columns={3} ariaLabel="供给摘要">
                        <DetailWorkspaceMetricItem
                            label="代发含税供货价"
                            value={
                                <MoneyValue
                                    value={terms.dropship_supply_price_gross}
                                    taxBasis="gross"
                                    size="summary"
                                />
                            }
                        />
                        <DetailWorkspaceMetricItem
                            label="集采含税供货价"
                            value={
                                <MoneyValue
                                    value={terms.bulk_supply_price_gross}
                                    taxBasis="gross"
                                    size="summary"
                                />
                            }
                        />
                        <DetailWorkspaceMetricItem
                            label="可供数量"
                            value={
                                offering.available_quantity == null ? (
                                    "未提供"
                                ) : (
                                    <QuantityValue
                                        value={offering.available_quantity}
                                        unit={offering.unit_name ?? ""}
                                    />
                                )
                            }
                            detail={timeLabel(
                                offering.availability_source_updated_at,
                            )}
                        />
                    </DetailWorkspaceMetrics>
                }
            >
                <DetailWorkspaceSection title="当前供给" width="form">
                    <div className="grid gap-3 text-body-compact sm:grid-cols-2">
                        <p>供给关系：{relationLabels[offering.status]}</p>
                        <p>
                            可供情况：
                            {availabilityLabels[offering.availability_status] ??
                                "待核对"}
                        </p>
                        <p>
                            集采起订量：
                            {terms.bulk_minimum_order_quantity != null ? (
                                <QuantityValue
                                    value={terms.bulk_minimum_order_quantity}
                                    unit={offering.unit_name ?? ""}
                                />
                            ) : (
                                "未提供"
                            )}
                        </p>
                        <p>
                            有效期：{terms.valid_from} 起
                            {terms.valid_to
                                ? `，${terms.valid_to} 失效`
                                : "长期有效"}
                        </p>
                        <p>
                            税率：
                            <RateValue
                                value={percentageFromRate(terms.input_tax_rate)}
                                precision={2}
                            />
                        </p>
                    </div>
                </DetailWorkspaceSection>
                {offering.pending_applications?.length ? (
                    <DetailWorkspaceSection title="待确认变更" width="form">
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
                    </DetailWorkspaceSection>
                ) : null}
                {canMaintain && (
                    <>
                        <PortalAvailabilityForm
                            offering={offering}
                            onReload={async () => (await query.refetch()).data}
                        />
                        <DetailWorkspaceSection width="form">
                            <Button
                                id="supplier-portal-offering-terms-edit"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.subtleButton}
                                disabled={!writable}
                                onClick={() => setEditTerms(!editTerms)}
                            >
                                {editTerms
                                    ? "收起条款申请"
                                    : "申请修改供给条款"}
                            </Button>
                            {editTerms && (
                                <PortalTermsForm offering={offering} />
                            )}
                        </DetailWorkspaceSection>
                    </>
                )}
                <DetailWorkspaceSection title="条款历史" width="form">
                    <PortalError
                        error={history.error}
                        retry={() => void history.refetch()}
                        id="supplier-portal-history-retry"
                    />
                    {history.data?.map((item) => (
                        <div
                            key={item.id}
                            className="flex flex-wrap gap-4 border-b pb-2 text-body-compact"
                        >
                            <span>第 {item.revision_no} 版</span>
                            <span>
                                代发供货价{" "}
                                <MoneyValue
                                    value={
                                        item.terms?.dropship_supply_price_gross
                                    }
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
                </DetailWorkspaceSection>
                {canMaintain && (
                    <DetailWorkspaceSection title="申请停止供应" width="form">
                        <form
                            data-slot="detail-form-grid"
                            className="flex flex-col gap-3"
                            onSubmit={(event) => {
                                event.preventDefault()
                                void form.handleSubmit()
                            }}
                        >
                            <p className="text-xs text-muted-foreground">
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
                                            new Error(
                                                "供给资料暂不可用，请重新读取",
                                            )
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
                    </DetailWorkspaceSection>
                )}
            </DetailWorkspaceBody>
        </DetailWorkspacePage>
    )
}
