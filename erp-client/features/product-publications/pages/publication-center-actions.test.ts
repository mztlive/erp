import { describe, expect, it, vi } from "vitest"

import type { ResultState } from "@/components/business/feedback"
import type { SessionEdit } from "@/features/product-publications/lib/publish-form"
import type {
    ManualPauseResult,
    ProductPublicationView,
    PublishRevisionResult,
    RetryDeliveryResult,
} from "@/features/product-publications/types"
import type {
    PublicationCenterFormValues,
} from "./publication-center-session"
import {
    performPause,
    performPublish,
    performRetry,
} from "./publication-center-actions"

function makeView(
    overrides: Partial<ProductPublicationView> = {},
): ProductPublicationView {
    return {
        identity: {
            publicationId: "pub_1",
            publicationCode: "P-1001",
            skuId: "sku_1",
            skuCode: "SKU-001",
            targetMallId: "mall_1",
            targetMallName: "示例商城",
        },
        status: "MALL_LIVE",
        statusLabel: "商城已生效",
        statusTone: "success",
        latestRevisionId: "rev_2",
        latestRevisionNo: 2,
        selectedRevision: {
            revisionId: "rev_2",
            revisionNo: 2,
            skuRevisionId: "skuRev_1",
            supplierOfferingRevisionId: "off_1",
            fixedOffering: {
                offeringRevisionId: "off_1",
                supplierName: "供应商A",
                availability: "AVAILABLE",
                availabilityLabel: "可供",
                supplyPriceVisible: true,
            },
            categoryId: "cat_1",
            categoryLabel: "类目一",
            name: "示例商品",
            specification: "500ml",
            salesDescription: "商城销售说明",
            minimumPurchaseQuantity: "5",
            salesPriceGross: "19.90",
            salesTaxRate: "0.13",
            baseUnitCode: "件",
            salesRegion: ["华东"],
            salesRegionLabel: "华东",
            saleStatus: "ON_SALE",
            saleStatusLabel: "上架",
            productCapabilities: ["能力A"],
            validFrom: "2026-01-01",
            contentHash: "hash_1",
            media: [],
            createdAt: "2026-01-01T00:00:00.000Z",
            createdBy: "张三",
        },
        revisions: [],
        deliveries: [],
        publishGate: {
            kind: "READY",
            gateVersion: "g1",
            submissionKind: "NORMAL",
            priceOrTaxChanged: false,
            policyVersion: "p1",
            reviewDisposition: "NOT_REQUIRED",
        },
        freshness: {
            queriedAt: "2026-01-02T00:00:00.000Z",
            integrationUpdatedAt: "2026-01-02T00:00:00.000Z",
        },
        allowedActions: ["PREPARE_REVISION", "PUBLISH"],
        actionBlockers: [],
        fieldPermissions: {},
        objectVersion: "v1",
        ownerLabel: "张三",
        ...overrides,
    }
}

function makeSessionEdit(): SessionEdit {
    return {
        baselineRevisionId: "rev_2",
        name: "示例商品",
        specification: "500ml",
        salesDescription: "商城销售说明",
        minimumPurchaseQuantity: "5",
        salesPriceGross: "19.90",
        salesTaxRate: "0.13",
        saleStatus: "ON_SALE",
        baseUnitCode: "件",
        salesRegion: ["华东"],
        categoryId: "cat_1",
        skuRevisionId: "skuRev_1",
        supplierOfferingRevisionId: "off_1",
        productCapabilities: ["能力A"],
        validFrom: "2026-01-01T00:00:00.000Z",
        media: [],
    }
}

function makeValues(): PublicationCenterFormValues {
    return {
        name: " 示例商品 ",
        specification: "500ml",
        salesDescription: "商城销售说明",
        minimumPurchaseQuantity: "5",
        salesPriceGross: "19.90",
        salesTaxRate: "0.13",
        categoryId: "cat_1",
        skuRevisionId: "skuRev_1",
        supplierOfferingRevisionId: "off_1",
        baseUnitCode: "件",
        salesRegionText: "华东、华南",
        productCapabilitiesText: "能力A、能力B",
        validFrom: "2026-01-01",
        validTo: "",
        media: [
            {
                fileAssetId: "f_1",
                mediaRole: "MAIN",
                sortNo: 0,
                altText: "主图说明",
            },
        ],
        saleStatus: "ON_SALE",
    }
}

