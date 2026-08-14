import { describe, it, expect } from 'vitest'

import { contractPdfError } from '@/features/contracts/lib/pdf'

function pdfFile(
    name = 'contract.pdf',
    type = 'application/pdf',
    size = 1024,
): File {
    return new File([new ArrayBuffer(size)], name, { type })
}

describe('contractPdfError', () => {
    it('requires a file', () => {
        expect(contractPdfError(null)).toBe('请上传合同 PDF')
    })

    it('accepts a valid PDF', () => {
        expect(contractPdfError(pdfFile())).toBeNull()
    })

    it('accepts an empty MIME type when the name ends with .pdf', () => {
        expect(contractPdfError(pdfFile('a.PDF', ''))).toBeNull()
    })

    it('rejects non-PDF file names', () => {
        expect(contractPdfError(pdfFile('contract.docx'))).toBe(
            '合同只支持 PDF 文件',
        )
    })

    it('rejects non-PDF MIME types', () => {
        expect(
            contractPdfError(pdfFile('contract.pdf', 'text/plain')),
        ).toBe('合同只支持 PDF 文件')
    })

    it('rejects empty files', () => {
        expect(contractPdfError(pdfFile('contract.pdf', 'application/pdf', 0))).toBe(
            '合同 PDF 不能为空文件',
        )
    })

    it('rejects files larger than 20 MB and accepts the boundary', () => {
        const boundary = 20 * 1024 * 1024
        expect(
            contractPdfError(pdfFile('big.pdf', 'application/pdf', boundary)),
        ).toBeNull()
        expect(
            contractPdfError(
                pdfFile('big.pdf', 'application/pdf', boundary + 1),
            ),
        ).toBe('合同 PDF 不能超过 20 MB')
    })
})
