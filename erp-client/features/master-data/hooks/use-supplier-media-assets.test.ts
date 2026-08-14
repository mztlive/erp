import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useSupplierMediaAssets } from "./use-supplier-media-assets"
import { createSupplierEditorDefaults } from "@/features/master-data/lib/supplier-editor-model"
import type { MasterDataCenterView } from "@/features/master-data/types"

const fileMocks = vi.hoisted(() => ({
    uploadFileAssetImage: vi.fn(),
}))

vi.mock("@/features/file-assets/api", () => ({
    uploadFileAssetImage: fileMocks.uploadFileAssetImage,
}))

function makeCenter(
    mediaAssets: MasterDataCenterView["mediaAssets"],
): MasterDataCenterView {
    return { mediaAssets } as MasterDataCenterView
}

beforeEach(() => {
    fileMocks.uploadFileAssetImage.mockReset()
})

describe("useSupplierMediaAssets", () => {
    it("returns empty lookups without data", () => {
        const { result } = renderHook(() => useSupplierMediaAssets(undefined))
        expect(result.current.mediaUrlsFor("qualification")).toEqual({})
        expect(result.current.mediaAssetIdsFor("qualification")).toEqual({})
    })

    it("maps stored asset ids and urls per field", () => {
        const { result } = renderHook(() =>
            useSupplierMediaAssets(
                makeCenter({
                    qualification: [
                        {
                            fileName: "a.pdf",
                            assetId: "asset-a",
                            url: "https://cdn/a.pdf",
                        },
                    ],
                }),
            ),
        )
        expect(result.current.mediaUrlsFor("qualification")).toEqual({
            "a.pdf": "https://cdn/a.pdf",
        })
        expect(result.current.mediaAssetIdsFor("qualification")).toEqual({
            "a.pdf": "asset-a",
        })
        expect(result.current.mediaAssetIdsFor("contractFile")).toEqual({})
    })

    it("uploads pending files once and reuses uploaded assets on retry", async () => {
        fileMocks.uploadFileAssetImage.mockImplementation(
            async (_file: File, _kind: string, sensitivity: string) => ({
                fileAssetId: `asset-${sensitivity}`,
                url: "https://cdn/new.pdf",
            }),
        )
        const { result } = renderHook(() =>
            useSupplierMediaAssets(
                makeCenter({
                    qualification: [
                        {
                            fileName: "a.pdf",
                            assetId: "asset-a",
                            url: "https://cdn/a.pdf",
                        },
                    ],
                }),
            ),
        )
        act(() => {
            result.current.rememberMediaFiles([new File(["x"], "new.pdf")])
        })

        const values = {
            ...createSupplierEditorDefaults(false),
            qualification: "a.pdf,new.pdf",
        }
        let maps: Record<string, Record<string, string>> = {}
        await act(async () => {
            maps = await result.current.resolvePendingMedia(values)
        })
        expect(maps.qualification).toEqual({
            "a.pdf": "asset-a",
            "new.pdf": "asset-sensitive",
        })
        expect(fileMocks.uploadFileAssetImage).toHaveBeenCalledTimes(1)
        expect(fileMocks.uploadFileAssetImage).toHaveBeenCalledWith(
            expect.any(File),
            "attachment",
            "sensitive",
        )

        await act(async () => {
            maps = await result.current.resolvePendingMedia(values)
        })
        expect(maps.qualification["new.pdf"]).toBe("asset-sensitive")
        expect(fileMocks.uploadFileAssetImage).toHaveBeenCalledTimes(1)
    })

    it("uses highly sensitive class for legal person id cards", async () => {
        fileMocks.uploadFileAssetImage.mockResolvedValue({
            fileAssetId: "asset-id",
            url: "https://cdn/id.png",
        })
        const { result } = renderHook(() =>
            useSupplierMediaAssets(undefined),
        )
        act(() => {
            result.current.rememberMediaFiles([new File(["x"], "id.png")])
        })
        const values = {
            ...createSupplierEditorDefaults(false),
            legalPersonIdCard: "id.png",
        }
        await act(async () => {
            await result.current.resolvePendingMedia(values)
        })
        expect(fileMocks.uploadFileAssetImage).toHaveBeenCalledWith(
            expect.any(File),
            "attachment",
            "highly_sensitive",
        )
    })

    it("skips names without a known asset or pending file", async () => {
        const { result } = renderHook(() =>
            useSupplierMediaAssets(undefined),
        )
        const values = {
            ...createSupplierEditorDefaults(false),
            contractFile: "missing.pdf",
        }
        let maps: Record<string, Record<string, string>> = {}
        await act(async () => {
            maps = await result.current.resolvePendingMedia(values)
        })
        expect(maps.contractFile).toEqual({})
        expect(fileMocks.uploadFileAssetImage).not.toHaveBeenCalled()
    })
})
