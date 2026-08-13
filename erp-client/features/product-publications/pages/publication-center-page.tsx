"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
    ArrowLeftIcon,
    HistoryIcon,
    LoaderCircleIcon,
    PauseIcon,
    RefreshCwIcon,
    SendIcon,
} from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    DataFreshness,
    DocumentHeader,
    DocumentSection,
    DocumentSummary,
    FormalActionConfirmDialog,
    FormalActionResult,
    OptionCombobox,
    PageHeader,
    PageScaffold,
    RevisionTimeline,
    StatusTrackSummary,
    surfacePanelClassName,
} from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import { type ResultState } from "@/components/business/feedback"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import { Textarea } from "@/components/ui/textarea"
import {
    useManualPauseMutation,
    usePublicationDetailQuery,
    usePublishRevisionMutation,
    useRetryDeliveryMutation,
} from "@/features/product-publications/hooks/queries"
import { PublishGateAlert } from "@/features/product-publications/components/publish-gate-alert"
import { RevisionContent } from "@/features/product-publications/components/revision-content"
import { SafetyPausePanel } from "@/features/product-publications/components/safety-pause-panel"
import {
    publishSchema,
    type SessionEdit,
} from "@/features/product-publications/lib/publish-form"
import type { SaleStatus } from "@/features/product-publications/types"
import {
    MEDIA_ROLE_LABEL,
    SALE_STATUS_LABEL,
} from "@/features/product-publications/types"
import { cn } from "@/lib/utils"
import { goToWorkspaceLabel } from "@/lib/ui-text"

const SECTIONS = [
    { id: "overview", label: "概览" },
    { id: "content", label: "发布内容" },
    { id: "media", label: "媒体" },
    { id: "offering", label: "固定供给" },
    { id: "delivery", label: "发送与版本" },
    { id: "audit", label: "审计" },
] as const

type SectionId = (typeof SECTIONS)[number]["id"]

function parseSection(raw: string | null): SectionId {
    const found = SECTIONS.find((s) => s.id === raw)
    return found?.id ?? "overview"
}

