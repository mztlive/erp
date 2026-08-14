import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act, renderHook } from '@testing-library/react'

import * as fileAssetsApi from '@/features/file-assets/api'
import { emptyProductFields } from '@/features/master-data/lib/product-model'
import { useProductUploads } from './use-product-uploads'

vi.mock('@/features/file-assets/api', () => ({
    uploadFileAssetImage: vi.fn(),
}))

const mockedUpload = vi.mocked(fileAssetsApi.uploadFileAssetImage)

function makeFile(name: string): File {
    return new File(['x'], name, { type: 'image/png' })
}

describe('useProductUploads', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('remembers pending SPU files by file name and SKU files by row index', () => {
        const { result } = renderHook(() => useProductUploads())

        const carousel = makeFile('a.png')
        act(() => result.current.rememberPendingFiles([carousel]))
        act(() => result.current.rememberSkuFile(2, makeFile('main.png')))

        expect(result.current.pendingFilesRef.current.get('a.png')).toBe(
            carousel,
        )
        expect(
            result.current.pendingSkuFilesRef.current.get(2)?.name,
        ).toBe('main.png')
    })

    it('uploads blob-previewed carousel images and fills resolved asset ids', async () => {
        const { result } = renderHook(() => useProductUploads())
        const fields = {
            ...emptyProductFields(),
            carouselImages: ['a.png'],
            carouselPreviewUrls: { 'a.png': 'blob:local-a' },
        }
        const file = makeFile('a.png')
        act(() => result.current.rememberPendingFiles([file]))
        mockedUpload.mockResolvedValue({
            url: 'https://cdn/uploaded-a.png',
            fileAssetId: 'fa-1',
        })

        let resolved: Awaited<
            ReturnType<typeof result.current.resolvePendingUploads>
        >
        await act(async () => {
            resolved = await result.current.resolvePendingUploads(fields)
        })

        expect(mockedUpload).toHaveBeenCalledWith(file)
        expect(resolved!.carouselPreviewUrls['a.png']).toBe(
            'https://cdn/uploaded-a.png',
        )
        expect(resolved!.carouselFileAssetIds['a.png']).toBe('fa-1')
    })

    it('keeps non-blob preview urls untouched without uploading', async () => {
        const { result } = renderHook(() => useProductUploads())
        const fields = {
            ...emptyProductFields(),
            detailImages: ['b.png'],
            detailPreviewUrls: { 'b.png': 'https://cdn/b.png' },
            detailFileAssetIds: { 'b.png': 'fa-9' },
        }

        const resolved = await result.current.resolvePendingUploads(fields)

        expect(mockedUpload).not.toHaveBeenCalled()
        expect(resolved.detailPreviewUrls['b.png']).toBe('https://cdn/b.png')
        expect(resolved.detailFileAssetIds['b.png']).toBe('fa-9')
    })

    it('rejects when a blob preview has no remembered file content', async () => {
        const { result } = renderHook(() => useProductUploads())
        const fields = {
            ...emptyProductFields(),
            carouselImages: ['missing.png'],
            carouselPreviewUrls: { 'missing.png': 'blob:missing' },
        }

        await expect(
            result.current.resolvePendingUploads(fields),
        ).rejects.toThrow('找不到待上传图片「missing.png」')
    })

    it('uploads blob-previewed SKU main images by row index', async () => {
        const { result } = renderHook(() => useProductUploads())
        const base = emptyProductFields()
        const fields = {
            ...base,
            skus: base.skus.map((sku, index) =>
                index === 0
                    ? {
                          ...sku,
                          mainImage: 'main.png',
                          mainImagePreviewUrl: 'blob:local-main',
                      }
                    : sku,
            ),
        }
        const file = makeFile('main.png')
        act(() => result.current.rememberSkuFile(0, file))
        mockedUpload.mockResolvedValue({
            url: 'https://cdn/uploaded-main.png',
            fileAssetId: 'fa-2',
        })

        const resolved = await result.current.resolvePendingUploads(fields)

        expect(mockedUpload).toHaveBeenCalledWith(file)
        expect(resolved.skus[0].mainImagePreviewUrl).toBe(
            'https://cdn/uploaded-main.png',
        )
        expect(resolved.skus[0].mainImageAssetId).toBe('fa-2')
    })

    it('skips SKU rows without a blob preview', async () => {
        const { result } = renderHook(() => useProductUploads())
        const base = emptyProductFields()
        const fields = {
            ...base,
            skus: base.skus.map((sku, index) =>
                index === 0
                    ? {
                          ...sku,
                          mainImage: 'remote.png',
                          mainImagePreviewUrl: 'https://cdn/remote.png',
                      }
                    : sku,
            ),
        }

        const resolved = await result.current.resolvePendingUploads(fields)

        expect(mockedUpload).not.toHaveBeenCalled()
        expect(resolved.skus[0].mainImagePreviewUrl).toBe(
            'https://cdn/remote.png',
        )
    })
})
