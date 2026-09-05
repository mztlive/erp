import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, expect, it } from "vitest"
import { emptyProductFields } from "../lib/product-model"
import { useProductUploads } from "./use-product-uploads"

afterEach(cleanup)

it("keeps pending main images attached to their SKUs after reordering and removal", () => {
    const { result } = renderHook(() => useProductUploads())
    const red = new File(["red"], "same-name.png", { type: "image/png" })
    const blue = new File(["blue"], "same-name.png", { type: "image/png" })
    act(() => {
        result.current.rememberSkuFile("blob:red", red)
        result.current.rememberSkuFile("blob:blue", blue)
    })
    const fields = { ...emptyProductFields() }
    const base = fields.skus[0]
    fields.skus = [
        {
            ...base,
            skuNo: "blue",
            mainImage: "same-name.png",
            mainImagePreviewUrl: "blob:blue",
        },
        {
            ...base,
            skuNo: "red",
            mainImage: "same-name.png",
            mainImagePreviewUrl: "blob:red",
        },
    ]
    const reordered = result.current.preparePendingUploads(fields)
    expect(reordered.pendingAssetUploads.map((item) => item.file)).toEqual([
        blue,
        red,
    ])
    fields.skus = [fields.skus[1]]
    expect(
        result.current.preparePendingUploads(fields).pendingAssetUploads[0]
            .file,
    ).toBe(red)
})