describe("performPublish", () => {
    it("is blocked without calling the API when the gate blocks", async () => {
        const mutateAsync = vi.fn()
        const setLastResult = vi.fn()
        const setConfirmOpen = vi.fn()
        const setSessionEdit = vi.fn()
        await performPublish({
            data: makeView(),
            sessionEdit: makeSessionEdit(),
            values: makeValues(),
            canPublish: true,
            gateBlocks: true,
            pausedOnSale: false,
            publishBlocker: { message: "复核策略未确定" },
            requestIdRef: { current: null },
            mutateAsync,
            setConfirmOpen,
            setLastResult,
            setSessionEdit,
        })
        expect(mutateAsync).not.toHaveBeenCalled()
        expect(setConfirmOpen).not.toHaveBeenCalled()
        expect(setLastResult).toHaveBeenCalledWith({
            status: "blocked",
            title: "提交被阻断",
            description: "复核策略未确定",
        })
    })

    it("is blocked without calling the API when there is no edit session", async () => {
        const mutateAsync = vi.fn()
        const setLastResult = vi.fn()
        await performPublish({
            data: makeView(),
            sessionEdit: null,
            values: makeValues(),
            canPublish: true,
            gateBlocks: false,
            pausedOnSale: false,
            publishBlocker: undefined,
            requestIdRef: { current: null },
            mutateAsync,
            setConfirmOpen: vi.fn(),
            setLastResult,
            setSessionEdit: vi.fn(),
        })
        expect(mutateAsync).not.toHaveBeenCalled()
        expect(setLastResult).not.toHaveBeenCalled()
    })

    it("builds the command from trimmed form values and reports success", async () => {
        const mutateAsync = vi
            .fn()
            .mockResolvedValue({
                status: "succeeded",
                operationId: "op_1",
                publicationId: "pub_1",
                revisionId: "rev_3",
                revisionNo: 3,
                deliveryId: "dlv_1",
                deliveryStatus: "PENDING_SEND",
                committedAt: "2026-01-03T00:00:00.000Z",
            } satisfies PublishRevisionResult)
        const setLastResult = vi.fn()
        const setConfirmOpen = vi.fn()
        const setSessionEdit = vi.fn()
        const requestIdRef = { current: null as string | null }
        await performPublish({
            data: makeView(),
            sessionEdit: makeSessionEdit(),
            values: makeValues(),
            canPublish: true,
            gateBlocks: false,
            pausedOnSale: false,
            publishBlocker: undefined,
            requestIdRef,
            mutateAsync,
            setConfirmOpen,
            setLastResult,
            setSessionEdit,
        })
        expect(mutateAsync).toHaveBeenCalledTimes(1)
        const command = mutateAsync.mock.calls[0][0]
        expect(command.publicationId).toBe("pub_1")
        expect(command.expectedObjectVersion).toBe("v1")
        expect(command.expectedPublishGateVersion).toBe("g1")
        expect(command.requestId).toMatch(/^w22-pub-pub_1-\d+$/)
        expect(command.content.name).toBe("示例商品")
        expect(command.content.salesRegion).toEqual(["华东", "华南"])
        expect(command.content.productCapabilities).toEqual([
            "能力A",
            "能力B",
        ])
        expect(command.content.validTo).toBeUndefined()
        expect(command.content.media).toEqual([
            {
                fileAssetId: "f_1",
                mediaRole: "MAIN",
                sortNo: 0,
                altText: "主图说明",
            },
        ])
        expect(setConfirmOpen).toHaveBeenCalledWith(false)
        expect(setSessionEdit).toHaveBeenCalledWith(null)
        expect(requestIdRef.current).toBeNull()
        expect(setLastResult).toHaveBeenCalledWith({
            status: "succeeded",
            title: "发布修订已提交，等待商城确认",
            description:
                "已形成新的发布版本并开始发送。商城确认前不会显示为「商城已生效」。",
            reference: "op_1",
            facts: [
                { label: "发布版本", value: "r3" },
                { label: "修订编号", value: "rev_3" },
                { label: "发送编号", value: "dlv_1" },
                { label: "发送状态", value: "待发送" },
            ],
        })
    })

    it("reuses the pending request id across attempts until success", async () => {
        const unknown: PublishRevisionResult = {
            status: "unknown",
            requestId: "w22-pub-pub_1-1",
            message: "结果未返回",
        }
        const mutateAsync = vi.fn().mockResolvedValue(unknown)
        const requestIdRef = { current: null as string | null }
        const base = {
            data: makeView(),
            sessionEdit: makeSessionEdit(),
            values: makeValues(),
            canPublish: true,
            gateBlocks: false,
            pausedOnSale: false,
            publishBlocker: undefined,
            requestIdRef,
            mutateAsync,
            setConfirmOpen: vi.fn(),
            setLastResult: vi.fn(),
            setSessionEdit: vi.fn(),
        }
        await performPublish(base)
        await performPublish(base)
        expect(mutateAsync).toHaveBeenCalledTimes(2)
        const firstId = mutateAsync.mock.calls[0][0].requestId
        const secondId = mutateAsync.mock.calls[1][0].requestId
        expect(firstId).toMatch(/^w22-pub-pub_1-\d+$/)
        expect(secondId).toBe(firstId)
        expect(requestIdRef.current).toBe(firstId)
        expect(base.setSessionEdit).not.toHaveBeenCalled()
    })

    it("reports an unknown result and keeps the session edit", async () => {
        const mutateAsync = vi.fn().mockResolvedValue({
            status: "unknown",
            requestId: "w22-pub-pub_1-1",
            message: "结果未返回，请勿重复提交",
        } satisfies PublishRevisionResult)
        const setLastResult = vi.fn()
        const setSessionEdit = vi.fn()
        await performPublish({
            data: makeView(),
            sessionEdit: makeSessionEdit(),
            values: makeValues(),
            canPublish: true,
            gateBlocks: false,
            pausedOnSale: false,
            publishBlocker: undefined,
            requestIdRef: { current: null },
            mutateAsync,
            setConfirmOpen: vi.fn(),
            setLastResult,
            setSessionEdit,
        })
        expect(setSessionEdit).not.toHaveBeenCalled()
        expect(setLastResult).toHaveBeenCalledWith({
            status: "unknown",
            title: "发布结果未知",
            description: "结果未返回，请勿重复提交",
            reference: "w22-pub-pub_1-1",
        })
    })

    it("reports a blocked result with the rejection code", async () => {
        const mutateAsync = vi.fn().mockResolvedValue({
            status: "blocked",
            code: "REVIEW_BLOCKED",
            message: "复核未通过",
        } satisfies PublishRevisionResult)
        const setLastResult = vi.fn()
        await performPublish({
            data: makeView(),
            sessionEdit: makeSessionEdit(),
            values: makeValues(),
            canPublish: true,
            gateBlocks: false,
            pausedOnSale: false,
            publishBlocker: undefined,
            requestIdRef: { current: null },
            mutateAsync,
            setConfirmOpen: vi.fn(),
            setLastResult,
            setSessionEdit: vi.fn(),
        })
        expect(setLastResult).toHaveBeenCalledWith({
            status: "blocked",
            title: "发布被阻断",
            description: "复核未通过",
            reference: "REVIEW_BLOCKED",
        })
    })

    it("is blocked when safety paused and sale status is ON_SALE", async () => {
        const mutateAsync = vi.fn()
        const setLastResult = vi.fn()
        await performPublish({
            data: makeView({ status: "SAFETY_PAUSED" }),
            sessionEdit: makeSessionEdit(),
            values: makeValues(),
            canPublish: true,
            gateBlocks: false,
            pausedOnSale: true,
            publishBlocker: undefined,
            requestIdRef: { current: null },
            mutateAsync,
            setConfirmOpen: vi.fn(),
            setLastResult,
            setSessionEdit: vi.fn(),
        })
        expect(mutateAsync).not.toHaveBeenCalled()
        expect(setLastResult).toHaveBeenCalledWith({
            status: "blocked",
            title: "提交被阻断",
            description: "当前状态不允许提交发布。",
        })
    })
})

