import { describe, expect, it, vi } from "vitest"

import { makeQueryClient } from "@/lib/query-client"

describe("makeQueryClient mutation error policy", () => {
    it("reports unhandled mutation errors once", async () => {
        const onMutationError = vi.fn()
        const client = makeQueryClient(onMutationError)
        const error = new Error("保存失败")
        const mutation = client.getMutationCache().build(client, {
            mutationFn: async () => {
                throw error
            },
        })

        await expect(mutation.execute(undefined)).rejects.toBe(error)

        expect(onMutationError).toHaveBeenCalledTimes(1)
        expect(onMutationError).toHaveBeenCalledWith(error)
    })

    it("allows an inline error flow to suppress the global toast", async () => {
        const onMutationError = vi.fn()
        const client = makeQueryClient(onMutationError)
        const error = new Error("结果待确认")
        const mutation = client.getMutationCache().build(client, {
            mutationFn: async () => {
                throw error
            },
            meta: { suppressErrorToast: true },
        })

        await expect(mutation.execute(undefined)).rejects.toBe(error)

        expect(onMutationError).not.toHaveBeenCalled()
    })
})
