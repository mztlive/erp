import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { FileUpload } from "@/components/ui/file-upload"

describe("FileUpload", () => {
    const createObjectURL = vi.fn(() => "blob:service-evidence")
    const revokeObjectURL = vi.fn()

    beforeEach(() => {
        createObjectURL.mockClear()
        revokeObjectURL.mockClear()
        Object.defineProperty(URL, "createObjectURL", {
            configurable: true,
            value: createObjectURL,
        })
        Object.defineProperty(URL, "revokeObjectURL", {
            configurable: true,
            value: revokeObjectURL,
        })
    })

    afterEach(() => {
        cleanup()
    })

    it("受控文件清空后移除待上传预览并释放 blob URL", async () => {
        const file = new File(["image"], "site.jpg", { type: "image/jpeg" })
        const props = {
            onFilesSelected: vi.fn(),
            previewSelectedImage: true,
        }
        const { rerender } = render(
            <FileUpload {...props} selectedImageFile={file} />,
        )

        expect(await screen.findByAltText("site.jpg")).toBeTruthy()
        rerender(<FileUpload {...props} selectedImageFile={null} />)

        expect(screen.queryByAltText("site.jpg")).toBeNull()
        expect(revokeObjectURL).toHaveBeenCalledWith("blob:service-evidence")
    })
})
