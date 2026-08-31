import type { ResultState } from "@/components/business/feedback"
import type { SessionEdit } from "@/features/product-publications/lib/publish-form"
import type {
    ManualPauseCommand,
    ManualPauseResult,
    ProductPublicationView,
    PublishRevisionCommand,
    PublishRevisionResult,
    RetryDeliveryCommand,
    RetryDeliveryResult,
} from "@/features/product-publications/types"
import type { PublicationCenterFormValues } from "./publication-center-session"

type RequestIdRef = { current: string | null }

export async function performPublish(params: {
    data: ProductPublicationView
    sessionEdit: SessionEdit | null
    values: PublicationCenterFormValues
    canPublish: boolean
    gateBlocks: boolean
    pausedOnSale: boolean
    publishBlocker: { message: string } | undefined
    requestIdRef: RequestIdRef
    mutateAsync: (
        command: PublishRevisionCommand,
    ) => Promise<PublishRevisionResult>
    setConfirmOpen: (open: boolean) => void
    setLastResult: (result: ResultState) => void
    setSessionEdit: (edit: SessionEdit | null) => void
}): Promise<void> {
    const {
        data,
        sessionEdit,
        values,
        canPublish,
        gateBlocks,
        pausedOnSale,
        publishBlocker,
        requestIdRef,
        mutateAsync,
        setConfirmOpen,
        setLastResult,
        setSessionEdit,
    } = params

    if (!sessionEdit) return
    if (!canPublish || gateBlocks || pausedOnSale) {
        setLastResult({
            status: "blocked",
            title: "提交被阻断",
            description: publishBlocker?.message ?? "当前状态不允许提交发布。",
        })
        return
    }
    if (!requestIdRef.current) {
        requestIdRef.current = `w22-pub-${data.identity.publicationId}-${Date.now()}`
    }
    const command: PublishRevisionCommand = {
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
    const result = await mutateAsync(command)
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

export async function performPause(params: {
    data: ProductPublicationView
    pauseReason: string
    mutateAsync: (command: ManualPauseCommand) => Promise<ManualPauseResult>
    setPauseOpen: (open: boolean) => void
    setPauseReason: (reason: string) => void
    setLastResult: (result: ResultState) => void
}): Promise<void> {
    const {
        data,
        pauseReason,
        mutateAsync,
        setPauseOpen,
        setPauseReason,
        setLastResult,
    } = params

    if (!pauseReason.trim()) return
    const result = await mutateAsync({
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

export async function performRetry(params: {
    data: ProductPublicationView
    deliveryId: string
    mutateAsync: (command: RetryDeliveryCommand) => Promise<RetryDeliveryResult>
    setLastResult: (result: ResultState) => void
}): Promise<void> {
    const { data, deliveryId, mutateAsync, setLastResult } = params

    const result = await mutateAsync({
        publicationId: data.identity.publicationId,
        deliveryId,
        requestId: `w22-retry-${Date.now()}`,
    })
    if (result.status === "succeeded") {
        setLastResult({
            status: "succeeded",
            title: "已发起重试发送",
            description: `继续发送，尝试次数 ${result.attemptCount}。`,
            facts: [
                { label: "发送编号", value: result.deliveryId },
                { label: "状态", value: result.deliveryStatus },
            ],
        })
        return
    }
    if (result.status === "blocked") {
        setLastResult({
            status: "blocked",
            title: "无法重试",
            description: result.message,
        })
    }
}
