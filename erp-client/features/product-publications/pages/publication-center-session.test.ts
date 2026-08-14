import { beforeEach, describe, expect, it, vi } from "vitest"
import { act, renderHook } from "@testing-library/react"

import type {
    ProductPublicationView,
} from "@/features/product-publications/types"
import type {
    PublicationCenterFormValues,
} from "./publication-center-session"
import {
    usePublicationCenterForm,
    usePublicationCenterSession,
} from "./publication-center-session"

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
        currentAckedRevisionNo: 2,
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
                supplyPriceGross: "10.50",
                supplierMoq: "100",
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
            salesRegion: ["华东", "华南"],
            salesRegionLabel: "华东、华南",
            saleStatus: "ON_SALE",
            saleStatusLabel: "上架",
            productCapabilities: ["能力A", "能力B"],
            validFrom: "2026-01-01T00:00:00.000Z",
            contentHash: "hash_1",
            media: [
                {
                    fileAssetId: "f_1",
                    mediaRole: "MAIN",
                    sortNo: 0,
                    altText: "主图说明",
                    thumbnailUrl: "http://example.com/1.jpg",
                    securityScanStatus: "PASSED",
                },
            ],
            createdAt: "2026-01-01T00:00:00.000Z",
            createdBy: "张三",
        },
        revisions: [
            {
                revisionId: "rev_2",
                revisionNo: 2,
                saleStatus: "ON_SALE",
                saleStatusLabel: "上架",
                createdAt: "2026-01-01T00:00:00.000Z",
                createdBy: "张三",
                contentHash: "hash_1",
                deliverySummary: "已发送",
                isMallAcked: true,
                isLatest: true,
            },
        ],
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

const validValues: PublicationCenterFormValues = {
    name: "示例商品",
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
            mediaRole: "MAIN" as const,
            sortNo: 0,
            altText: "主图说明",
        },
    ],
    saleStatus: "ON_SALE" as const,
}

function renderSession() {
    const onSubmitRequest = vi.fn()
    const onCloseConfirm = vi.fn()
    const onStartEdit = vi.fn()
    const { result } = renderHook(() => {
        const form = usePublicationCenterForm({ onSubmitRequest })
        const session = usePublicationCenterSession({
            form,
            onCloseConfirm,
            onStartEdit,
        })
        return { form, session }
    })
    return { result, onSubmitRequest, onCloseConfirm, onStartEdit }
}

beforeEach(() => {
    vi.restoreAllMocks()
})