describe("performPause", () => {
    const base = () => ({
        data: makeView(),
        pauseReason: "价格调整",
        mutateAsync: vi.fn(),
        setPauseOpen: vi.fn(),
        setPauseReason: vi.fn(),
        setLastResult: vi.fn(),
    })

    it("does nothing when the reason is empty or whitespace", async () => {
        const params = base()
        params.pauseReason = "   "
        await performPause(params)
        expect(params.mutateAsync).not.toHaveBeenCalled()
        expect(params.setPauseOpen).not.toHaveBeenCalled()
    })

    it("submits the trimmed reason and reports success", async () => {
        const params = base()
        params.pauseReason = "  价格调整  "
        params.mutateAsync.mockResolvedValue({
            status: "succeeded",
            revisionId: "rev_9",
            revisionNo: 9,
            deliveryId: "dlv_9",
            committedAt: "2026-01-03T00:00:00.000Z",
        } satisfies ManualPauseResult)
        await performPause(params)
        expect(params.mutateAsync).toHaveBeenCalledWith({
            publicationId: "pub_1",
            expectedObjectVersion: "v1",
            requestId: expect.stringMatching(/^w22-pause-\d+$/),
            reason: "价格调整",
        })
        expect(params.setPauseOpen).toHaveBeenCalledWith(false)
        expect(params.setPauseReason).toHaveBeenCalledWith("")
        expect(params.setLastResult).toHaveBeenCalledWith({
            status: "succeeded",
            title: "人工暂停修订已提交",
            description: "已形成暂停发布修订并进入发送。",
            facts: [
                { label: "发布版本", value: "r9" },
                { label: "发送编号", value: "dlv_9" },
            ],
        })
    })

    it("reports an unknown pause result", async () => {
        const params = base()
        params.mutateAsync.mockResolvedValue({
            status: "unknown",
            requestId: "w22-pause-1",
            message: "结果未返回",
        } satisfies ManualPauseResult)
        await performPause(params)
        expect(params.setPauseReason).not.toHaveBeenCalled()
        expect(params.setLastResult).toHaveBeenCalledWith({
            status: "unknown",
            title: "暂停结果未知",
            description: "结果未返回",
        })
    })

    it("reports a blocked pause result", async () => {
        const params = base()
        params.mutateAsync.mockResolvedValue({
            status: "blocked",
            code: "OBJECT_VERSION_CONFLICT",
            message: "数据已更新，请刷新后重试",
        } satisfies ManualPauseResult)
        await performPause(params)
        expect(params.setLastResult).toHaveBeenCalledWith({
            status: "blocked",
            title: "暂停被阻断",
            description: "数据已更新，请刷新后重试",
        })
    })
})

