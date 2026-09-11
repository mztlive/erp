import { describe, expect, it } from "vitest"

import {
    exportDomainLabel,
    exportProgressStatus,
    formatExportDateTime,
    isExportJobActive,
} from "@/features/export-tasks/labels"

describe("export task labels", () => {
    it("maps progress status to shared job status", () => {
        expect(exportProgressStatus("pending")).toBe("queued")
        expect(exportProgressStatus("running")).toBe("running")
        expect(exportProgressStatus("partially_succeeded")).toBe("partial")
        expect(exportProgressStatus("succeeded")).toBe("succeeded")
        expect(exportProgressStatus("failed")).toBe("failed")
        expect(exportProgressStatus("cancelled")).toBe("frozen")
    })

    it("treats pending and running tasks as active", () => {
        expect(isExportJobActive("pending")).toBe(true)
        expect(isExportJobActive("running")).toBe(true)
        expect(isExportJobActive("partially_succeeded")).toBe(true)
        expect(isExportJobActive("succeeded")).toBe(false)
        expect(isExportJobActive("failed")).toBe(false)
        expect(isExportJobActive("cancelled")).toBe(false)
    })

    it("labels known export domains and falls back safely", () => {
        expect(exportDomainLabel("SALES_ORDER_EXPORT")).toBe("销售单导出")
        expect(exportDomainLabel(null)).toBe("通用导出")
        expect(exportDomainLabel("UNKNOWN_EXPORT")).toBe("UNKNOWN_EXPORT")
    })

    it("formats timestamps and guards empty values", () => {
        expect(formatExportDateTime(null)).toBe("—")
        expect(formatExportDateTime(1_700_000_000)).toContain("2023")
    })
})