describe("usePublicationCenterSession", () => {
    it("starts empty with form defaults", () => {
        const { result } = renderSession()
        expect(result.current.session.sessionEdit).toBeNull()
        expect(result.current.session.dirty).toBe(false)
        const values = result.current.form.state.values
        expect(values.name).toBe("")
        expect(values.minimumPurchaseQuantity).toBe("1")
        expect(values.salesTaxRate).toBe("0.13")
        expect(values.saleStatus).toBe("ON_SALE")
        expect(values.media).toEqual([])
    })

    it("startPrepareRevision derives the edit and resets the form", () => {
        const { result, onStartEdit } = renderSession()
        const resetSpy = vi.spyOn(result.current.form, "reset")
        const data = makeView()
        act(() => {
            result.current.session.startPrepareRevision(data)
        })
        const edit = result.current.session.sessionEdit
        expect(edit).not.toBeNull()
        expect(edit?.baselineRevisionId).toBe("rev_2")
        expect(edit?.name).toBe("示例商品")
        expect(edit?.salesRegion).toEqual(["华东", "华南"])
        expect(edit?.productCapabilities).toEqual(["能力A", "能力B"])
        expect(edit?.media).toHaveLength(1)
        expect(edit?.media[0].altText).toBe("主图说明")
        expect(edit?.validFrom).toMatch(/^\d{4}-\d{2}-\d{2}T/)
        expect(result.current.session.dirty).toBe(true)
        expect(onStartEdit).toHaveBeenCalledTimes(1)
        expect(resetSpy).toHaveBeenCalledWith({
            name: "示例商品",
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
            validFrom: edit?.validFrom,
            validTo: "",
            media: [{ fileAssetId: "f_1", mediaRole: "MAIN", sortNo: 0, altText: "主图说明" }],
            saleStatus: "ON_SALE",
        })
    })

    it("keeps PAUSED sale status when preparing from a paused revision", () => {
        const { result } = renderSession()
        const data = makeView({
            selectedRevision: {
                ...makeView().selectedRevision,
                saleStatus: "PAUSED",
                saleStatusLabel: "暂停下单",
            },
        })
        const resetSpy = vi.spyOn(result.current.form, "reset")
        act(() => {
            result.current.session.startPrepareRevision(data)
        })
        expect(result.current.session.sessionEdit?.saleStatus).toBe("PAUSED")
        expect(resetSpy).toHaveBeenCalledWith(
            expect.objectContaining({ saleStatus: "PAUSED" }),
        )
    })

    it("registers beforeunload while dirty and removes it after discard", () => {
        const addSpy = vi.spyOn(window, "addEventListener")
        const removeSpy = vi.spyOn(window, "removeEventListener")
        const { result } = renderSession()
        act(() => {
            result.current.session.startPrepareRevision(makeView())
        })
        expect(addSpy).toHaveBeenCalledWith(
            "beforeunload",
            expect.any(Function),
        )
        vi.spyOn(window, "confirm").mockReturnValue(true)
        act(() => {
            result.current.session.discardSession()
        })
        expect(removeSpy).toHaveBeenCalledWith(
            "beforeunload",
            expect.any(Function),
        )
    })

    it("discardSession keeps the edit when the user cancels", () => {
        vi.spyOn(window, "confirm").mockReturnValue(false)
        const { result, onCloseConfirm } = renderSession()
        act(() => {
            result.current.session.startPrepareRevision(makeView())
        })
        act(() => {
            result.current.session.discardSession()
        })
        expect(result.current.session.sessionEdit).not.toBeNull()
        expect(onCloseConfirm).not.toHaveBeenCalled()
    })

    it("discardSession clears the edit and closes the confirm dialog", () => {
        vi.spyOn(window, "confirm").mockReturnValue(true)
        const { result, onCloseConfirm } = renderSession()
        act(() => {
            result.current.session.startPrepareRevision(makeView())
        })
        act(() => {
            result.current.session.discardSession()
        })
        expect(result.current.session.sessionEdit).toBeNull()
        expect(onCloseConfirm).toHaveBeenCalledTimes(1)
    })

    it("discardSession without an active edit closes the confirm dialog without asking", () => {
        const confirmSpy = vi.spyOn(window, "confirm")
        const { result, onCloseConfirm } = renderSession()
        act(() => {
            result.current.session.discardSession()
        })
        expect(confirmSpy).not.toHaveBeenCalled()
        expect(onCloseConfirm).toHaveBeenCalledTimes(1)
    })
})

describe("usePublicationCenterForm", () => {
    it("requests confirmation when a valid form is submitted", async () => {
        const { result, onSubmitRequest } = renderSession()
        act(() => {
            result.current.form.reset(validValues)
        })
        await act(async () => {
            await result.current.form.handleSubmit()
        })
        expect(onSubmitRequest).toHaveBeenCalledTimes(1)
    })

    it("does not request confirmation when required fields are empty", async () => {
        const { result, onSubmitRequest } = renderSession()
        await act(async () => {
            await result.current.form.handleSubmit()
        })
        expect(onSubmitRequest).not.toHaveBeenCalled()
        const nameField = result.current.form.state.fieldMeta.name
        expect(
            nameField?.errors.map((error) =>
                typeof error === "string" ? error : error.message,
            ),
        ).toContain("请填写展示名称")
    })
})