describe("performRetry", () => {
    it("retries a delivery and reports success", async () => {
        const mutateAsync = vi.fn().mockResolvedValue({
            status: "succeeded",
            deliveryId: "dlv_1",
            attemptCount: 3,
            deliveryStatus: "SENDING",
        } satisfies RetryDeliveryResult)
        const setLastResult = vi.fn()
        await performRetry({
            data: makeView(),
            deliveryId: "dlv_1",
            mutateAsync,
            setLastResult,
        })
        expect(mutateAsync).toHaveBeenCalledWith({
            publicationId: "pub_1",
            deliveryId: "dlv_1",
            requestId: expect.stringMatching(/^w22-retry-\d+$/),
        })
        expect(setLastResult).toHaveBeenCalledWith({
            status: "succeeded",
            title: "已发起重试发送",
            description: "继续发送，尝试次数 3。",
            facts: [
                { label: "发送编号", value: "dlv_1" },
                { label: "状态", value: "SENDING" },
            ],
        })
    })

    it("reports a blocked retry", async () => {
        const mutateAsync = vi.fn().mockResolvedValue({
            status: "blocked",
            code: "NOT_RETRYABLE",
            message: "该发送不可重试",
        } satisfies RetryDeliveryResult)
        const setLastResult = vi.fn()
        await performRetry({
            data: makeView(),
            deliveryId: "dlv_1",
            mutateAsync,
            setLastResult,
        })
        expect(setLastResult).toHaveBeenCalledWith({
            status: "blocked",
            title: "无法重试",
            description: "该发送不可重试",
        })
    })

    it("leaves the result untouched for unknown outcomes", async () => {
        const mutateAsync = vi.fn().mockResolvedValue({
            status: "unknown",
            requestId: "w22-retry-1",
            message: "结果未返回",
        } satisfies RetryDeliveryResult)
        const setLastResult = vi.fn() as (result: ResultState) => void
        await performRetry({
            data: makeView(),
            deliveryId: "dlv_1",
            mutateAsync,
            setLastResult,
        })
        expect(setLastResult).not.toHaveBeenCalled()
    })
})
