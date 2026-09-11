import { describe, expect, it } from "vitest"

import {
    backgroundJobDomainLabel,
    formatJobDateTime,
    isBackgroundJobAdmin,
    isJobActive,
    jobProgressStatus,
} from "@/features/background-jobs/labels"

describe("background job labels", () => {
    it("maps progress status to shared job status", () => {
        expect(jobProgressStatus("pending")).toBe("queued")
        expect(jobProgressStatus("running")).toBe("running")
        expect(jobProgressStatus("partially_succeeded")).toBe("partial")
        expect(jobProgressStatus("succeeded")).toBe("succeeded")
        expect(jobProgressStatus("failed")).toBe("failed")
        expect(jobProgressStatus("cancelled")).toBe("frozen")
    })

    it("treats pending, running and partial tasks as active", () => {
        expect(isJobActive("pending")).toBe(true)
        expect(isJobActive("running")).toBe(true)
        expect(isJobActive("partially_succeeded")).toBe(true)
        expect(isJobActive("partially_succeeded", 1700000000)).toBe(false)
        expect(isJobActive("succeeded")).toBe(false)
        expect(isJobActive("failed")).toBe(false)
        expect(isJobActive("cancelled")).toBe(false)
    })

    it("labels business types and falls back to the job type", () => {
        expect(backgroundJobDomainLabel("PRODUCT_IMPORT", "import")).toBe(
            "商品导入",
        )
        expect(backgroundJobDomainLabel("SALES_ORDER_EXPORT", "export")).toBe(
            "销售单导出",
        )
        expect(backgroundJobDomainLabel("SUPPLIER_IMPORT", "import")).toBe(
            "供应商导入",
        )
        expect(backgroundJobDomainLabel(null, "export")).toBe("导出")
        expect(backgroundJobDomainLabel(null, null)).toBe("后台任务")
        expect(backgroundJobDomainLabel("UNKNOWN_DOMAIN", "import")).toBe(
            "UNKNOWN_DOMAIN",
        )
    })

    it("formats timestamps and guards empty values", () => {
        expect(formatJobDateTime(null)).toBe("—")
        expect(formatJobDateTime(1_700_000_000)).toContain("2023")
    })

    it("grants full task visibility only to admin roles", () => {
        expect(isBackgroundJobAdmin(["role-root"])).toBe(true)
        expect(isBackgroundJobAdmin(["role-sysadmin"])).toBe(true)
        expect(isBackgroundJobAdmin(["role-sales"])).toBe(false)
        expect(isBackgroundJobAdmin([])).toBe(false)
        expect(isBackgroundJobAdmin(null)).toBe(false)
    })
})