export function PublicationCenterPage({
    publicationId,
}: {
    publicationId: string
}) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const section = parseSection(searchParams.get("section"))
    const revisionParam = searchParams.get("revision") ?? undefined

    const detailQuery = usePublicationDetailQuery(publicationId, revisionParam)
    const publishMutation = usePublishRevisionMutation()
    const pauseMutation = useManualPauseMutation()
    const retryMutation = useRetryDeliveryMutation()

    const [sessionEdit, setSessionEdit] = React.useState<SessionEdit | null>(
        null,
    )
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [pauseOpen, setPauseOpen] = React.useState(false)
    const [pauseReasonOpen, setPauseReasonOpen] = React.useState(false)
    const [pauseReason, setPauseReason] = React.useState("")
    const [lastResult, setLastResult] = React.useState<ResultState>(null)
    const requestIdRef = React.useRef<string | null>(null)

    const data = detailQuery.data
    const dirty = sessionEdit != null

    // Session-only: no localStorage / draft mutation; warn before unload
    React.useEffect(() => {
        if (!dirty) return
        const onBeforeUnload = (e: BeforeUnloadEvent) => {
            e.preventDefault()
            e.returnValue = "当前输入尚未提交，刷新后将丢失。"
        }
        window.addEventListener("beforeunload", onBeforeUnload)
        return () => window.removeEventListener("beforeunload", onBeforeUnload)
    }, [dirty])

    React.useEffect(() => {
        if (!data) return
        const el = document.getElementById(`pub-section-${section}`)
        if (el) el.scrollIntoView({ block: "start", behavior: "smooth" })
    }, [section, data])

    // revision 参数归一：指向不存在或已是最新的历史修订时清理 URL 残留
    React.useEffect(() => {
        if (!data || !revisionParam) return
        const known = data.revisions.some((r) => r.revisionId === revisionParam)
        if (!known || revisionParam === data.latestRevisionId) {
            const sp = new URLSearchParams(searchParams.toString())
            sp.delete("revision")
            const qs = sp.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname)
        }
    }, [data, pathname, revisionParam, router, searchParams])

    const isViewingHistoricalRevision =
        revisionParam != null &&
        data != null &&
        revisionParam !== data.latestRevisionId &&
        data.revisions.some((r) => r.revisionId === revisionParam)

    const form = useAppForm({
        defaultValues: {
            name: "",
            specification: "",
            salesDescription: "",
            minimumPurchaseQuantity: "1",
            salesPriceGross: "",
            salesTaxRate: "0.13",
            categoryId: "",
            skuRevisionId: "",
            supplierOfferingRevisionId: "",
            baseUnitCode: "",
            salesRegionText: "",
            productCapabilitiesText: "",
            validFrom: "",
            validTo: "",
            media: [] as Array<{
                fileAssetId: string
                mediaRole: "MAIN" | "CAROUSEL" | "DETAIL"
                sortNo: number
                altText: string
            }>,
            saleStatus: "ON_SALE" as SaleStatus,
        },
        validators: { onChange: publishSchema },
        onSubmit: async () => {
            setConfirmOpen(true)
        },
    })

    const startPrepareRevision = React.useCallback(() => {
        if (!data) return
        const base = data.selectedRevision
        const edit: SessionEdit = {
            baselineRevisionId: base.revisionId,
            name: base.name,
            specification: base.specification,
            salesDescription: base.salesDescription,
            minimumPurchaseQuantity: base.minimumPurchaseQuantity,
            salesPriceGross: base.salesPriceGross,
            salesTaxRate: base.salesTaxRate,
            saleStatus:
                base.saleStatus === "PAUSED" ? "PAUSED" : base.saleStatus,
            baseUnitCode: base.baseUnitCode,
            salesRegion:
                base.salesRegion ??
                base.salesRegionLabel
                    .split(/[、，,]/)
                    .map((entry) => entry.trim())
                    .filter(Boolean),
            categoryId: base.categoryId,
            skuRevisionId: base.skuRevisionId,
            supplierOfferingRevisionId: base.supplierOfferingRevisionId,
            productCapabilities: [...base.productCapabilities],
            validFrom: new Date().toISOString(),
            media: base.media.map((m) => ({ ...m })),
        }
        setSessionEdit(edit)
        form.reset({
            name: edit.name,
            specification: edit.specification,
            salesDescription: edit.salesDescription,
            minimumPurchaseQuantity: edit.minimumPurchaseQuantity,
            salesPriceGross: edit.salesPriceGross,
            salesTaxRate: edit.salesTaxRate,
            categoryId: edit.categoryId,
            skuRevisionId: edit.skuRevisionId,
            supplierOfferingRevisionId: edit.supplierOfferingRevisionId,
            baseUnitCode: edit.baseUnitCode,
            salesRegionText: edit.salesRegion.join("、"),
            productCapabilitiesText: edit.productCapabilities.join("、"),
            validFrom: edit.validFrom,
            validTo: edit.validTo ?? "",
            media: edit.media.map((media) => ({
                fileAssetId: media.fileAssetId,
                mediaRole: media.mediaRole,
                sortNo: media.sortNo,
                altText: media.altText,
            })),
            saleStatus: edit.saleStatus,
        })
        setLastResult(null)
        requestIdRef.current = null
    }, [data, form])

    const discardSession = () => {
        if (
            sessionEdit &&
            !window.confirm("放弃本次输入？未提交内容将丢失，不会保存草稿。")
        ) {
            return
        }
        setSessionEdit(null)
        setConfirmOpen(false)
    }

    /** 返回类导航在 dirty 时先确认，避免站内跳转无声丢弃未提交输入 */
    const goBackToList = () => {
        if (
            dirty &&
            !window.confirm("当前输入尚未提交，返回列表将丢失本次未提交内容。")
        ) {
            return
        }
        router.push("/commerce/publications")
    }

    const setSection = (id: SectionId) => {
        const sp = new URLSearchParams(searchParams.toString())
        if (id === "overview") sp.delete("section")
        else sp.set("section", id)
        const qs = sp.toString()
        router.replace(qs ? `${pathname}?${qs}` : pathname)
    }

    const selectRevision = (revisionId: string) => {
        if (dirty) {
            if (
                !window.confirm(
                    "切换历史修订将放弃本次未提交输入。输入仅存在于当前页签，不会保存草稿。",
                )
            ) {
                return
            }
            setSessionEdit(null)
        }
        const sp = new URLSearchParams(searchParams.toString())
        sp.set("section", "delivery")
        sp.set("revision", revisionId)
        router.replace(`${pathname}?${sp.toString()}`)
    }

    const doPublish = async () => {
        if (!data || !sessionEdit) return
        if (!canPublish || gateBlocks || pausedOnSale) {
            setLastResult({
                status: "blocked",
                title: "提交被阻断",
                description:
                    publishBlocker?.message ?? "当前状态不允许提交发布。",
            })
            return
        }
        const values = form.state.values
        if (!requestIdRef.current) {
            requestIdRef.current = `w22-pub-${data.identity.publicationId}-${Date.now()}`
        }
        const command = {
            publicationId: data.identity.publicationId,
            expectedObjectVersion: data.objectVersion,
            expectedPublishGateVersion: data.publishGate.gateVersion,
            requestId: requestIdRef.current,
            content: {
                skuRevisionId: values.skuRevisionId.trim(),
                supplierOfferingRevisionId:
                    values.supplierOfferingRevisionId.trim(),
                categoryId: values.categoryId.trim(),
                name: values.name.trim(),
                specification: values.specification.trim(),
                salesDescription: values.salesDescription.trim(),
                minimumPurchaseQuantity: values.minimumPurchaseQuantity.trim(),
                salesPriceGross: values.salesPriceGross.trim(),
                salesTaxRate: values.salesTaxRate.trim(),
                baseUnitCode: values.baseUnitCode.trim(),
                salesRegion: values.salesRegionText
                    .split(/[、，,]/)
                    .map((entry) => entry.trim())
                    .filter(Boolean),
                saleStatus: values.saleStatus,
                productCapabilities: values.productCapabilitiesText
                    .split(/[、，,]/)
                    .map((entry) => entry.trim())
                    .filter(Boolean),
                validFrom: values.validFrom,
                validTo: values.validTo || undefined,
                media: values.media.map((m) => ({
                    fileAssetId: m.fileAssetId,
                    mediaRole: m.mediaRole,
                    sortNo: m.sortNo,
                    altText: m.altText,
                })),
            },
        }
        const result = await publishMutation.mutateAsync(command)
        setConfirmOpen(false)
        if (result.status === "succeeded") {
            setLastResult({
                status: "succeeded",
                title: "发布修订已提交，等待商城确认",
                description:
                    "已形成新的发布版本并开始发送。商城确认前不会显示为「商城已生效」。",
                reference: result.operationId,
                facts: [
                    { label: "发布版本", value: `r${result.revisionNo}` },
                    { label: "修订编号", value: result.revisionId },
                    { label: "发送编号", value: result.deliveryId },
                    { label: "发送状态", value: "待发送" },
                ],
            })
            setSessionEdit(null)
            requestIdRef.current = null
            return
        }
        if (result.status === "unknown") {
            setLastResult({
                status: "unknown",
                title: "发布结果未知",
                description: result.message,
                reference: result.requestId,
            })
            return
        }
        setLastResult({
            status: "blocked",
            title: "发布被阻断",
            description: result.message,
            reference: result.code,
        })
    }

    const doPause = async () => {
        if (!data || !pauseReason.trim()) return
        const result = await pauseMutation.mutateAsync({
            publicationId: data.identity.publicationId,
            expectedObjectVersion: data.objectVersion,
            requestId: `w22-pause-${Date.now()}`,
            reason: pauseReason.trim(),
        })
        setPauseOpen(false)
        if (result.status === "succeeded") {
            setLastResult({
                status: "succeeded",
                title: "人工暂停修订已提交",
                description: "已形成暂停发布修订并进入发送。",
                facts: [
                    { label: "发布版本", value: `r${result.revisionNo}` },
                    { label: "发送编号", value: result.deliveryId },
                ],
            })
            setPauseReason("")
            return
        }
        if (result.status === "unknown") {
            setLastResult({
                status: "unknown",
                title: "暂停结果未知",
                description: result.message,
            })
            return
        }
        setLastResult({
            status: "blocked",
            title: "暂停被阻断",
            description: result.message,
        })
    }

    if (detailQuery.isPending) {
        return (
            <PageScaffold>
                <PageHeader title="商品发布" description="正在加载…" />
                <div className="space-y-3" aria-busy="true" aria-label="加载中">
                    <div className="h-16 animate-pulse rounded-lg bg-muted" />
                    <div className="h-40 animate-pulse rounded-lg bg-muted" />
                </div>
            </PageScaffold>
        )
    }

    if (detailQuery.isError) {
        return (
            <PageScaffold>
                <PageHeader title="商品发布" />
                <BusinessFailureState
                    error={detailQuery.error}
                    action={
                        <Button
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={() => void detailQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (!data) {
        return (
            <PageScaffold>
                <BusinessEmptyState
                    kind="no-data"
                    title="发布对象不存在"
                    description="该发布对象不存在，或当前账号无权查看。"
                    action={
                        <Button
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            render={<Link href="/commerce/publications" />}
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const canPrepare =
        data.allowedActions.includes("PREPARE_REVISION") && !dirty
    const canPublish = data.allowedActions.includes("PUBLISH")
    const canPause = data.allowedActions.includes("PAUSE")
    const publishBlocker = data.actionBlockers.find(
        (b) => b.action === "PUBLISH",
    )
    const gateBlocks =
        data.publishGate.kind === "REVIEW_POLICY_UNCONFIGURED" ||
        data.publishGate.kind === "RECOVERY_RESPONSIBILITY_UNCONFIRMED" ||
        data.publishGate.kind === "REVIEW_BLOCKED"
    /** 安全暂停 + 选择上架时提交必被阻断（页头按钮与表单提交共用同一组条件） */
    const pausedOnSale =
        data.status === "SAFETY_PAUSED" &&
        form.state.values.saleStatus === "ON_SALE"
    const publishBlocked = !canPublish || gateBlocks || pausedOnSale

    const ackedLabel =
        data.currentAckedRevisionNo != null
            ? `r${data.currentAckedRevisionNo}`
            : "尚未生效"
    const latestLabel =
        data.latestRevisionNo != null ? `r${data.latestRevisionNo}` : "—"

    const deliveryTrack = data.deliveries.find(
        (d) => d.revisionId === data.latestRevisionId,
    )

    return (
        <PageScaffold>
            <PageHeader
                variant="object-chrome"
                breadcrumbs={[
                    {
                        id: "com",
                        label: "商城与发布",
                        href: "/commerce/publications",
                    },
                    {
                        id: "list",
                        label: "商品发布",
                        href: "/commerce/publications",
                    },
                    {
                        id: "obj",
                        label: data.identity.skuCode,
                        current: true,
                    },
                ]}
                metadata={
                    <DataFreshness
                        updatedAt="详情"
                        dateTime={data.freshness.queriedAt}
                        state={detailQuery.isFetching ? "syncing" : "fresh"}
                        label="发布信息更新于"
                    />
                }
                actions={
                    <div className="flex flex-wrap gap-2">
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={goBackToList}
                        >
                            <ArrowLeftIcon />
                            返回列表
                        </Button>
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            disabled={detailQuery.isFetching}
                            onClick={() => void detailQuery.refetch()}
                        >
                            <RefreshCwIcon
                                className={
                                    detailQuery.isFetching
                                        ? "animate-spin"
                                        : undefined
                                }
                            />
                            刷新
                        </Button>
                    </div>
                }
            />

            {dirty ? (
                <Alert variant="warning" role="status">
                    <AlertTitle>本次编辑 · 未保存</AlertTitle>
                    <AlertDescription>
                        当前输入仅保存在当前页面，无草稿保存、无自动保存。刷新或关闭前将提示丢失。
                        <div className="mt-2 flex gap-2">
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={discardSession}
                            >
                                放弃输入
                            </Button>
                        </div>
                    </AlertDescription>
                </Alert>
            ) : null}

            {lastResult ? (
                <FormalActionResult
                    status={
                        lastResult.status === "failed"
                            ? "blocked"
                            : lastResult.status
                    }
                    title={lastResult.title}
                    description={lastResult.description}
                    reference={lastResult.reference}
                    facts={lastResult.facts}
                />
            ) : null}

            <DocumentHeader
                density="compact"
                title={data.selectedRevision.name}
                documentNumber={data.identity.publicationCode}
                primaryStatus={{
                    label: data.statusLabel,
                    tone: data.statusTone,
                }}
                version={
                    data.latestRevisionNo != null
                        ? `最新 r${data.latestRevisionNo}`
                        : undefined
                }
                meta={
                    <span className="text-muted-foreground">
                        {data.identity.skuCode} · {data.identity.targetMallName}
                    </span>
                }
                statuses={[
                    {
                        id: "content",
                        label: "发布内容",
                        status: {
                            label:
                                data.latestRevisionNo != null
                                    ? `r${data.latestRevisionNo}`
                                    : "无",
                            tone: "info",
                        },
                    },
                    {
                        id: "delivery",
                        label: "发送",
                        status: {
                            label: deliveryTrack?.statusLabel ?? "无发送",
                            tone: deliveryTrack?.statusTone ?? "neutral",
                        },
                    },
                    {
                        id: "ack",
                        label: "商城确认",
                        status: {
                            label:
                                data.currentAckedRevisionNo != null
                                    ? `已生效 r${data.currentAckedRevisionNo}`
                                    : "尚未生效",
                            tone:
                                data.currentAckedRevisionNo != null
                                    ? "success"
                                    : "warning",
                        },
                    },
                ]}
                primaryAction={
                    dirty ? (
                        <Button
                            type="button"
                            size="sm"
                            disabled={
                                publishBlocked || publishMutation.isPending
                            }
                            title={
                                publishBlocked
                                    ? (publishBlocker?.message ??
                                      "当前状态不允许提交发布")
                                    : undefined
                            }
                            onClick={() => void form.handleSubmit()}
                        >
                            {publishMutation.isPending ? (
                                <LoaderCircleIcon className="animate-spin" />
                            ) : (
                                <SendIcon />
                            )}
                            提交发布
                        </Button>
                    ) : (
                        <Button
                            type="button"
                            size="sm"
                            disabled={!canPrepare}
                            title={
                                canPrepare
                                    ? undefined
                                    : "当前角色无权准备新版本"
                            }
                            onClick={startPrepareRevision}
                        >
                            <HistoryIcon />
                            准备新版本
                        </Button>
                    )
                }
                secondaryActions={
                    canPause ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => {
                                setPauseReason("")
                                setPauseReasonOpen(true)
                            }}
                        >
                            <PauseIcon />
                            人工暂停
                        </Button>
                    ) : undefined
                }
            />

            {/* 一屏识别：稳定发布 / 商城生效版 / 最新待确认版 */}
            <Card className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30 pb-2">
                    <CardTitle className="text-base">发布身份与版本</CardTitle>
                    <CardDescription>
                        展示商城实际生效版本与最新提交版本，避免误判。
                    </CardDescription>
                </CardHeader>
                <CardContent>
                    <StatusTrackSummary
                        variant="table"
                        tracks={[
                            {
                                id: "stable",
                                label: "稳定发布",
                                status: {
                                    label: data.identity.publicationCode,
                                    tone: "neutral",
                                    description: `${data.identity.skuCode} · ${data.identity.targetMallName}`,
                                },
                            },
                            {
                                id: "acked",
                                label: "当前商城生效版",
                                status: {
                                    label: ackedLabel,
                                    tone:
                                        data.currentAckedRevisionNo != null
                                            ? "success"
                                            : "warning",
                                    description:
                                        data.currentAckedRevisionNo != null
                                            ? "商城已成功确认"
                                            : "商城确认前不显示已生效",
                                },
                            },
                            {
                                id: "latest",
                                label: "最新发布版",
                                status: {
                                    label: latestLabel,
                                    tone:
                                        data.latestRevisionNo ===
                                        data.currentAckedRevisionNo
                                            ? "success"
                                            : "info",
                                    description:
                                        data.currentAckedRevisionNo != null &&
                                        data.latestRevisionNo !==
                                            data.currentAckedRevisionNo
                                            ? "等待商城确认"
                                            : "与商城生效版一致或尚无待确认",
                                },
                            },
                        ]}
                    />
                    {/* hasPendingConfirmation is on row not view — derive */}
                    {data.currentAckedRevisionNo != null &&
                    data.latestRevisionNo !== data.currentAckedRevisionNo ? (
                        <p className="mt-2 text-xs text-muted-foreground">
                            最新 r{data.latestRevisionNo}{" "}
                            尚未被商城确认，商城生效前仍按待确认处理。
                        </p>
                    ) : null}
                </CardContent>
            </Card>

            <nav
                role="group"
                aria-label="发布对象锚点"
                className="sticky top-0 z-10 inline-flex flex-wrap rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
            >
                {SECTIONS.map((s) => (
                    <button
                        key={s.id}
                        type="button"
                        aria-pressed={section === s.id}
                        onClick={() => setSection(s.id)}
                        className={cn(
                            "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all",
                            section === s.id
                                ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                                : "text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                        )}
                    >
                        {s.label}
                    </button>
                ))}
            </nav>

            {data.safetyPause ? (
                <div id="pub-section-overview-safety">
                    <SafetyPausePanel
                        pause={data.safetyPause}
                        sourceObjectLabel={`${data.selectedRevision.fixedOffering.supplierName} · ${data.identity.skuCode}`}
                        affectedPublicationLabels={{
                            [data.identity.publicationId]:
                                data.identity.publicationCode,
                        }}
                    />
                </div>
            ) : null}

            <PublishGateAlert gate={data.publishGate} />

            <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(16rem,20rem)]">
                <div
                    className={cn(
                        surfacePanelClassName,
                        "min-w-0 space-y-6 p-3 md:p-4",
                    )}
                >
                    <DocumentSection
                        id="pub-section-overview"
                        title="概览"
                        description="安全暂停、责任人与阻塞原因"
                    >
                        <DocumentSummary
                            columns="two"
                            items={[
                                {
                                    id: "code",
                                    label: "发布编号",
                                    value: (
                                        <span className="num">
                                            {data.identity.publicationCode}
                                        </span>
                                    ),
                                },
                                {
                                    id: "owner",
                                    label: "负责人",
                                    value: data.ownerLabel,
                                },
                                {
                                    id: "selRev",
                                    label: "当前选中修订",
                                    value: (
                                        <span className="num">
                                            r{data.selectedRevision.revisionNo}
                                        </span>
                                    ),
                                },
                                {
                                    id: "offering",
                                    label: "固定供给",
                                    value: data.selectedRevision.fixedOffering
                                        .supplierName,
                                },
                            ]}
                        />
                        {publishBlocker ? (
                            <Alert variant="warning" className="mt-3">
                                <AlertTitle>动作阻断</AlertTitle>
                                <AlertDescription>
                                    {publishBlocker.message}
                                </AlertDescription>
                            </Alert>
                        ) : null}
                    </DocumentSection>

                    <DocumentSection
                        id="pub-section-content"
                        title="发布内容"
                        description="选中修订的完整商城内容记录"
                    >
                        {dirty ? (
                            <form
                                className="space-y-3"
                                onSubmit={(e) => {
                                    e.preventDefault()
                                    void form.handleSubmit()
                                }}
                            >
                                <Alert variant="info">
                                    <AlertTitle>
                                        基于历史/当前版本的本次编辑
                                    </AlertTitle>
                                    <AlertDescription>
                                        基于 r{data.selectedRevision.revisionNo}{" "}
                                        版本开始编辑。最小购买量需运营确认填写，不随供应商起订量带入；销售价与供货价分开填写。
                                    </AlertDescription>
                                </Alert>
                                <form.AppField name="name">
                                    {(field) => (
                                        <field.TextField label="展示名称" />
                                    )}
                                </form.AppField>
                                <form.AppField name="specification">
                                    {(field) => (
                                        <field.TextField label="规格" />
                                    )}
                                </form.AppField>
                                <form.AppField name="salesDescription">
                                    {(field) => (
                                        <field.TextareaField
                                            label="商城销售说明"
                                            rows={3}
                                        />
                                    )}
                                </form.AppField>
                                <div className="grid gap-3 sm:grid-cols-2">
                                    <form.AppField name="salesPriceGross">
                                        {(field) => (
                                            <field.TextField label="含税销售价" />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="salesTaxRate">
                                        {(field) => (
                                            <field.TextField label="销项税率" />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="minimumPurchaseQuantity">
                                        {(field) => (
                                            <field.TextField label="最小购买量（运营确认）" />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="saleStatus">
                                        {(field) => (
                                            <div className="space-y-1.5">
                                                <Label htmlFor="saleStatus">
                                                    商城销售状态
                                                </Label>
                                                <OptionCombobox
                                                    id="saleStatus"
                                                    value={field.state.value}
                                                    onValueChange={(v) =>
                                                        field.handleChange(
                                                            (v ??
                                                                field.state
                                                                    .value) as SaleStatus,
                                                        )
                                                    }
                                                    options={[
                                                        {
                                                            value: "ON_SALE",
                                                            label: SALE_STATUS_LABEL.ON_SALE,
                                                        },
                                                        {
                                                            value: "OFF_SALE",
                                                            label: SALE_STATUS_LABEL.OFF_SALE,
                                                        },
                                                        {
                                                            value: "PAUSED",
                                                            label: SALE_STATUS_LABEL.PAUSED,
                                                        },
                                                    ]}
                                                    className="w-full"
                                                    allowClear={false}
                                                    aria-label="商城销售状态"
                                                    placeholder="商城销售状态"
                                                />
                                                {data.status ===
                                                    "SAFETY_PAUSED" &&
                                                field.state.value ===
                                                    "ON_SALE" ? (
                                                    <p className="text-xs text-destructive">
                                                        安全暂停中的对象提交上架会被系统阻断。
                                                    </p>
                                                ) : null}
                                            </div>
                                        )}
                                    </form.AppField>
                                </div>
                                <div className="grid gap-3 sm:grid-cols-2">
                                    <form.AppField name="skuRevisionId">
                                        {(field) => (
                                            <field.TextField label="SKU 修订编号" />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="categoryId">
                                        {(field) => (
                                            <field.TextField label="商城类目编号" />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="supplierOfferingRevisionId">
                                        {(field) => (
                                            <field.TextField label="唯一固定供给修订编号" />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="baseUnitCode">
                                        {(field) => (
                                            <field.TextField label="基础单位代码" />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="salesRegionText">
                                        {(field) => (
                                            <field.TextField label="可销售区域（顿号/逗号分隔）" />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="productCapabilitiesText">
                                        {(field) => (
                                            <field.TextField label="商品能力（顿号/逗号分隔）" />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="validFrom">
                                        {(field) => (
                                            <field.TextField label="生效时间" />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="validTo">
                                        {(field) => (
                                            <field.TextField label="失效时间（可空）" />
                                        )}
                                    </form.AppField>
                                </div>
                                <div className="space-y-2 rounded-lg bg-muted/40 p-3">
                                    <div className="text-sm font-medium">
                                        发布媒体资料
                                    </div>
                                    {sessionEdit?.media.map((media, index) => (
                                        <div
                                            key={media.fileAssetId}
                                            className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_2fr]"
                                        >
                                            <div className="text-xs text-muted-foreground">
                                                {
                                                    MEDIA_ROLE_LABEL[
                                                        media.mediaRole
                                                    ]
                                                }{" "}
                                                · 顺序 {media.sortNo}
                                            </div>
                                            <form.AppField
                                                name={`media[${index}].altText`}
                                            >
                                                {(field) => (
                                                    <field.TextField label="图片说明" />
                                                )}
                                            </form.AppField>
                                        </div>
                                    ))}
                                </div>
                                <p className="text-xs text-muted-foreground">
                                    供应商起订{" "}
                                    {data.selectedRevision.fixedOffering
                                        .supplierMoq ?? "—"}
                                    （只读展示，不复制到商城最小购买量）。供给修订、区域、能力和媒体变化都会形成新发布修订。
                                </p>
                                <div className="flex flex-wrap gap-2">
                                    <form.AppForm>
                                        <form.SubmitButton
                                            label="核对并提交发布"
                                            disabled={publishBlocked}
                                        />
                                    </form.AppForm>
                                    <Button
                                        type="button"
                                        variant="outline"
                                        onClick={discardSession}
                                    >
                                        放弃
                                    </Button>
                                    {publishBlocked ? (
                                        <span className="text-xs text-destructive">
                                            {publishBlocker?.message ??
                                                "当前状态不允许提交发布。"}
                                        </span>
                                    ) : null}
                                </div>
                            </form>
                        ) : (
                            <RevisionContent
                                rev={data.selectedRevision}
                                fieldPermissions={data.fieldPermissions}
                            />
                        )}
                    </DocumentSection>

                    <DocumentSection
                        id="pub-section-media"
                        title="媒体"
                        description="主图、轮播、详情图及替代文本"
                    >
                        <ul className="grid gap-2 sm:grid-cols-3">
                            {data.selectedRevision.media.map((m) => (
                                <li
                                    key={`${m.fileAssetId}-${m.mediaRole}-${m.sortNo}`}
                                    className="rounded-lg bg-muted/40 p-3 text-sm"
                                >
                                    <div className="mb-2 flex size-full min-h-20 items-center justify-center rounded bg-muted text-xs text-muted-foreground">
                                        {MEDIA_ROLE_LABEL[m.mediaRole]}
                                    </div>
                                    <div className="font-medium">
                                        {MEDIA_ROLE_LABEL[m.mediaRole]}
                                    </div>
                                    <div className="text-xs text-muted-foreground">
                                        {m.altText}
                                    </div>
                                </li>
                            ))}
                        </ul>
                    </DocumentSection>

                    <DocumentSection
                        id="pub-section-offering"
                        title="固定供给"
                        description="本版本唯一履约来源"
                    >
                        <Card className="border-0 bg-muted/40 shadow-none ring-0">
                            <CardContent className="space-y-2 pt-4 text-sm">
                                <div>
                                    供应商{" "}
                                    {
                                        data.selectedRevision.fixedOffering
                                            .supplierName
                                    }
                                </div>
                                <div>
                                    可供状态{" "}
                                    {
                                        data.selectedRevision.fixedOffering
                                            .availabilityLabel
                                    }
                                </div>
                                <div>
                                    供货价{" "}
                                    {data.fieldPermissions.supplyPriceGross ===
                                        "masked" ||
                                    !data.selectedRevision.fixedOffering
                                        .supplyPriceVisible
                                        ? "******"
                                        : data.selectedRevision.fixedOffering
                                                .supplyPriceGross
                                          ? `¥${data.selectedRevision.fixedOffering.supplyPriceGross}`
                                          : "—"}
                                </div>
                                <p className="text-xs text-muted-foreground">
                                    每次发布对应一个固定供给版本；修改图片、供给、价格或销售状态都会形成新版本，不覆盖历史。
                                </p>
                            </CardContent>
                        </Card>
                    </DocumentSection>

                    <DocumentSection
                        id="pub-section-delivery"
                        title="发送与版本"
                        description="各版本发送与商城确认时间线"
                        action={
                            isViewingHistoricalRevision ? (
                                <Button
                                    type="button"
                                    size="xs"
                                    variant="outline"
                                    onClick={() => {
                                        const sp = new URLSearchParams(
                                            searchParams.toString(),
                                        )
                                        sp.delete("revision")
                                        const qs = sp.toString()
                                        router.replace(
                                            qs ? `${pathname}?${qs}` : pathname,
                                        )
                                    }}
                                >
                                    回到最新版本
                                </Button>
                            ) : undefined
                        }
                    >
                        <RevisionTimeline
                            revisions={data.revisions
                                .slice()
                                .reverse()
                                .map((r) => ({
                                    id: r.revisionId,
                                    version: r.revisionNo,
                                    source: "erp-change" as const,
                                    actor: r.createdBy,
                                    effectiveAt: {
                                        dateTime: r.createdAt,
                                        label: formatDateTime(
                                            r.createdAt,
                                            "default",
                                        ),
                                    },
                                    reason: r.deliverySummary,
                                    status: {
                                        label: r.saleStatusLabel,
                                        tone: r.isMallAcked
                                            ? ("success" as const)
                                            : r.isLatest
                                              ? ("info" as const)
                                              : ("neutral" as const),
                                    },
                                    isCurrent:
                                        r.revisionId ===
                                        data.selectedRevision.revisionId,
                                    action: (
                                        <Button
                                            type="button"
                                            size="xs"
                                            variant="outline"
                                            onClick={() =>
                                                selectRevision(r.revisionId)
                                            }
                                        >
                                            查看历史记录
                                        </Button>
                                    ),
                                }))}
                        />
                        <Separator className="my-4" />
                        <div className="space-y-2">
                            <div className="text-sm font-medium">发送记录</div>
                            {data.deliveries.length === 0 ? (
                                <p className="text-sm text-muted-foreground">
                                    暂无发送
                                </p>
                            ) : (
                                <ul className="space-y-2">
                                    {data.deliveries
                                        .slice()
                                        .reverse()
                                        .map((d) => (
                                            <li
                                                key={d.deliveryId}
                                                className={cn(
                                                    "rounded-lg p-3 text-sm ring-1",
                                                    d.revisionId ===
                                                        data.selectedRevision
                                                            .revisionId
                                                        ? "bg-primary/5 ring-primary/40"
                                                        : "bg-muted/40 ring-transparent",
                                                )}
                                            >
                                                <div className="flex flex-wrap items-center justify-between gap-2">
                                                    <div>
                                                        <span className="num font-medium">
                                                            {d.deliveryId}
                                                        </span>
                                                        <span className="mx-2 text-muted-foreground">
                                                            r{d.revisionNo}
                                                        </span>
                                                        <BusinessStatusBadge
                                                            context="list"
                                                            label={
                                                                d.statusLabel
                                                            }
                                                            tone={d.statusTone}
                                                        />
                                                    </div>
                                                    {d.status === "FAILED" ? (
                                                        <Button
                                                            type="button"
                                                            size="xs"
                                                            variant="outline"
                                                            disabled={
                                                                !data.allowedActions.includes(
                                                                    "RETRY_DELIVERY",
                                                                ) ||
                                                                retryMutation.isPending
                                                            }
                                                            onClick={async () => {
                                                                const r =
                                                                    await retryMutation.mutateAsync(
                                                                        {
                                                                            publicationId:
                                                                                data
                                                                                    .identity
                                                                                    .publicationId,
                                                                            deliveryId:
                                                                                d.deliveryId,
                                                                            requestId: `w22-retry-${Date.now()}`,
                                                                        },
                                                                    )
                                                                if (
                                                                    r.status ===
                                                                    "succeeded"
                                                                ) {
                                                                    setLastResult(
                                                                        {
                                                                            status: "succeeded",
                                                                            title: "已发起重试发送",
                                                                            description: `继续发送，尝试次数 ${r.attemptCount}。`,
                                                                            facts: [
                                                                                {
                                                                                    label: "发送编号",
                                                                                    value: r.deliveryId,
                                                                                },
                                                                                {
                                                                                    label: "状态",
                                                                                    value: r.deliveryStatus,
                                                                                },
                                                                            ],
                                                                        },
                                                                    )
                                                                } else if (
                                                                    r.status ===
                                                                    "blocked"
                                                                ) {
                                                                    setLastResult(
                                                                        {
                                                                            status: "blocked",
                                                                            title: "无法重试",
                                                                            description:
                                                                                r.message,
                                                                        },
                                                                    )
                                                                }
                                                            }}
                                                        >
                                                            {retryMutation.isPending ? (
                                                                <LoaderCircleIcon className="animate-spin" />
                                                            ) : null}
                                                            重试发送
                                                        </Button>
                                                    ) : null}
                                                    {d.status === "HANDOFF" ? (
                                                        <Button
                                                            type="button"
                                                            size="xs"
                                                            variant="outline"
                                                            render={
                                                                <Link
                                                                    href={`/governance/integration-errors?q=${encodeURIComponent(d.deliveryId)}`}
                                                                />
                                                            }
                                                        >
                                                            {goToWorkspaceLabel(
                                                                "W29",
                                                            )}
                                                        </Button>
                                                    ) : null}
                                                </div>
                                                <div className="mt-1 text-xs text-muted-foreground">
                                                    尝试 {d.attemptCount}
                                                    {d.lastAttemptAt
                                                        ? ` · 最近 ${formatDateTime(d.lastAttemptAt, "default")}`
                                                        : ""}
                                                    {d.mallAckAt
                                                        ? ` · 商城确认 ${formatDateTime(d.mallAckAt, "default")}`
                                                        : ""}
                                                    {d.mallVersion ? (
                                                        <>
                                                            {" · 商城版本 "}
                                                            <span className="num">
                                                                {d.mallVersion}
                                                            </span>
                                                        </>
                                                    ) : null}
                                                </div>
                                                {d.errorSummary ? (
                                                    <p className="mt-1 text-xs text-destructive">
                                                        {d.errorSummary}
                                                    </p>
                                                ) : null}
                                            </li>
                                        ))}
                                </ul>
                            )}
                        </div>
                    </DocumentSection>

                    <DocumentSection
                        id="pub-section-audit"
                        title="审计"
                        description="创建、提交、暂停与处理记录摘要"
                    >
                        <ul className="space-y-2 text-sm">
                            {data.revisions
                                .slice()
                                .reverse()
                                .map((r) => (
                                    <li
                                        key={r.revisionId}
                                        className="flex flex-wrap justify-between gap-2 border-b border-border/30 py-2"
                                    >
                                        <span>
                                            r{r.revisionNo} · {r.createdBy} ·{" "}
                                            {r.saleStatusLabel}
                                        </span>
                                        <span className="num text-xs text-muted-foreground">
                                            {formatDateTime(
                                                r.createdAt,
                                                "default",
                                            )}
                                        </span>
                                    </li>
                                ))}
                        </ul>
                    </DocumentSection>
                </div>

                <aside className="min-w-0 space-y-3 xl:sticky xl:top-14 xl:self-start">
                    <Card size="sm" className={surfacePanelClassName}>
                        <CardHeader className="border-b border-border/30 pb-2">
                            <CardTitle className="text-sm">选中修订</CardTitle>
                        </CardHeader>
                        <CardContent className="space-y-1 text-sm">
                            <div className="num font-medium">
                                r{data.selectedRevision.revisionNo}
                            </div>
                            <div className="text-xs text-muted-foreground">
                                {data.selectedRevision.createdBy} ·{" "}
                                {formatDateTime(
                                    data.selectedRevision.createdAt,
                                    "default",
                                )}
                            </div>
                            <BusinessStatusBadge
                                context="preview"
                                label={data.selectedRevision.saleStatusLabel}
                                tone="neutral"
                            />
                            <div className="pt-2 text-xs">
                                供给{" "}
                                {
                                    data.selectedRevision.fixedOffering
                                        .supplierName
                                }
                            </div>
                        </CardContent>
                    </Card>
                    <Card size="sm" className={surfacePanelClassName}>
                        <CardHeader className="border-b border-border/30 pb-2">
                            <CardTitle className="text-sm">版本对照</CardTitle>
                        </CardHeader>
                        <CardContent className="space-y-2 text-sm">
                            <div className="flex justify-between">
                                <span className="text-muted-foreground">
                                    商城生效
                                </span>
                                <span className="num">{ackedLabel}</span>
                            </div>
                            <div className="flex justify-between">
                                <span className="text-muted-foreground">
                                    最新发布
                                </span>
                                <span className="num">{latestLabel}</span>
                            </div>
                        </CardContent>
                    </Card>
                </aside>
            </div>

            <FormalActionConfirmDialog
                open={confirmOpen}
                onOpenChange={setConfirmOpen}
                actionLabel="提交发布"
                confirmLabel="确认提交"
                title="确认提交发布"
                description="提交后将形成新发布版本并发送至目标商城，进入「等待商城确认」。"
                fromStatus={{ label: "本次编辑", tone: "warning" }}
                toStatus={{ label: "待商城确认", tone: "info" }}
                lockedFields={
                    sessionEdit
                        ? [
                              `目标商城 ${data.identity.targetMallName}`,
                              `含税销售价 ¥${form.state.values.salesPriceGross}`,
                              `销售状态 ${SALE_STATUS_LABEL[form.state.values.saleStatus]}`,
                              `固定供给 ${data.selectedRevision.fixedOffering.supplierName}`,
                              `最小购买量 ${form.state.values.minimumPurchaseQuantity}`,
                          ]
                        : []
                }
                effects={[
                    "形成新的发布版本并发送",
                    "商城确认前不显示为商城已生效",
                    "不覆盖历史修订",
                ]}
                nextDepartment="商城接收确认"
                irreversibleEffects={["写入新的发布版本号与发送编号"]}
                pending={publishMutation.isPending}
                onConfirm={() => void doPublish()}
            />

            <FormalActionConfirmDialog
                open={pauseOpen}
                onOpenChange={setPauseOpen}
                actionLabel="人工暂停"
                confirmLabel="确认暂停"
                title="确认人工暂停"
                description="将形成新的暂停发布修订并发送至目标商城。"
                fromStatus={{ label: data.statusLabel, tone: data.statusTone }}
                toStatus={{ label: "已暂停", tone: "warning" }}
                lockedFields={[
                    `受影响商城 ${data.identity.targetMallName}`,
                    pauseReason.trim()
                        ? `原因 ${pauseReason.trim()}`
                        : "请填写暂停原因",
                ]}
                effects={["形成暂停修订", "发送至商城", "不覆盖历史版本"]}
                irreversibleEffects={["写入新的暂停修订与发送编号"]}
                pending={pauseMutation.isPending}
                onConfirm={() => void doPause()}
            />

            <AlertDialog
                open={pauseReasonOpen}
                onOpenChange={(open) => {
                    if (!open) setPauseReasonOpen(false)
                }}
            >
                <AlertDialogContent className="sm:max-w-md">
                    <AlertDialogHeader>
                        <AlertDialogTitle>填写暂停原因</AlertDialogTitle>
                        <AlertDialogDescription>
                            原因将随暂停修订一起记录；必填，最多 100 字。
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <div className="space-y-3">
                        <div className="flex flex-wrap gap-1.5">
                            {[
                                "价格调整",
                                "库存不足",
                                "营销调整",
                                "商品下架",
                            ].map((quick) => (
                                <Button
                                    key={quick}
                                    type="button"
                                    size="xs"
                                    variant="outline"
                                    onClick={() => setPauseReason(quick)}
                                >
                                    {quick}
                                </Button>
                            ))}
                        </div>
                        <Textarea
                            value={pauseReason}
                            onChange={(e) =>
                                setPauseReason(e.target.value.slice(0, 100))
                            }
                            placeholder="请填写暂停原因"
                            aria-label="暂停原因"
                            rows={3}
                        />
                        {pauseReason.trim() ? (
                            <p className="text-xs text-muted-foreground">
                                {pauseReason.length}/100
                            </p>
                        ) : null}
                    </div>
                    <AlertDialogFooter>
                        <AlertDialogCancel
                            onClick={() => setPauseReasonOpen(false)}
                        >
                            取消
                        </AlertDialogCancel>
                        <AlertDialogAction
                            disabled={!pauseReason.trim()}
                            onClick={() => {
                                setPauseReasonOpen(false)
                                setPauseOpen(true)
                            }}
                        >
                            下一步
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </PageScaffold>
    )
}
